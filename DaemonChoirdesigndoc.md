

```markdown
# DAEMON CHOIR — Production Engineering Specification

---

## DOCUMENT GOVERNANCE

| Field            | Value                                              |
|------------------|----------------------------------------------------|
| **Title**        | DAEMON CHOIR — System Design & Engineering Spec    |
| **Version**      | 2.0.0                                              |
| **Status**       | 🟡 In Review → promote to ✅ Approved before merge  |
| **Owner**        | @knarayanareddy                                    |
| **Reviewers**    | (assign: systems lead, security lead, audio lead)  |
| **Last Updated** | 2025-06-12                                         |
| **Canonical Sources** | See §13 — Canonicalization Map               |

### Change Log

| Version | Date       | Author           | Summary                                      |
|---------|------------|------------------|----------------------------------------------|
| 1.0.0   | (original) | @knarayanareddy  | Initial technical vision + implementation plan |
| 2.0.0   | 2025-06-12 | @knarayanareddy  | Full restructure to production-ready spec: governance, API contracts, threat model, PRR section, ADRs |

### Canonicalization Rules (CRITICAL — read before editing)

> These rules exist to prevent spec drift. When in doubt, the canonical
> source wins. All other descriptions are commentary only.

| Artifact                        | Canonical Source                            | Other references are... |
|---------------------------------|---------------------------------------------|-------------------------|
| Control API contract            | `docs/api/control-api.openapi.yaml`         | Illustrative only        |
| Config schema                   | `docs/schema/daemon-choir.config.schema.json` | Illustrative only      |
| Shared event structs            | `crates/daemon-choir-common/src/events.rs`  | Illustrative only        |
| Performance budget targets      | §8 of this document                         | Canonical                |
| Security invariants             | §9 of this document                         | Canonical                |
| Error handling policy           | §7.3 of this document                       | Canonical                |
| Privacy commitments             | §10 of this document                        | Canonical                |

---

## TABLE OF CONTENTS

1. [Overview](#1-overview)
2. [Goals, Non-Goals & Constraints](#2-goals-non-goals--constraints)
3. [System Context & Trust Boundaries](#3-system-context--trust-boundaries)
4. [Architecture](#4-architecture)
5. [Component Specifications](#5-component-specifications)
6. [Control API Contract](#6-control-api-contract)
7. [Data Model & Storage](#7-data-model--storage)
8. [Non-Functional Requirements](#8-non-functional-requirements)
9. [Security Model & Threat Analysis](#9-security-model--threat-analysis)
10. [Privacy-by-Design](#10-privacy-by-design)
11. [Operability — Production Readiness Review (PRR)](#11-operability--production-readiness-review-prr)
12. [Testing & Verification](#12-testing--verification)
13. [Rollout & Installation](#13-rollout--installation)
14. [Architectural Decision Records (ADRs)](#14-architectural-decision-records-adrs)
15. [Open Questions & Deferred Work](#15-open-questions--deferred-work)
16. [Appendix A — Glossary](#appendix-a--glossary)
17. [Appendix B — Kernel Compatibility Matrix](#appendix-b--kernel-compatibility-matrix)
18. [Appendix C — Platform Support Matrix](#appendix-c--platform-support-matrix)

---

## 1. Overview

### 1.1 What Is DAEMON CHOIR?

DAEMON CHOIR is a **local-first, privacy-preserving system-state-to-sound mapping daemon** for Linux. It observes kernel-level system telemetry (CPU scheduling, memory pressure, network I/O, process lifecycle) via eBPF ring buffers and translates that telemetry — in real time — into generative audio and visual output via the Open Sound Control (OSC) protocol.

The system runs entirely on the local machine with **no cloud dependencies, no telemetry exfiltration, and no persistent PII capture.** It is designed for:

- **Developers and power users** who want an ambient, auditory awareness layer over their machine's runtime behavior.
- **Live coders and audiovisual performers** who want to drive generative audio/visuals from real system state.
- **Systems researchers** who want a low-overhead, human-perceptible signal of kernel activity patterns.

### 1.2 One-Line Invariant

> *The audio output MUST NEVER be silenced, corrupted, or interrupted by internal metric collection or processing errors. Audio continuity is the highest-priority invariant in this system.*

This invariant governs all error handling, fallback, and degradation decisions throughout this spec.

### 1.3 Core Data Flow (Summary)

```
┌─────────────────────────────────────────────────────────────────────┐
│  KERNEL SPACE                                                        │
│  eBPF programs (attach to tracepoints / kprobes)                     │
│  → perf_event ring buffers (kernel ≥ 5.8)                            │
└───────────────────────────┬─────────────────────────────────────────┘
                            │ ring buffer reads (Aya, async, polling)
┌───────────────────────────▼─────────────────────────────────────────┐
│  USER SPACE — CONDUCTOR (core daemon process)                        │
│  ┌──────────────┐   ┌──────────────┐   ┌───────────────────────┐    │
│  │ Probe Manager│   │ Ring Buffer  │   │ Aggregation Engine    │    │
│  │ (load/unload │──▶│ Consumer     │──▶│ (windowed snapshots,  │    │
│  │  eBPF progs) │   │ (per-probe)  │   │  metric normalization)│    │
│  └──────────────┘   └──────────────┘   └──────────┬────────────┘    │
│                                                    │                 │
│  ┌─────────────────────────────────────────────────▼──────────────┐ │
│  │ Mapping Engine                                                  │ │
│  │ (metric values → OSC parameter space, user-configurable rules) │ │
│  └─────────────────────────────────────────────────┬──────────────┘ │
│                                                    │                 │
│  ┌─────────────────────────────────────────────────▼──────────────┐ │
│  │ OSC Dispatcher                                                  │ │
│  │ → localhost audio backend (SuperCollider, Pure Data, etc.)      │ │
│  │ → localhost visual backend (optional, via OSC)                  │ │
│  └─────────────────────────────────────────────────────────────────┘ │
│                                                                       │
│  ┌─────────────────────────────────────────────────────────────────┐ │
│  │ Control Daemon (Axum, localhost:9876)                            │ │
│  │ → REST API for TUI, config reloads, voice/state inspection       │ │
│  └─────────────────────────────────────────────────────────────────┘ │
└──────────────────────────────────────────────────────────────────────┘
```

---

## 2. Goals, Non-Goals & Constraints

### 2.1 Goals

| ID   | Goal                                                                                  | Priority |
|------|---------------------------------------------------------------------------------------|----------|
| G-01 | Map live kernel telemetry to generative audio/visual output via OSC in real time      | P0       |
| G-02 | Operate entirely local; zero cloud dependency, zero external network calls            | P0       |
| G-03 | Never collect, log, or store sensitive process data (see §10)                         | P0       |
| G-04 | Maintain audio output continuity under all internal error conditions                  | P0       |
| G-05 | Expose a stable, versioned control API for TUI and external integrations              | P1       |
| G-06 | Support user-configurable mapping rules without daemon restart (hot reload)           | P1       |
| G-07 | Run as an unprivileged user with minimal granted capabilities (see §9)                | P1       |
| G-08 | Support multiple concurrent audio/visual backends via OSC fan-out                    | P2       |
| G-09 | Provide an installable distribution (signed binary, systemd unit, Makefile target)   | P2       |
| G-10 | Support performance profiling mode: emit internal latency metrics via OSC             | P2       |

### 2.2 Non-Goals

| ID    | Non-Goal                                                                             |
|-------|--------------------------------------------------------------------------------------|
| NG-01 | Remote monitoring or dashboarding (cloud, network, SaaS — never)                    |
| NG-02 | Recording or replaying audio output (audio backend's responsibility)                 |
| NG-03 | Acting as an audio synthesis engine (synthesis is delegated to audio backends)       |
| NG-04 | Monitoring non-local machines (single-host scope)                                    |
| NG-05 | Windows or macOS support (Linux-only; eBPF is Linux-specific)                        |
| NG-06 | Exposing any API over non-loopback interfaces in default config                      |
| NG-07 | Supporting kernel versions below 5.8 (ring buffer API requirement)                   |

### 2.3 Constraints

| ID   | Constraint                                                                                    |
|------|-----------------------------------------------------------------------------------------------|
| C-01 | MUST run on Linux kernel ≥ 5.8 (BPF ring buffer API)                                         |
| C-02 | MUST NOT run as root in steady state; MUST use capability model (§9.3)                        |
| C-03 | MUST NOT use more than 2% CPU (Conductor) or 80 MB RAM in steady state (§8.1)                |
| C-04 | MUST complete ring buffer → OSC dispatch in ≤ 20ms end-to-end (§8.2)                         |
| C-05 | All persisted data MUST be local to user home directory; no system-wide writes except install |
| C-06 | Config and schema changes MUST maintain backward compatibility within a major version         |

---

## 3. System Context & Trust Boundaries

### 3.1 Context Diagram

```
┌──────────────────────────────────────────────────────────────────────────┐
│  LOCAL MACHINE (single trust boundary — no external network)             │
│                                                                           │
│  ┌──────────────┐        eBPF ring buffers       ┌────────────────────┐  │
│  │  KERNEL      │ ◀──── read (unprivileged+cap) ──│                    │  │
│  │  (tracepoints│                                 │  DAEMON CHOIR      │  │
│  │   kprobes    │                                 │  (Conductor)       │  │
│  │   perf events│                                 │                    │  │
│  └──────────────┘                                 │  localhost:9876    │  │
│                                                   └─────────┬──────────┘  │
│                                OSC (UDP, localhost)         │              │
│          ┌──────────────────────────────┐                   │              │
│          │  AUDIO BACKEND               │◀──────────────────┤              │
│          │  (SuperCollider / PD / etc.) │                   │              │
│          └──────────────────────────────┘                   │              │
│                                                             │              │
│          ┌──────────────────────────────┐    REST           │              │
│          │  TUI / Config UI             │◀──────────────────┘              │
│          │  (separate process,          │                                   │
│          │   same user session)         │                                   │
│          └──────────────────────────────┘                                   │
│                                                                             │
│          ┌──────────────────────────────┐                                   │
│          │  VISUAL BACKEND (optional)   │◀── OSC (UDP, localhost)           │
│          │  (Processing, Hydra, etc.)   │                                   │
│          └──────────────────────────────┘                                   │
└──────────────────────────────────────────────────────────────────────────┘
                      ↕ NO external network calls. Ever.
```

### 3.2 Trust Boundary Rules

| Rule | Description |
|------|-------------|
| TB-01 | All inter-process communication is loopback-only (127.0.0.1). No LAN/WAN exposure in default config. |
| TB-02 | The control API (port 9876) MUST bind to 127.0.0.1 only. Binding to 0.0.0.0 is a misconfiguration and MUST be rejected at startup. |
| TB-03 | eBPF programs are read-only observers; they MUST NOT modify kernel data structures. |
| TB-04 | The Conductor process MUST drop all capabilities after eBPF program load (see §9.3). |
| TB-05 | Audio and visual backends are out-of-trust-boundary; they are treated as untrusted consumers of OSC output. |

### 3.3 External Dependency Inventory

| Dependency       | Type         | Purpose                          | Network? | Trust Level  |
|------------------|--------------|----------------------------------|----------|--------------|
| Linux kernel     | System       | eBPF execution environment       | No       | Trusted      |
| libbpf / Aya     | Library      | eBPF program loading (userspace) | No       | Trusted      |
| Axum             | Library      | Control HTTP server              | No       | Trusted      |
| Tokio            | Library      | Async runtime                    | No       | Trusted      |
| SQLite (via rusqlite) | Library | Local event log / state store    | No       | Trusted      |
| rosc / nannou-osc | Library     | OSC message encoding/dispatch    | No       | Trusted      |
| SuperCollider / PD | External process | Audio synthesis (user-chosen) | No  | Untrusted consumer |

---

## 4. Architecture

### 4.1 Crate Structure

```
daemon-choir/
├── Cargo.toml                    # workspace
├── crates/
│   ├── daemon-choir-common/      # shared types: events, metrics, config schema
│   │   └── src/
│   │       ├── events.rs         # CANONICAL event structs (see §13)
│   │       ├── metrics.rs        # metric value types + normalization
│   │       └── config.rs         # config type definitions
│   ├── conductor/                # main daemon binary
│   │   └── src/
│   │       ├── main.rs
│   │       ├── probe_manager.rs  # eBPF load/unload lifecycle
│   │       ├── ringbuf.rs        # ring buffer consumer tasks
│   │       ├── aggregator.rs     # windowed snapshot aggregation
│   │       ├── mapping.rs        # metric → OSC parameter mapping engine
│   │       ├── osc.rs            # OSC dispatcher + fan-out
│   │       └── control_api.rs    # Axum server + route handlers
│   ├── ebpf-programs/            # eBPF C/Rust source (compiled separately)
│   │   ├── cpu_sched.bpf.c
│   │   ├── mem_pressure.bpf.c
│   │   ├── net_io.bpf.c
│   │   └── proc_lifecycle.bpf.c
│   └── daemon-choir-tui/         # optional TUI client
│       └── src/main.rs
├── docs/
│   ├── api/
│   │   └── control-api.openapi.yaml   # CANONICAL API spec (see §6, §13)
│   ├── schema/
│   │   └── daemon-choir.config.schema.json  # CANONICAL config schema (see §7.2, §13)
│   └── adrs/                      # Architectural Decision Records (see §14)
├── scripts/
│   ├── install.sh
│   └── uninstall.sh
├── Makefile
└── systemd/
    └── daemon-choir.service
```

### 4.2 Component Responsibilities

| Component           | Responsibility                                                                 | Failure Mode |
|---------------------|--------------------------------------------------------------------------------|--------------|
| **Probe Manager**   | Load eBPF programs at startup; unload on shutdown/signal; hot-reload on SIGHUP | Non-fatal: log + continue with loaded probes |
| **Ring Buffer Consumer** | Poll per-probe ring buffers; deserialize raw events; forward to aggregator | Non-fatal: drop event + increment loss counter; NEVER block audio path |
| **Aggregation Engine** | Apply windowed (configurable, default 50ms) snapshot averaging; normalize values 0.0–1.0 | Non-fatal: emit last-known-good snapshot |
| **Mapping Engine**  | Apply user config rules: metric → OSC address + parameter transformation      | Non-fatal: emit no-op OSC; reload defaults |
| **OSC Dispatcher**  | UDP fan-out to configured backends; fire-and-forget                            | Non-fatal: log unreachable backend; continue |
| **Control API**     | Serve REST API on localhost:9876; handle config reload, state queries          | Non-fatal: API down ≠ audio down; isolated failure domain |
| **Local Store (SQLite)** | Persist event log, voice state snapshots (optional)                      | Non-fatal: disable persistence; continue in-memory only |

### 4.3 Concurrency Model

```
Main Thread
└── Tokio Runtime (multi-thread)
    ├── Task: probe_manager (spawn + supervise eBPF tasks)
    │   ├── Task: ringbuf_consumer[cpu_sched]
    │   ├── Task: ringbuf_consumer[mem_pressure]
    │   ├── Task: ringbuf_consumer[net_io]
    │   └── Task: ringbuf_consumer[proc_lifecycle]
    ├── Task: aggregator (receives from consumers via mpsc channel)
    ├── Task: mapping_engine (receives from aggregator via mpsc channel)
    ├── Task: osc_dispatcher (receives from mapping_engine via mpsc channel)
    └── Task: control_api (Axum, isolated from above pipeline)
```

**Channel design principle:**
- All inter-task channels are bounded.
- A full channel on the audio pipeline side MUST trigger backpressure logging + metric increment, NOT a panic or block.
- The control API task MUST NOT share mutable state with the audio pipeline tasks (read-only snapshots only, via `Arc<RwLock<Snapshot>>`).

---

## 5. Component Specifications

### 5.1 eBPF Probes

| Probe Name       | Attach Point                    | Data Captured                   | Data Explicitly NOT Captured      |
|------------------|---------------------------------|---------------------------------|-----------------------------------|
| `cpu_sched`      | `tracepoint/sched/sched_switch` | prev/next comm hash, latency_ns | cmdline, args, env vars           |
| `mem_pressure`   | `tracepoint/vmscan/*`           | reclaim events, pages, zone     | memory contents, addresses        |
| `net_io`         | `tracepoint/net/net_dev_xmit`   | bytes_tx, bytes_rx, dev_name    | packet contents, IPs, ports       |
| `proc_lifecycle` | `tracepoint/sched/sched_process_exec` | comm hash, uid (hashed)  | cmdline, argv, file paths         |

> **INVARIANT (Privacy):** eBPF programs MUST NOT access or forward: `task_struct->comm` in plaintext for any PID matching a sensitive comm pattern, syscall arguments, socket buffer contents, file path strings, or environment variables. All process identity fields MUST be hashed (FNV-64) before forwarding to userspace. See §10.

### 5.2 Aggregation Engine

- **Window size:** configurable, default `50ms`, minimum `10ms`, maximum `500ms`.
- **Snapshot fields:** one normalized `f64` value per metric dimension (0.0–1.0), plus a `lost_events: u64` counter per window.
- **Normalization strategy:** exponential moving average (EMA, α = 0.3 default) over raw window sum, then min-max clamp to configured domain per metric.
- **Failure behavior:** if a consumer task has died, emit the last valid snapshot with a `stale: true` flag in the snapshot struct.

### 5.3 Mapping Engine

The mapping engine is the primary user-facing customization surface. It evaluates a set of **mapping rules** defined in `daemon-choir.config.toml`:

```toml
# Example mapping rule
[[mapping]]
source_metric = "cpu.sched.latency_p95"
osc_address   = "/choir/voice/0/frequency"
transform     = "exponential"   # linear | exponential | logarithmic | step
input_range   = [0.0, 1.0]
output_range  = [220.0, 880.0]
```

**Hot reload:** the mapping engine MUST support config reload via:
1. `SIGHUP` signal (triggers file re-read + rule recompile).
2. `POST /v1/config/reload` control API endpoint.

Rules are compiled to a static dispatch table at load time for O(1) lookup per metric dimension.

### 5.4 OSC Dispatcher

- **Protocol:** UDP (connectionless, fire-and-forget). No TCP OSC in v1.
- **Fan-out:** up to 8 backends concurrently (configurable).
- **Backend health:** unreachable backends are logged at WARN level and skipped; they do NOT block dispatch to other backends.
- **Message format:** OSC 1.1 bundles per snapshot window, with a timetag of the snapshot timestamp.

---

## 6. Control API Contract

> **CANONICAL:** `docs/api/control-api.openapi.yaml` is the authoritative contract.
> This section is a human-readable summary. In case of conflict, the OpenAPI file wins.

### 6.1 Binding & Auth

| Property       | Value                              |
|----------------|------------------------------------|
| Bind address   | `127.0.0.1:9876` (ONLY — not configurable to non-loopback in default mode) |
| Auth model     | None (loopback-only; same-user-session assumed). See §9.4 for multi-user note. |
| API versioning | URI prefix: `/v1/...` All endpoints below are under `/v1`. |
| Content-Type   | `application/json` for all request/response bodies. |

### 6.2 Endpoint Table

| Method | Path                  | Description                                      | Success | Error codes |
|--------|-----------------------|--------------------------------------------------|---------|-------------|
| GET    | `/v1/status`          | Daemon health, uptime, version, build info       | 200     | 500         |
| GET    | `/v1/state`           | Current metric snapshot (normalized values)      | 200     | 500         |
| GET    | `/v1/voices`          | Current OSC voice assignment map                 | 200     | 500         |
| GET    | `/v1/probes`          | Loaded eBPF probes + per-probe stats             | 200     | 500         |
| GET    | `/v1/config`          | Active config (redacted — no secrets)            | 200     | 500         |
| POST   | `/v1/config/reload`   | Trigger hot reload of mapping rules from disk    | 202     | 409, 500    |
| PUT    | `/v1/config/backends` | Update OSC backend list (hot, no restart needed) | 200     | 400, 500    |
| GET    | `/v1/metrics`         | Internal perf counters (Prometheus text format)  | 200     | 500         |
| POST   | `/v1/shutdown`        | Graceful shutdown (requires `X-Daemon-Choir: shutdown` header as CSRF guard) | 202 | 403, 500 |

### 6.3 Key Response Schemas

#### `GET /v1/state` → `StateSnapshot`
```json
{
  "timestamp_us": 1718200000000000,
  "window_ms": 50,
  "stale": false,
  "lost_events": 0,
  "metrics": {
    "cpu.sched.latency_p95":   0.42,
    "cpu.sched.context_rate":  0.61,
    "mem.reclaim.pressure":    0.18,
    "net.tx.bytes_norm":       0.07,
    "net.rx.bytes_norm":       0.09,
    "proc.exec.rate":          0.03
  }
}
```

#### `GET /v1/status` → `DaemonStatus`
```json
{
  "version": "0.1.0",
  "build_commit": "abc1234",
  "uptime_secs": 3820,
  "state": "running",
  "probes_loaded": 4,
  "backends_active": 1,
  "api_version": "v1"
}
```

#### `GET /v1/probes` → `ProbeInfo[]`
```json
[
  {
    "name": "cpu_sched",
    "attach_point": "tracepoint/sched/sched_switch",
    "loaded": true,
    "events_total": 482910,
    "events_lost": 12,
    "last_event_us": 1718200000000000
  }
]
```

### 6.4 Versioning Policy

- **Breaking changes** (field removal, type change, endpoint removal) → increment major version (`/v2/...`).
- **Additive changes** (new optional fields, new endpoints) → no version bump; document in changelog.
- The `/v1/` prefix MUST remain supported for a minimum of 2 major releases after a `/v2/` is introduced.

---

## 7. Data Model & Storage

### 7.1 In-Memory Structures (Canonical: `crates/daemon-choir-common/src/`)

```rust
// events.rs — CANONICAL (see §13)

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawEvent {
    pub probe: ProbeId,
    pub timestamp_ns: u64,
    pub payload: EventPayload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EventPayload {
    CpuSched(CpuSchedEvent),
    MemPressure(MemPressureEvent),
    NetIo(NetIoEvent),
    ProcLifecycle(ProcLifecycleEvent),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuSchedEvent {
    pub prev_comm_hash: u64,  // FNV-64 hash — NEVER raw comm string
    pub next_comm_hash: u64,
    pub latency_ns: u64,
    pub cpu_id: u32,
}

// (MemPressureEvent, NetIoEvent, ProcLifecycleEvent similarly structured)
```

### 7.2 Config Schema (Canonical: `docs/schema/daemon-choir.config.schema.json`)

The config file lives at `$XDG_CONFIG_HOME/daemon-choir/daemon-choir.config.toml` (default: `~/.config/daemon-choir/daemon-choir.config.toml`).

Top-level sections:

```toml
[daemon]
window_ms        = 50          # Aggregation window (10–500)
log_level        = "info"      # trace | debug | info | warn | error
log_format       = "json"      # json | pretty
control_api_port = 9876        # MUST remain on loopback

[probes]
enabled = ["cpu_sched", "mem_pressure", "net_io", "proc_lifecycle"]

[backends]
# At least one backend required
[[backends.osc]]
name    = "supercollider"
address = "127.0.0.1:57120"

[[mapping]]
# (see §5.3 for mapping rule schema)
```

**Config schema versioning:** the config file MUST include a `[meta]` block:

```toml
[meta]
schema_version = "1"   # increment on breaking changes
```

The daemon MUST reject config files with an unsupported `schema_version` with a clear error message.

### 7.3 Error Handling Policy (CANONICAL)

> These rules are binding. Deviation requires an ADR and owner sign-off.

| Error Class                      | MUST behavior                                                    | MUST NOT behavior                  |
|----------------------------------|------------------------------------------------------------------|------------------------------------|
| Ring buffer event loss           | Increment `events_lost` counter; log WARN once per window        | Panic; block; drop OSC dispatch    |
| eBPF probe load failure          | Log ERROR; continue with remaining probes; expose in `/v1/probes` | Abort startup if ≥1 probe loads    |
| Mapping rule parse error         | Reject new config; keep running with previous valid config; return 409 from reload API | Apply partial/corrupt config |
| OSC backend unreachable          | Log WARN; skip backend; continue fan-out to others               | Block dispatch; retry synchronously |
| Control API error                | Log + return HTTP error to caller                                | Affect audio pipeline in any way   |
| SQLite write failure             | Log ERROR; disable persistence; continue in-memory               | Panic; affect audio pipeline       |
| Config hot reload parse failure  | Return error; preserve running config                            | Silently apply partial config      |

### 7.4 Local Storage (SQLite)

- **Location:** `$XDG_DATA_HOME/daemon-choir/state.db` (default: `~/.local/share/daemon-choir/state.db`)
- **Purpose:** optional event log for post-session analysis; voice state snapshots for TUI history.
- **Retention:** configurable; default 24h rolling window. Events older than retention window are purged on startup and at midnight.
- **Schema migrations:** handled via embedded SQL migrations in `conductor/src/migrations/`. Migrations MUST be idempotent.
- **Permissions:** file MUST be created with mode `0600`.

---

## 8. Non-Functional Requirements

### 8.1 Performance Budgets (CANONICAL — binding targets)

| Component                 | Metric                    | Target        | Hard Limit    | Measurement Method                    |
|---------------------------|---------------------------|---------------|---------------|---------------------------------------|
| Conductor (steady state)  | CPU usage                 | < 1.0%        | < 2.0%        | `perf stat` over 60s window           |
| Conductor (steady state)  | RSS memory                | < 50 MB       | < 80 MB       | `/proc/self/status` VmRSS             |
| Ring buf → OSC (e2e)      | Dispatch latency          | ≤ 10ms p99    | ≤ 20ms p99    | Internal timestamp delta, exposed via `/v1/metrics` |
| Aggregation window        | Processing time per window | ≤ 2ms         | ≤ 5ms         | Tokio task timing instrumentation     |
| eBPF probe (per event)    | Kernel execution time     | ≤ 5µs avg     | ≤ 20µs max    | `bpftool prog profile`                |
| OSC fan-out (1 backend)   | Dispatch time             | ≤ 1ms         | ≤ 3ms         | Internal timestamp delta              |
| Control API               | Response latency (p99)    | ≤ 50ms        | ≤ 200ms       | Internal histogram, exposed via `/v1/metrics` |
| Renderer (visual backend) | Frame time (60fps target) | ≤ 16.6ms      | ≤ 33ms        | External backend's responsibility     |

**Gate policy:** PRs that regress any "Hard Limit" MUST NOT merge. PRs that regress a "Target" require reviewer sign-off with justification.

### 8.2 Reliability Invariants

| ID    | Invariant                                                                          |
|-------|------------------------------------------------------------------------------------|
| R-01  | **Audio continuity MUST be preserved** under any single-component failure.         |
| R-02  | The Conductor MUST NOT crash on malformed eBPF event payloads; it MUST log + skip. |
| R-03  | The Conductor MUST handle SIGTERM gracefully: flush buffers, unload eBPF programs, close SQLite, exit 0. |
| R-04  | The Conductor MUST handle SIGHUP as a hot reload trigger, not a crash.              |
| R-05  | After a probe reload failure, the daemon MUST continue operating with previously loaded probes. |

### 8.3 Startup & Shutdown SLOs

| Phase    | Target  | Hard Limit | Notes                              |
|----------|---------|------------|------------------------------------|
| Startup (to first OSC dispatch) | < 500ms | < 2s | Includes eBPF load time |
| Graceful shutdown | < 1s | < 5s | Includes eBPF unload + SQLite flush |

---

## 9. Security Model & Threat Analysis

### 9.1 Assets

| Asset                   | Sensitivity | Why it matters                                          |
|-------------------------|-------------|----------------------------------------------------------|
| Process identity data   | High        | Even hashed comm names can be fingerprinted              |
| System call patterns    | High        | Can reveal application behavior/secrets indirectly       |
| Config file (mappings)  | Low         | User-authored; no secrets expected                       |
| SQLite event log        | Medium      | Aggregated behavioral fingerprint of the system          |
| Control API             | Medium      | Can trigger config changes or graceful shutdown          |

### 9.2 Threat Model

| Threat                                   | Actor                    | Surface              | Likelihood | Impact | Mitigation                              |
|------------------------------------------|--------------------------|----------------------|------------|--------|-----------------------------------------|
| Unprivileged local user reads event log  | Malicious local user     | SQLite file          | Medium     | Medium | File mode 0600; owner = running user    |
| Unprivileged local user calls control API | Malicious local user    | TCP localhost:9876   | Medium     | Medium | Bind to 127.0.0.1; add optional token auth in v1.1 |
| eBPF program exploits kernel bug         | Supply chain / CVE       | eBPF verifier        | Low        | High   | Use stable tracepoints only; no kprobes in v1; BPF verifier is defense-in-depth |
| Malicious config triggers log injection  | Local user               | Config file          | Low        | Low    | Sanitize all user config strings before logging |
| Dependency supply chain compromise       | Upstream Rust crates     | cargo dependency tree | Low       | High   | Pin Cargo.lock; audit with `cargo-audit` in CI |
| Sensitive process names leaked via OSC   | OSC backend process      | UDP localhost        | Low        | Medium | Enforce hash-only rule in eBPF (§9.5); no raw strings in OSC output |

### 9.3 Capability Model (MUST follow)

The Conductor MUST follow this exact capability lifecycle:

```
1. Startup:
   - Binary has CAP_BPF + CAP_PERFMON granted via setcap (installer sets this)
   - OR: binary is run as root ONLY during install (sets capabilities), then drops to user

2. During eBPF load (probe_manager):
   - Holds: CAP_BPF, CAP_PERFMON
   - Does: loads and attaches eBPF programs

3. After eBPF load (steady state):
   - Drops: ALL capabilities via PR_SET_SECUREBITS + prctl(PR_SET_NO_NEW_PRIVS)
   - Retains: nothing above a normal user process

4. Shutdown:
   - eBPF programs are detached and unloaded (kernel ref-counted; no root needed for detach)
```

**setcap install command (Makefile target `install-caps`):**
```bash
sudo setcap 'cap_bpf,cap_perfmon=ep' /usr/local/bin/daemon-choir
```

### 9.4 Multi-User Machine Note

On multi-user machines (e.g., shared workstations), the control API on localhost:9876 is reachable by any local user. Until optional token auth is implemented (tracked as deferred work §15), **operators MUST be aware of this** and may use firewall rules (`iptables -A OUTPUT -d 127.0.0.1 -p tcp --dport 9876 -m owner ! --uid-owner <uid> -j REJECT`) to restrict access.

This is tracked as a known limitation and MUST be documented in the README.

### 9.5 Binary Distribution & Trust

- All distributed binaries MUST be signed (GPG or Sigstore/cosign).
- The install script MUST verify the signature before running.
- `cargo-audit` MUST be run in CI on every PR and on a nightly schedule.
- `Cargo.lock` MUST be committed to the repository.

---

## 10. Privacy-by-Design

### 10.1 Privacy Commitment Summary

DAEMON CHOIR is **local-first, privacy-first by construction.** No personal data, no behavioral telemetry, and no system information of any kind leaves the local machine under any configuration.

### 10.2 Data Handling Table

| Data Type                      | Collected? | Stored?     | Transmitted? | Notes                                   |
|--------------------------------|-----------|-------------|-------------|------------------------------------------|
| Process command names (comm)   | Hash only  | Hash only   | Never       | FNV-64 hash; not reversible in isolation |
| Process arguments / cmdline    | Never      | Never       | Never       | Not accessible from eBPF tracepoint      |
| Environment variables          | Never      | Never       | Never       | —                                        |
| Syscall arguments              | Never      | Never       | Never       | tracepoint attach only, not kprobes      |
| File paths                     | Never      | Never       | Never       | —                                        |
| Socket / packet contents       | Never      | Never       | Never       | —                                        |
| Memory contents / addresses    | Never      | Never       | Never       | —                                        |
| IP addresses / ports           | Never      | Never       | Never       | net_io probe captures bytes only         |
| User IDs                       | Hash only  | Hash only   | Never       | UID hashed before forwarding             |
| Aggregated metric values (normalized 0.0–1.0) | Yes | Optional (SQLite, local) | OSC to localhost only | Not personally identifiable |

### 10.3 Retention Controls

| Store                     | Default Retention | User Control                       |
|---------------------------|-------------------|------------------------------------|
| In-memory snapshot        | Current window only | n/a (always ephemeral)           |
| SQLite event log          | 24 hours          | `[storage] retention_hours` config |
| Log files                 | Session only (no log rotation by default) | `[daemon] log_file` config |

### 10.4 "Privacy Mode" (Emergency Wipe)

`POST /v1/privacy/wipe` (or CLI `daemon-choir wipe`) MUST:
1. Truncate all SQLite tables.
2. Delete any log files written by the daemon.
3. Log a single line: `{"event": "privacy_wipe", "timestamp": "..."}` and nothing else.
4. Return 200 OK.

---

## 11. Operability — Production Readiness Review (PRR)

> This section is the functional equivalent of an SRE Production Readiness Review.
> ALL items MUST be resolved (or explicitly accepted as known gaps with owner sign-off) before the v1.0.0 release tag.

### 11.1 SLOs (Service Level Objectives)

| Journey                              | SLO                                                   | SLI Measurement Source                    |
|--------------------------------------|-------------------------------------------------------|-------------------------------------------|
| Ring buffer → OSC dispatch           | p99 latency ≤ 20ms over any 60s window               | `/v1/metrics` histogram: `dispatch_latency_ms` |
| Daemon availability (audio path up) | ≥ 99.9% of windows produce valid OSC output while running | `/v1/metrics` counter: `osc_dispatch_success_total` / `osc_dispatch_total` |
| Control API availability             | ≥ 99% of requests respond in ≤ 200ms                 | `/v1/metrics` histogram: `api_response_ms` |
| Startup time                         | ≤ 2s to first OSC dispatch in ≥ 95% of starts        | Log: `first_osc_dispatch_ms` structured log field |

### 11.2 Internal Metrics (MUST expose via `GET /v1/metrics`)

Prometheus text format. These are the minimum required metrics:

```
# HELP dispatch_latency_ms Ring buffer to OSC dispatch latency
# TYPE dispatch_latency_ms histogram
dispatch_latency_ms_bucket{le="5"} ...
dispatch_latency_ms_bucket{le="10"} ...
dispatch_latency_ms_bucket{le="20"} ...

# HELP osc_dispatch_total Total OSC dispatches attempted
# TYPE osc_dispatch_total counter
osc_dispatch_total{backend="supercollider"} ...

# HELP osc_dispatch_success_total Successful OSC dispatches
# TYPE osc_dispatch_success_total counter
osc_dispatch_success_total{backend="supercollider"} ...

# HELP events_lost_total Ring buffer events lost due to overflow
# TYPE events_lost_total counter
events_lost_total{probe="cpu_sched"} ...

# HELP api_response_ms Control API response latency
# TYPE api_response_ms histogram

# HELP probe_load_errors_total eBPF probe load failures
# TYPE probe_load_errors_total counter
```

### 11.3 Structured Logging

All logs MUST be structured JSON (when `log_format = "json"`). Required fields on every log line:

```json
{
  "timestamp": "2025-06-12T10:00:00.000Z",
  "level": "INFO",
  "component": "conductor.osc",
  "message": "...",
  "version": "0.1.0"
}
```

Event-specific fields are additive. No log line MUST contain process comm strings (only hashes), file paths, or socket contents.

### 11.4 Alerting Recommendations (for users running daemon-choir in critical workflows)

| Condition                                          | Suggested Alert                              |
|----------------------------------------------------|----------------------------------------------|
| `events_lost_total` rate > 100/s for > 30s         | WARN: ring buffer overflow, increase window_ms |
| `dispatch_latency_ms` p99 > 20ms for > 60s        | WARN: dispatch latency SLO breach            |
| `probe_load_errors_total` > 0 on startup           | ERROR: one or more eBPF probes failed to load |
| Process not running (systemd unit inactive)        | ERROR: daemon-choir is down                  |

### 11.5 Runbook: Common Failure Scenarios

#### Scenario 1: eBPF probe fails to load
```
Symptom: /v1/probes shows loaded=false; /v1/metrics shows probe_load_errors_total > 0
Check:   kernel version (must be ≥ 5.8); setcap status; dmesg for BPF verifier errors
Fix:     sudo setcap 'cap_bpf,cap_perfmon=ep' $(which daemon-choir); restart daemon
```

#### Scenario 2: OSC backend unreachable
```
Symptom: osc_dispatch_success_total << osc_dispatch_total; WARN logs for backend
Check:   Is audio backend (SuperCollider/PD) running? Is it listening on configured port?
Fix:     Start audio backend first; or update backend address via PUT /v1/config/backends
```

#### Scenario 3: High dispatch latency
```
Symptom: dispatch_latency_ms p99 > 20ms
Check:   System under extreme load? window_ms too small?
Fix:     Increase window_ms in config (try 100ms); reload config (POST /v1/config/reload)
```

#### Scenario 4: Daemon consuming > 2% CPU
```
Symptom: perf stat shows CPU > hard limit
Check:   How many probes loaded? Window size? Number of OSC backends?
Fix:     Reduce enabled probes; increase window_ms; profile with cargo flamegraph
```

### 11.6 Rollout Plan (v1.0.0)

| Phase | Description | Criteria to Proceed |
|-------|-------------|---------------------|
| Phase 0 | Internal dogfood (developer machines only) | All unit + integration tests pass; performance targets met |
| Phase 1 | Limited release (opt-in, documented as beta) | No P0 bugs in 2 weeks of dogfood; cargo-audit clean |
| Phase 2 | Public release (signed binary, GitHub releases) | Phase 1 complete; README + install docs complete; security review sign-off |

### 11.7 Rollback Plan

- Rollback = stop daemon (`systemctl stop daemon-choir` or kill process), reinstall previous binary.
- Config files are backward-compatible within a major version; no config migration needed for minor/patch rollback.
- SQLite schema changes that are not backward-compatible MUST be gated behind a major version bump and MUST include a migration that preserves data.

---

## 12. Testing & Verification

### 12.1 Test Matrix

| Test Type              | Scope                                                      | Tooling              | Required? |
|------------------------|------------------------------------------------------------|----------------------|-----------|
| Unit tests             | Aggregator, mapping engine, config parsing, event structs  | `cargo test`         | Yes, P0   |
| Integration tests      | Conductor startup + OSC output (mock backend)              | `cargo test --test`  | Yes, P0   |
| eBPF probe tests       | Probe load/unload lifecycle, event emission               | Custom test harness (VM) | Yes, P0 |
| Control API tests      | All endpoints; error cases; CSRF guard on /v1/shutdown     | `cargo test` + `httpc-test` | Yes, P0 |
| Performance benchmarks | dispatch_latency, CPU, memory against §8.1 targets        | `cargo bench` + `criterion` | Yes, P1 |
| Security tests         | Verify no raw comm strings in OSC output or logs; verify API bind address | Custom assertions | Yes, P1 |
| Chaos tests            | Kill probe tasks; fill ring buffer; kill OSC backend; corrupt config | Manual + scripted | Yes, P1 (before v1.0.0) |
| Kernel compat tests    | Boot in VM with kernel 5.8, 6.1, 6.6, 6.9 (see Appendix B) | CI matrix (QEMU)  | Yes, P1   |

### 12.2 Performance Benchmark Gates (CI-enforced)

These benchmarks MUST be run on every PR that touches `conductor/`, `ebpf-programs/`, or `daemon-choir-common/`:

```bash
# Run benchmarks
cargo bench --bench dispatch_latency
cargo bench --bench aggregation_throughput

# Gate check (fail CI if any benchmark regresses > 10% vs baseline)
cargo bench -- --save-baseline main   # on main branch
cargo bench -- --load-baseline main   # on PR branch (compare)
```

### 12.3 Privacy Assertion Tests

The following MUST be tested automatically:

```rust
#[test]
fn test_no_raw_comm_in_events() {
    // Assert that no RawEvent or derived struct contains a plaintext
    // process name field — only hash fields are permitted.
}

#[test]
fn test_no_raw_comm_in_osc_output() {
    // Capture OSC output from a test run; assert no string arguments
    // match known process names from /proc.
}

#[test]
fn test_api_binds_loopback_only() {
    // Assert that control API does not accept connections on non-loopback.
}
```

---

## 13. Rollout & Installation

### 13.1 Install Targets (Makefile)

```makefile
install:         ## Install binary, systemd unit, default config, set capabilities
install-caps:    ## Set CAP_BPF + CAP_PERFMON on installed binary
install-systemd: ## Install and enable systemd user service
uninstall:       ## Remove binary, systemd unit, capabilities
wipe:            ## Remove all daemon-choir data (SQLite, logs) — privacy wipe
```

### 13.2 Systemd Unit (`systemd/daemon-choir.service`)

```ini
[Unit]
Description=DAEMON CHOIR — system-state-to-sound daemon
After=network.target sound.target
Documentation=https://github.com/knarayanareddy/DAEMON-CHOIR

[Service]
Type=simple
ExecStart=/usr/local/bin/daemon-choir --config %h/.config/daemon-choir/daemon-choir.config.toml
Restart=on-failure
RestartSec=5s
# Security hardening
NoNewPrivileges=yes
ProtectSystem=strict
ProtectHome=read-only
PrivateTmp=yes
# Allow eBPF (capabilities set via setcap on binary)
AmbientCapabilities=

[Install]
WantedBy=default.target
```

> Note: `NoNewPrivileges=yes` is compatible with `setcap` because capabilities are file capabilities, not inherited ambient capabilities.

### 13.3 Canonicalization Map

| What you're looking for | Go here — this is canonical | NOT here |
|-------------------------|------------------------------|---------|
| Event struct definitions | `crates/daemon-choir-common/src/events.rs` | This doc (§5.1 is illustrative) |
| Control API endpoint contracts | `docs/api/control-api.openapi.yaml` | This doc (§6 is illustrative) |
| Config schema & field types | `docs/schema/daemon-choir.config.schema.json` | This doc (§7.2 is illustrative) |
| Performance budget targets | This doc, §8.1 | Nowhere else |
| Security invariants | This doc, §9 | Nowhere else |
| Error handling policy | This doc, §7.3 | Nowhere else |
| Privacy commitments | This doc, §10 | Nowhere else |

---

## 14. Architectural Decision Records (ADRs)

> ADRs are stored as individual files in `docs/adrs/`. This section is an index + summary.
> Each ADR uses the standard format: **Context → Decision → Consequences**.

### ADR-001: eBPF Library — Aya (Rust-native) vs libbpf (C bindings)

- **Context:** We need to load and manage eBPF programs from Rust. Two options: Aya (pure Rust, no libbpf dep) or Aya-libbpf or raw libbpf-sys.
- **Decision:** Use **Aya** (pure Rust). No C dependency; better type safety; async-native Tokio integration; ring buffer support since Aya 0.12.
- **Consequences:** eBPF programs themselves are still written in C (kernel-side). CO-RE (Compile Once, Run Everywhere) via BTF is required for kernel portability. We accept the Aya API stability risk (pre-1.0) and will pin to a specific release.

### ADR-002: Ring Buffer API — BPF ring buffer vs perf buffer

- **Context:** Two options for kernel→user event delivery: classic `BPF_MAP_TYPE_PERF_EVENT_ARRAY` (perf buffer) or modern `BPF_MAP_TYPE_RINGBUF` (ring buffer, kernel ≥ 5.8).
- **Decision:** Use **BPF ring buffer**. Lower overhead; avoids per-CPU buffer management; better for multi-consumer; aligns with kernel direction.
- **Consequences:** Hard requirement: kernel ≥ 5.8. This is acceptable given our target (modern Linux developer machines). Documented in §2.3 and Appendix B.

### ADR-003: Control API Protocol — REST/HTTP vs Unix Socket vs gRPC

- **Context:** We need an IPC mechanism for the TUI and other clients to query daemon state and trigger actions.
- **Decision:** Use **REST/HTTP over TCP loopback** (Axum). Simple to implement, simple to consume from any language/tool (curl, scripts, etc.), adequate for the low-frequency control plane. Not gRPC (unnecessary complexity for local IPC). Not Unix socket (harder to expose to multiple clients cleanly).
- **Consequences:** Must enforce loopback-only binding (§9.2, §6.1). Must document multi-user limitation (§9.4).

### ADR-004: OSC Protocol — UDP vs TCP

- **Context:** OSC supports both UDP and TCP transports.
- **Decision:** **UDP only** in v1. Audio backends (SuperCollider, PD) default to UDP OSC. Fire-and-forget is appropriate for a stream of low-stakes control messages. TCP adds connection management complexity for no benefit.
- **Consequences:** No delivery guarantee. A lost OSC message means one missed parameter update — acceptable, since the next window will send an updated value anyway. Revisit TCP for high-reliability use cases if user demand exists.

### ADR-005: Local Storage — SQLite vs flat files vs no persistence

- **Context:** We want optional persistence of event logs for post-session analysis and TUI history. Options: SQLite, flat JSON/CSV files, no persistence.
- **Decision:** **SQLite** (via rusqlite). Indexed queries for TUI history; atomic writes; well-understood. Flat files risk unbounded growth and no query capability. No persistence is a non-starter for TUI history feature.
- **Consequences:** Additional dependency (rusqlite). Must handle SQLite errors as non-fatal (§7.3). Must enforce file permissions (§7.4).

### ADR-006: Process Identity — Raw comm strings vs hashing

- **Context:** eBPF tracepoints expose `task->comm` (process name, 16 bytes). Including this in events is useful for debugging but creates privacy exposure.
- **Decision:** **FNV-64 hash of comm** in all userspace events and logs. Raw strings are never forwarded past the eBPF→userspace boundary.
- **Consequences:** Loss of human-readable process names in events. Acceptable: debug mode can print hash-to-name mappings from `/proc` at runtime if user explicitly opts in (deferred feature, §15). Privacy commitment is non-negotiable.

---

## 15. Open Questions & Deferred Work

| ID    | Item                                                                               | Priority | Decision Needed By |
|-------|------------------------------------------------------------------------------------|----------|--------------------|
| OQ-01 | Optional token auth on control API for multi-user machines                        | P1       | v1.1.0             |
| OQ-02 | Debug mode: opt-in hash→comm resolution from /proc (user must explicitly enable)  | P2       | v1.2.0             |
| OQ-03 | TCP OSC support for high-reliability use cases                                     | P3       | Backlog            |
| OQ-04 | macOS support via DTrace / dtrace-rs (if kernel-independent tracepoints feasible)  | P3       | Backlog            |
| OQ-05 | Plugin system for user-defined mapping transform functions (WASM sandbox?)         | P3       | Backlog            |
| OQ-06 | Formal verification of eBPF programs (PREVAIL or similar)                         | P2       | v1.1.0 (security)  |
| OQ-07 | Sigstore/cosign integration for binary signing in release pipeline                 | P1       | v1.0.0 (release)   |
| OQ-08 | Visual backend protocol — extend OSC schema or define a separate MIDI/DMX path?   | P2       | Before v1.0.0      |
| OQ-09 | CI kernel compatibility matrix in QEMU — infrastructure needed                     | P1       | Before v1.0.0      |

---

## Appendix A — Glossary

| Term              | Definition                                                                                  |
|-------------------|---------------------------------------------------------------------------------------------|
| **eBPF**          | Extended Berkeley Packet Filter — a kernel subsystem that runs sandboxed programs in kernel space |
| **Aya**           | A pure-Rust library for writing and loading eBPF programs                                   |
| **Ring buffer**   | `BPF_MAP_TYPE_RINGBUF` — a lock-free, single-producer/multi-consumer event buffer in eBPF   |
| **Conductor**     | The main DAEMON CHOIR daemon process                                                         |
| **Probe**         | An eBPF program attached to a kernel tracepoint or kprobe                                   |
| **Snapshot**      | A single aggregation window output: one normalized metric value per dimension                |
| **Voice**         | A named OSC output channel mapped from one or more metric dimensions                         |
| **Mapping rule**  | A user-configured rule that transforms a metric value to an OSC parameter value             |
| **OSC**           | Open Sound Control — a UDP-based protocol for real-time audio/visual parameter control       |
| **CAP_BPF**       | Linux capability required to load BPF programs (kernel ≥ 5.8)                               |
| **CAP_PERFMON**   | Linux capability required to attach BPF to perf events (kernel ≥ 5.8)                       |
| **FNV-64**        | Fowler–Noll–Vo 64-bit hash function — used for process identity anonymization                |
| **SIGHUP**        | POSIX signal used by DAEMON CHOIR to trigger hot config reload                               |
| **ADR**           | Architectural Decision Record — a short doc capturing a key decision: context → decision → consequences |

---

## Appendix B — Kernel Compatibility Matrix

| Kernel Version | BPF Ring Buffer | CAP_BPF / CAP_PERFMON | Supported? | Notes |
|----------------|-----------------|----------------------|------------|-------|
| < 5.8          | ❌               | ❌                    | ❌ No      | Hard requirement not met |
| 5.8 – 5.12     | ✅               | ✅                    | ✅ Yes     | Minimum supported; limited BTF |
| 5.13 – 5.19    | ✅               | ✅                    | ✅ Yes     | Full BTF/CO-RE support |
| 6.0 – 6.5      | ✅               | ✅                    | ✅ Yes     | Recommended range |
| 6.6 LTS        | ✅               | ✅                    | ✅ Yes     | **Primary test target** |
| 6.7+           | ✅               | ✅                    | ✅ Yes     | Tested in CI matrix |

CI MUST run eBPF probe tests in QEMU VMs for: 5.8, 6.1 (LTS), 6.6 (LTS), latest stable.

---

## Appendix C — Platform Support Matrix

| Distro                  | Kernel (typical) | Package Manager | Status         | Notes                        |
|-------------------------|-----------------|-----------------|----------------|------------------------------|
| Ubuntu 22.04 LTS        | 5.15            | apt             | ✅ Supported   | Primary development target   |
| Ubuntu 24.04 LTS        | 6.8             | apt             | ✅ Supported   | CI target                    |
| Fedora 39+              | 6.5+            | dnf             | ✅ Supported   | CI target                    |
| Debian 12 (Bookworm)    | 6.1             | apt             | ✅ Supported   | CI target                    |
| Arch Linux              | Rolling (6.x)   | pacman          | ✅ Supported   | Developer dogfood             |
| NixOS (unstable)        | 6.x             | nix             | 🟡 Planned     | Nix flake planned for v1.1   |
| openSUSE Tumbleweed     | Rolling         | zypper          | 🟡 Untested    | Expected to work             |
| macOS                   | n/a             | n/a             | ❌ Not supported | eBPF not available; see OQ-04 |
| Windows (WSL2)          | 5.15 (WSL)      | n/a             | 🟡 Partial     | eBPF in WSL2 limited; not a target |

---

*End of DAEMON CHOIR Production Engineering Specification v2.0.0*
*Status: 🟡 In Review — promote to ✅ Approved before merge to main*
```

---

## What changed from v1.0 → this v2.0 (summary for your review)

Here's the precise delta so you know exactly what was added vs preserved:

| Area | v1.0 | v2.0 |
|------|------|------|
| **Doc governance** | Absent | Full front-matter: version, status, owner, reviewers, changelog, canonicalization map |
| **Scope** | Covered implicitly | Explicit goals/non-goals/constraints tables with IDs + priorities |
| **System context** | Partial text description | Full context diagram + trust boundary rules + external dependency inventory |
| **Crate layout** | Mentioned | Explicit workspace tree with canonical file locations called out |
| **API contract** | Mentioned `localhost:9876` + a few routes | Full endpoint table, JSON schemas per endpoint, versioning policy, CSRF guard, binding rules |
| **Error handling** | Stated as a principle | Formal table: per-error-class MUST/MUST NOT behavior — marked canonical |
| **Config schema** | Described in prose | TOML skeleton + schema versioning rules + `[meta] schema_version` required field |
| **Performance** | Targets listed | Full table with targets + hard limits + measurement methods + CI gate policy |
| **Reliability** | Audio invariant stated | Formal invariant table with IDs (R-01 through R-05) |
| **Security** | Capability notes | Full threat model table (actors/surfaces/likelihood/impact/mitigations) + capability lifecycle + multi-user note + supply chain policy |
| **Privacy** | Data table present | Expanded + retention controls + privacy wipe endpoint spec |
| **PRR / Operability** | Absent | Full SLOs, required metrics (Prometheus), structured log spec, alerting recommendations, runbooks, rollout phases, rollback plan |
| **Testing** | Mentioned | Full test matrix with tooling + required status + privacy assertion tests + benchmark gates |
| **ADRs** | Absent | 6 ADRs: Aya, ring buffer, REST vs gRPC, UDP OSC, SQLite, privacy hashing |
| **Open questions** | A few scattered notes | Formal table with IDs, priorities, target versions |
| **Appendices** | Partial | Glossary, kernel compat matrix, platform support matrix |
