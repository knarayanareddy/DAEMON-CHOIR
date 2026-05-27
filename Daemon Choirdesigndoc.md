🎵 DAEMON CHOIR
Comprehensive Engineering Design Document
Version 1.0 | Synesthetic OS Monitor — Turning Kernel Events Into Generative Ambient Music
TABLE OF CONTENTS

    Project Overview & Vision
    Goals, Non-Goals & Constraints
    System Architecture
    Module Breakdown
        4.1 eBPF Instrumentation Layer (Rust + Aya)
        4.2 Event Transport & Ring Buffer
        4.3 Conductor Engine (Aggregation & Mapping)
        4.4 Audio Synthesis Engine (SuperCollider / PureData)
        4.5 Visual Renderer (Ratatui TUI / Bevy 3D)
        4.6 Voice Allocation & Timbral Identity System
        4.7 Scene & Macro Behavior Engine
        4.8 Control API & Configuration Daemon
    Data Models & Schemas
    API Specifications
    Directory Structure
    Configuration System
    Audio Mapping Deep Dive
    eBPF Program Deep Dive
    Visual Renderer Deep Dive
    Privacy & Safety Model
    Security Model & Privilege Boundaries
    Storage & Persistence
    Logging, Observability & Debugging
    Testing Strategy
    Build, Packaging & Installation
    Platform Support Matrix
    Performance Targets & Benchmarks
    Error Handling Strategy
    Dependency Registry
    Milestone & Phased Rollout Plan
    Open Questions & Future Work

1. Project Overview & Vision
1.1 What is Daemon Choir?

Daemon Choir is a local, kernel-aware, generative ambient audio monitor that runs entirely on the user's machine. It hooks into the Linux kernel via eBPF, instruments every running process, network interface, and memory subsystem in real time, and translates that live kernel activity into a continuously evolving soundscape — optionally accompanied by a 3D or terminal visualizer.

It acts as a sensory layer on top of the OS:

    Every running process becomes a musical voice with a unique timbral identity
    Network traffic drives rhythm, tempo, and percussive density
    Memory pressure shapes reverb, space, and harmonic darkness
    CPU load governs amplitude, roughness, and tension
    Process lifecycle events (fork, exec, exit) trigger musical events (new voice, decay, silence)
    Anomalies (runaway processes, DDoS traffic, OOM pressure) translate to discordant, urgent, unmistakable sound

The result: a healthy, idle system sounds like Brian Eno's Ambient 1. A thrashing Docker build sounds like a rhythmic industrial bassline. A DDoS attack sounds like a thunderstorm. A memory leak sounds like a room slowly filling with wet, heavy fog.

All processing is local. No audio leaves the machine. No cloud backend. No subscription. The user's system speaks for itself.
1.2 The Problem Being Solved

Traditional system monitors demand your full visual attention:

    htop, btop, top require you to stare at a dashboard
    Alert systems only fire at threshold crossings — the transition to danger is invisible
    No tool leverages the human brain's extraordinary ability to detect ambient pattern changes in sound
    Operators running long-lived processes (builds, training jobs, servers) have no ambient signal of system health without dedicated screen real estate

Daemon Choir converts continuous system state into a background sensory channel — one you can perceive peripherally, without looking, without polling, without alert fatigue.
1.3 Design Philosophy
Principle	Description
Ambient-first	The soundscape must remain listenable over hours. Never shrill, never demanding, but always informative.
Learnable mappings	Every sound change must consistently correspond to a cause. Users must be able to learn the language.
Kernel-honest	Data comes directly from the kernel via eBPF. No polling approximations. No userspace guessing.
Graceful degradation	If eBPF load fails, fall back to /proc polling. If SuperCollider is absent, fall back to PureData or a built-in Rust synth.
Non-intrusive	The monitor must never materially affect the system it observes. CPU overhead < 1%, memory < 50MB.
Composable	Audio mappings are user-configurable. Visual scenes are swappable. Synthesis backends are pluggable.
Local-first	All synthesis and visualization runs on-device. No streaming audio to the cloud.
2. Goals, Non-Goals & Constraints
2.1 Goals (In Scope)

    eBPF-based kernel instrumentation for process, CPU, network, and memory events
    In-kernel aggregation via BPF ring buffers (no raw per-syscall event flooding)
    Userspace Rust "Conductor" that maps kernel metrics to musical parameters
    SuperCollider synthesis server as primary audio backend (via OSC)
    PureData as secondary audio backend
    Built-in Rust soft-synth as zero-dependency fallback backend
    Per-process voice allocation with stable timbral identity
    Network traffic → rhythm engine
    Memory pressure → reverb/space engine
    CPU load → amplitude/tension engine
    Scene/macro system: Calm, Busy, Storm, Critical
    Terminal UI visualizer (Ratatui) showing active voices, metrics, and scene state
    Optional 3D visualizer (Bevy) with spatial process constellation
    OSC control interface for user customization
    TOML configuration file for all mappings and thresholds
    Graceful /proc fallback when eBPF is unavailable
    Cross-platform: Linux (primary), macOS (limited, no eBPF), Windows (proc fallback only)
    Low-resource preset for embedded/edge servers

2.2 Non-Goals (Explicitly Out of Scope)

    ❌ Any modification to kernel behavior or process state
    ❌ Killing, pausing, or renice-ing processes based on audio state
    ❌ Streaming audio to any remote endpoint
    ❌ Cloud telemetry of any kind
    ❌ Acting as a security IDS/IPS or alerting replacement
    ❌ Windows kernel-level instrumentation (WFP/ETW integration is future work)
    ❌ macOS kernel extension or kext instrumentation
    ❌ Acting as a DAW or music production tool (it generates ambient texture, not compositions)
    ❌ MIDI output or DAW plugin format (a future extension, not v1.0)

2.3 Constraints

    eBPF requires Linux kernel ≥ 5.8 for BPF ring buffer support
    eBPF loading requires CAP_BPF + CAP_PERFMON capabilities (or root)
    Audio backend (SuperCollider/PureData) runs as a separate process; Daemon Choir communicates via OSC over localhost UDP
    Conductor process must use < 1% CPU on a modern multi-core machine (measured at P99)
    eBPF programs must pass kernel verifier — no unbounded loops, no unsafe memory access
    Ring buffer event volume must be rate-limited at the kernel level — no unbounded event floods to userspace
    SuperCollider synthesis must not stutter when the system is under moderate load (the monitor must not worsen the system it observes)
    Single binary deployment for the Conductor (Rust static binary)
    Audio backend must be separately installable but not embedded (too large)

3. System Architecture
3.1 High-Level Architecture Diagram

text

┌─────────────────────────────────────────────────────────────────────────┐
│                           USER'S MACHINE                                │
│                                                                         │
│  ┌──────────────────────────────────────────────────────────────────┐   │
│  │                    LINUX KERNEL                                  │   │
│  │                                                                  │   │
│  │  ┌──────────────┐  ┌──────────────┐  ┌────────────────────────┐ │   │
│  │  │  sched/      │  │  net/        │  │  mm/                   │ │   │
│  │  │  process     │  │  traffic     │  │  memory pressure       │ │   │
│  │  │  tracepoints │  │  counters    │  │  major faults, reclaim │ │   │
│  │  └──────┬───────┘  └──────┬───────┘  └──────────┬────────────┘ │   │
│  │         │                 │                      │              │   │
│  │  ┌──────▼─────────────────▼──────────────────────▼────────────┐ │   │
│  │  │              eBPF PROGRAMS (Rust/Aya)                       │ │   │
│  │  │   in-kernel aggregation → BPF_MAP_TYPE_RINGBUF              │ │   │
│  │  └──────────────────────────┬──────────────────────────────────┘ │   │
│  └─────────────────────────────┼────────────────────────────────────┘   │
│                                │  ring buffer events (10–60 Hz)         │
│                                ▼                                        │
│  ┌──────────────────────────────────────────────────────────────────┐   │
│  │              DAEMON CHOIR CONDUCTOR (Rust)                       │   │
│  │                                                                  │   │
│  │  ┌─────────────────┐    ┌─────────────────┐   ┌───────────────┐ │   │
│  │  │  Event Consumer │    │   Aggregator    │   │ Scene Engine  │ │   │
│  │  │  (ring buf poll)│───►│ (metric windows)│──►│ (macro state) │ │   │
│  │  └─────────────────┘    └─────────────────┘   └───────┬───────┘ │   │
│  │                                                        │         │   │
│  │  ┌─────────────────────────────────────────────────────▼───────┐ │   │
│  │  │              VOICE ALLOCATOR                                 │ │   │
│  │  │  process → voice identity → musical parameters              │ │   │
│  │  └──────────────────┬───────────────────────────────────────────┘ │   │
│  │                     │                                             │   │
│  │  ┌──────────────────▼──────────────────┐                         │   │
│  │  │         MAPPING ENGINE              │                         │   │
│  │  │  CPU   → amplitude/roughness        │                         │   │
│  │  │  Net   → tempo/rhythm/density       │                         │   │
│  │  │  Mem   → reverb/decay/cutoff        │                         │   │
│  │  │  Procs → pitch/timbre/pan           │                         │   │
│  │  └──────────────────┬──────────────────┘                         │   │
│  └─────────────────────┼────────────────────────────────────────────┘   │
│                        │  OSC messages (UDP localhost)                  │
│          ┌─────────────┴──────────────────┐                            │
│          │                                │                            │
│          ▼                                ▼                            │
│  ┌───────────────────┐         ┌──────────────────────┐               │
│  │  AUDIO BACKEND    │         │  VISUAL RENDERER     │               │
│  │                   │         │                      │               │
│  │  SuperCollider    │         │  Ratatui (TUI)       │               │
│  │  or PureData      │         │  or Bevy (3D)        │               │
│  │  or Rust Synth    │         │                      │               │
│  └───────────────────┘         └──────────────────────┘               │
└─────────────────────────────────────────────────────────────────────────┘

3.2 Data Flow (Step by Step)

text

Step 1:  eBPF programs attach to sched, net, and mm tracepoints at startup
Step 2:  Kernel events fire; eBPF programs aggregate counters IN KERNEL
Step 3:  At configured rate (10–60 Hz), snapshot structs are written to BPF ring buffer
Step 4:  Conductor's ring buffer poller wakes, reads snapshot batch
Step 5:  Aggregator merges snapshots into rolling metric windows (250ms, 1s, 5s)
Step 6:  Scene Engine evaluates global system state → assigns macro scene
         Scenes: Calm | Busy | Intense | Storm | Critical
Step 7:  Voice Allocator maps top-N processes to synth voices
         Stable voice identity = hash(pid + exe_path + cgroup)
Step 8:  Mapping Engine computes per-voice and global OSC control values
         e.g. voice.amplitude = log_scale(cpu_pct) * scene_scale_factor
Step 9:  OSC messages dispatched to audio backend (UDP, localhost)
Step 10: SuperCollider/PureData/Rust Synth updates running synthesis graph
Step 11: Simultaneously, visual state is pushed to Ratatui or Bevy renderer
Step 12: All metrics and scene transitions logged to SQLite (configurable)

3.3 Component Ownership
Component	Language	Owns
eBPF Programs	Rust (aya-bpf)	Kernel instrumentation, in-kernel aggregation
Ring Buffer Consumer	Rust (aya)	Kernel → userspace event transport
Conductor / Aggregator	Rust	Metric windowing, smoothing, normalization
Scene Engine	Rust	Macro state machine (Calm → Storm etc.)
Voice Allocator	Rust	Process → synth voice mapping, LRU pool
Mapping Engine	Rust	Metric → musical parameter translation
OSC Dispatcher	Rust (rosc)	UDP OSC message construction + dispatch
Audio Backend	SuperCollider / PD / Rust	Actual sound synthesis and output
TUI Renderer	Rust (Ratatui)	Terminal visualizer
3D Renderer	Rust (Bevy)	Spatial process constellation visualizer
Config System	Rust (serde/toml)	TOML config load, validation, hot-reload
Storage	Rust (rusqlite)	Metric history, session log, voice registry
4. Module Breakdown
4.1 eBPF Instrumentation Layer (Rust + Aya)
Purpose

The eBPF layer is the ground truth data source. It attaches to kernel tracepoints and kprobes to observe process lifecycle, CPU accounting, network traffic, and memory pressure — without modifying any kernel behavior. All aggregation happens inside the kernel before data is surfaced to userspace.
Why Aya (Not BCC)

Aya is a Rust-native eBPF library that does not depend on libbpf or LLVM at runtime. It compiles eBPF programs as Rust code targeting the bpfel-unknown-none target, loads them at runtime, and manages map and program lifecycles via a safe Rust API. This means:

    Single static Rust binary for the entire Conductor
    No Python runtime, no LLVM headers, no BCC install required
    Full Rust type safety across kernel/userspace boundary (shared structs via aya-bpf-bindings)
    eBPF programs and userspace code share types from a common daemon-choir-common crate

eBPF Programs
Program 1: Process Monitor (sched_process_events)

Rust

// daemon-choir-ebpf/src/process_monitor.rs

use aya_bpf::{
    macros::{tracepoint, map},
    maps::RingBuf,
    programs::TracePointContext,
    BpfContext,
};
use daemon_choir_common::{ProcessEvent, ProcessEventKind};

#[map]
static PROCESS_EVENTS: RingBuf = RingBuf::with_byte_size(1024 * 256, 0); // 256KB ring

// Fires on every process fork
#[tracepoint(name = "sched_process_fork")]
pub fn on_process_fork(ctx: TracePointContext) -> u32 {
    match try_process_fork(ctx) {
        Ok(ret) => ret,
        Err(_) => 0,
    }
}

fn try_process_fork(ctx: TracePointContext) -> Result<u32, i64> {
    let child_pid: u32 = unsafe { ctx.read_at(40)? }; // child_pid offset in tracepoint args
    let parent_pid: u32 = unsafe { ctx.read_at(8)? };

    if let Some(mut entry) = PROCESS_EVENTS.reserve::<ProcessEvent>(0) {
        let event = entry.as_mut_ptr();
        unsafe {
            (*event).kind = ProcessEventKind::Fork;
            (*event).pid = child_pid;
            (*event).parent_pid = parent_pid;
            (*event).timestamp_ns = bpf_ktime_get_ns();
        }
        entry.submit(0);
    }
    Ok(0)
}

// Fires on exec (new program replacing process image)
#[tracepoint(name = "sched_process_exec")]
pub fn on_process_exec(ctx: TracePointContext) -> u32 {
    let pid: u32 = bpf_get_current_pid_tgid() as u32;
    // Read comm (process name) from task_struct via bpf_get_current_comm
    let mut comm = [0u8; 16];
    unsafe { bpf_get_current_comm(&mut comm as *mut _ as *mut _, 16) };

    if let Some(mut entry) = PROCESS_EVENTS.reserve::<ProcessEvent>(0) {
        let event = entry.as_mut_ptr();
        unsafe {
            (*event).kind = ProcessEventKind::Exec;
            (*event).pid = pid;
            (*event).comm = comm;
            (*event).timestamp_ns = bpf_ktime_get_ns();
        }
        entry.submit(0);
    }
    0
}

// Fires on process exit
#[tracepoint(name = "sched_process_exit")]
pub fn on_process_exit(ctx: TracePointContext) -> u32 {
    let pid: u32 = bpf_get_current_pid_tgid() as u32;
    if let Some(mut entry) = PROCESS_EVENTS.reserve::<ProcessEvent>(0) {
        let event = entry.as_mut_ptr();
        unsafe {
            (*event).kind = ProcessEventKind::Exit;
            (*event).pid = pid;
            (*event).timestamp_ns = bpf_ktime_get_ns();
        }
        entry.submit(0);
    }
    0
}

Program 2: CPU Accounting (cpu_accounting)

Rather than emitting per-context-switch events (which would flood the ring buffer), this program maintains per-PID CPU time accumulators in a BPF hash map and emits snapshot diffs at a controlled rate.

Rust

// daemon-choir-ebpf/src/cpu_accounting.rs

use aya_bpf::{
    macros::{tracepoint, map},
    maps::{HashMap, RingBuf},
};
use daemon_choir_common::CpuSnapshot;

#[map]
static CPU_ACCUMULATORS: HashMap<u32, u64> = HashMap::with_max_entries(4096, 0); // pid → ns on cpu

#[map]
static CPU_SNAPSHOTS: RingBuf = RingBuf::with_byte_size(1024 * 64, 0);

// Fires when a task is scheduled OFF the CPU — we know how long it ran
#[tracepoint(name = "sched_stat_runtime")]
pub fn on_sched_runtime(ctx: TracePointContext) -> u32 {
    let pid: u32 = unsafe { ctx.read_at(8).unwrap_or(0) };
    let runtime_ns: u64 = unsafe { ctx.read_at(24).unwrap_or(0) };

    // Accumulate CPU time per PID
    let current = CPU_ACCUMULATORS.get(&pid).copied().unwrap_or(0);
    let _ = CPU_ACCUMULATORS.insert(&pid, &(current + runtime_ns), 0);

    0
}

Program 3: Network Traffic Counter (net_traffic)

Rust

// daemon-choir-ebpf/src/net_traffic.rs

use aya_bpf::{
    macros::{kprobe, map},
    maps::{PerCpuArray, RingBuf},
};
use daemon_choir_common::NetSnapshot;

// Per-CPU counters to avoid lock contention
#[map]
static NET_TX_BYTES: PerCpuArray<u64> = PerCpuArray::with_max_entries(1, 0);
#[map]
static NET_TX_PACKETS: PerCpuArray<u64> = PerCpuArray::with_max_entries(1, 0);
#[map]
static NET_RX_BYTES: PerCpuArray<u64> = PerCpuArray::with_max_entries(1, 0);
#[map]
static NET_RX_PACKETS: PerCpuArray<u64> = PerCpuArray::with_max_entries(1, 0);
#[map]
static UNIQUE_REMOTE_IPS: HashMap<u32, u8> = HashMap::with_max_entries(1024, 0);

#[map]
static NET_SNAPSHOTS: RingBuf = RingBuf::with_byte_size(1024 * 32, 0);

#[kprobe(name = "dev_queue_xmit")]
pub fn on_tx(ctx: KProbeContext) -> u32 {
    // Increment TX counters
    if let Some(counter) = NET_TX_PACKETS.get_ptr_mut(0) {
        unsafe { *counter += 1 };
    }
    // Read skb->len for byte count
    // ... skb struct offset navigation via ctx.arg(0)
    0
}

Program 4: Memory Pressure (mm_pressure)

Rust

// daemon-choir-ebpf/src/mm_pressure.rs

use aya_bpf::{macros::{tracepoint, map}, maps::RingBuf};
use daemon_choir_common::MemSnapshot;

#[map]
static MEM_EVENTS: RingBuf = RingBuf::with_byte_size(1024 * 32, 0);

// Major page fault = process had to wait for disk I/O to satisfy memory request
#[tracepoint(name = "exceptions_page_fault_kernel")]
pub fn on_major_fault(ctx: TracePointContext) -> u32 {
    // Only count major faults (bit 2 of error_code = 1)
    let error_code: u64 = unsafe { ctx.read_at(16).unwrap_or(0) };
    if error_code & 4 == 0 { return 0; } // minor fault, skip

    if let Some(mut entry) = MEM_EVENTS.reserve::<MemSnapshot>(0) {
        let event = entry.as_mut_ptr();
        unsafe {
            (*event).major_fault = true;
            (*event).pid = bpf_get_current_pid_tgid() as u32;
            (*event).timestamp_ns = bpf_ktime_get_ns();
        }
        entry.submit(0);
    }
    0
}

Shared Types (Common Crate)

Rust

// daemon-choir-common/src/lib.rs

#![no_std]

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ProcessEvent {
    pub kind: ProcessEventKind,
    pub pid: u32,
    pub parent_pid: u32,
    pub comm: [u8; 16],
    pub timestamp_ns: u64,
}

#[repr(u8)]
#[derive(Clone, Copy, PartialEq)]
pub enum ProcessEventKind {
    Fork = 0,
    Exec = 1,
    Exit = 2,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct CpuSnapshot {
    pub pid: u32,
    pub cpu_ns_delta: u64,    // CPU nanoseconds since last snapshot
    pub timestamp_ns: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct NetSnapshot {
    pub tx_bytes_delta: u64,
    pub rx_bytes_delta: u64,
    pub tx_packets_delta: u64,
    pub rx_packets_delta: u64,
    pub unique_remote_ips: u32,
    pub timestamp_ns: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct MemSnapshot {
    pub free_pages: u64,
    pub major_faults_delta: u64,
    pub reclaim_delta: u64,
    pub swap_in_delta: u64,
    pub major_fault: bool,
    pub pid: u32,
    pub timestamp_ns: u64,
}

// Safe to send across FFI boundary
unsafe impl Send for ProcessEvent {}
unsafe impl Send for CpuSnapshot {}
unsafe impl Send for NetSnapshot {}
unsafe impl Send for MemSnapshot {}

4.2 Event Transport & Ring Buffer
Purpose

The BPF ring buffer is the data highway between the kernel eBPF programs and the userspace Conductor. It was chosen over the older BPF_MAP_TYPE_PERF_EVENT_ARRAY because it provides a single shared ring buffer across all CPUs, delivers in-order events, and supports self-pacing wakeups that prevent the consumer from being notified faster than it can drain the buffer.
Kernel Minimum Requirement

BPF ring buffer (BPF_MAP_TYPE_RINGBUF) was introduced in Linux 5.8. Daemon Choir detects the kernel version at startup and falls back to /proc polling on older kernels.
Ring Buffer Consumer

Rust

// conductor/src/transport/ring_consumer.rs

use aya::maps::RingBuf;
use aya::Ebpf;
use daemon_choir_common::{ProcessEvent, CpuSnapshot, NetSnapshot, MemSnapshot};
use tokio::sync::mpsc;

pub struct RingConsumer {
    process_rx: mpsc::Sender<ProcessEvent>,
    cpu_rx: mpsc::Sender<CpuSnapshot>,
    net_rx: mpsc::Sender<NetSnapshot>,
    mem_rx: mpsc::Sender<MemSnapshot>,
}

impl RingConsumer {
    pub async fn run(bpf: &mut Ebpf, senders: EventSenders) -> anyhow::Result<()> {
        let mut process_rb = RingBuf::try_from(bpf.map_mut("PROCESS_EVENTS")?)?;
        let mut cpu_rb = RingBuf::try_from(bpf.map_mut("CPU_SNAPSHOTS")?)?;
        let mut net_rb = RingBuf::try_from(bpf.map_mut("NET_SNAPSHOTS")?)?;
        let mut mem_rb = RingBuf::try_from(bpf.map_mut("MEM_EVENTS")?)?;

        // Use AsyncFd to wake tokio task when ring buffer has data
        let process_fd = AsyncFd::new(process_rb.as_raw_fd())?;
        let cpu_fd = AsyncFd::new(cpu_rb.as_raw_fd())?;

        loop {
            tokio::select! {
                // Process events
                guard = process_fd.readable() => {
                    let mut guard = guard?;
                    while let Some(item) = process_rb.next() {
                        if item.len() == std::mem::size_of::<ProcessEvent>() {
                            let event: ProcessEvent = unsafe { std::ptr::read(item.as_ptr() as *const _) };
                            let _ = senders.process.send(event).await;
                        }
                    }
                    guard.clear_ready();
                }

                // CPU snapshots — emitted at fixed rate by userspace snapshot timer
                guard = cpu_fd.readable() => {
                    let mut guard = guard?;
                    while let Some(item) = cpu_rb.next() {
                        let snap: CpuSnapshot = unsafe { std::ptr::read(item.as_ptr() as *const _) };
                        let _ = senders.cpu.send(snap).await;
                    }
                    guard.clear_ready();
                }
            }
        }
    }
}

Snapshot Emission Timer

To avoid flooding the ring buffer with per-event writes, a userspace timer triggers a periodic snapshot flush from in-kernel accumulators into the ring buffer. This is the key rate-control mechanism:

Rust

// conductor/src/transport/snapshot_timer.rs

use tokio::time::{interval, Duration};

pub struct SnapshotTimer {
    /// Hz at which to emit kernel snapshots (default: 20)
    rate_hz: u32,
    bpf: Arc<Mutex<Ebpf>>,
}

impl SnapshotTimer {
    pub async fn run(&self) {
        let period = Duration::from_millis(1000 / self.rate_hz as u64);
        let mut ticker = interval(period);
        loop {
            ticker.tick().await;
            self.trigger_snapshot_flush().await;
        }
    }

    async fn trigger_snapshot_flush(&self) {
        let bpf = self.bpf.lock().await;
        // Write a sentinel value to a BPF array map that the eBPF program
        // polls to know when to flush accumulators → ring buffer
        if let Ok(mut flush_trigger) = bpf.map_mut("FLUSH_TRIGGER") {
            let mut array = Array::<_, u64>::try_from(flush_trigger).unwrap();
            let ts = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos() as u64;
            let _ = array.set(0, ts, 0);
        }
    }
}

4.3 Conductor Engine (Aggregation & Mapping)
Purpose

The Conductor is the brain of Daemon Choir. It receives raw kernel snapshots, maintains rolling metric windows, normalizes values to musical parameter ranges, and dispatches OSC control messages to the audio backend at a stable cadence.
Aggregator

Rust

// conductor/src/aggregator.rs

use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

/// Rolling window of metric samples for a single process
#[derive(Default)]
pub struct ProcessMetricWindow {
    pub pid: u32,
    pub exe_hash: u64,           // stable identity even across restart
    pub cpu_samples: VecDeque<(Instant, f64)>,  // (timestamp, cpu_fraction 0.0-1.0)
    pub ctx_switches: VecDeque<u64>,
    pub last_seen: Instant,
}

impl ProcessMetricWindow {
    const WINDOW_DURATION: Duration = Duration::from_secs(1);

    pub fn push_cpu_sample(&mut self, cpu_fraction: f64) {
        let now = Instant::now();
        self.cpu_samples.push_back((now, cpu_fraction));
        self.prune(now);
        self.last_seen = now;
    }

    fn prune(&mut self, now: Instant) {
        while let Some(&(ts, _)) = self.cpu_samples.front() {
            if now.duration_since(ts) > Self::WINDOW_DURATION {
                self.cpu_samples.pop_front();
            } else {
                break;
            }
        }
    }

    /// Average CPU fraction over the rolling window
    pub fn cpu_avg(&self) -> f64 {
        if self.cpu_samples.is_empty() { return 0.0; }
        self.cpu_samples.iter().map(|(_, c)| c).sum::<f64>() / self.cpu_samples.len() as f64
    }

    /// Coefficient of variation — high = periodic/rhythmic behavior
    pub fn cpu_burstiness(&self) -> f64 {
        let avg = self.cpu_avg();
        if avg < 0.001 { return 0.0; }
        let variance = self.cpu_samples.iter()
            .map(|(_, c)| (c - avg).powi(2))
            .sum::<f64>() / self.cpu_samples.len() as f64;
        variance.sqrt() / avg
    }
}

/// Global system-level metric aggregator
pub struct Aggregator {
    pub processes: HashMap<u32, ProcessMetricWindow>,
    pub net_window: NetMetricWindow,
    pub mem_window: MemMetricWindow,
    pub snapshot_rate_hz: u32,
}

impl Aggregator {
    pub fn ingest_cpu_snapshot(&mut self, snap: CpuSnapshot) {
        let window_s = 1.0 / self.snapshot_rate_hz as f64;
        let cpu_fraction = (snap.cpu_ns_delta as f64 / 1e9) / window_s;
        let cpu_fraction = cpu_fraction.clamp(0.0, 1.0);

        let entry = self.processes.entry(snap.pid).or_default();
        entry.pid = snap.pid;
        entry.push_cpu_sample(cpu_fraction);
    }

    pub fn ingest_net_snapshot(&mut self, snap: NetSnapshot) {
        self.net_window.push(snap);
    }

    pub fn ingest_mem_snapshot(&mut self, snap: MemSnapshot) {
        self.mem_window.push(snap);
    }

    /// Returns top-N processes sorted by CPU usage (for voice allocation)
    pub fn top_n_processes(&self, n: usize) -> Vec<(u32, f64)> {
        let mut ranked: Vec<(u32, f64)> = self.processes.iter()
            .map(|(pid, w)| (*pid, w.cpu_avg()))
            .collect();
        ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        ranked.truncate(n);
        ranked
    }

    /// Evict processes not seen in last 5 seconds
    pub fn evict_stale(&mut self) {
        let cutoff = Instant::now() - Duration::from_secs(5);
        self.processes.retain(|_, w| w.last_seen > cutoff);
    }
}

Scene Engine (Macro State Machine)

Rust

// conductor/src/scene.rs

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scene {
    Calm,       // Low CPU (<10%), low net, low mem pressure — Eno-esque
    Busy,       // Moderate CPU (10-50%), active network — alive, engaged
    Intense,    // High CPU (50-80%), active net — focused, driving
    Storm,      // Extreme net activity (DDoS-like) — thunderstorm
    Critical,   // OOM/swap pressure or CPU >90% sustained — urgent, discordant
}

pub struct SceneEngine {
    pub current: Scene,
    /// Hysteresis: require N consecutive ticks in new scene before transitioning
    candidate: Option<(Scene, u32)>,
    hysteresis_ticks: u32,
}

impl SceneEngine {
    pub fn evaluate(&mut self, metrics: &GlobalMetrics) -> Option<Scene> {
        let new_scene = self.classify(metrics);
        if new_scene == self.current {
            self.candidate = None;
            return None;
        }
        // Hysteresis prevents rapid scene flapping
        match &mut self.candidate {
            Some((scene, count)) if *scene == new_scene => {
                *count += 1;
                if *count >= self.hysteresis_ticks {
                    let transition = new_scene;
                    self.current = new_scene;
                    self.candidate = None;
                    return Some(transition);
                }
                None
            }
            _ => {
                self.candidate = Some((new_scene, 1));
                None
            }
        }
    }

    fn classify(&self, m: &GlobalMetrics) -> Scene {
        // Critical: OOM imminent or CPU completely saturated
        if m.mem_pressure > 0.9 || m.swap_in_rate > 1000.0 || m.global_cpu > 0.92 {
            return Scene::Critical;
        }
        // Storm: extreme packet rate or unique IP fan-in suggesting DDoS
        if m.net_packets_per_sec > 50_000.0 || m.unique_remote_ips > 500 {
            return Scene::Storm;
        }
        // Intense: high sustained CPU
        if m.global_cpu > 0.5 {
            return Scene::Intense;
        }
        // Busy: moderate activity
        if m.global_cpu > 0.1 || m.net_packets_per_sec > 1000.0 {
            return Scene::Busy;
        }
        Scene::Calm
    }
}

4.4 Audio Synthesis Engine
Purpose

The Audio Synthesis Engine is responsible for actually generating sound. Daemon Choir supports three backends, chosen at startup based on availability. All backends receive OSC control messages from the Conductor over UDP localhost — the synthesis graph itself lives entirely inside the backend.
Backend Selection Logic

Rust

// conductor/src/audio/backend_selector.rs

pub enum AudioBackend {
    SuperCollider(SuperColliderBackend),
    PureData(PureDataBackend),
    RustSynth(RustSynthBackend),
}

pub async fn select_backend(cfg: &AudioConfig) -> anyhow::Result<AudioBackend> {
    // 1. Try user-configured backend first
    match cfg.preferred_backend.as_str() {
        "supercollider" => {
            if let Ok(b) = SuperColliderBackend::connect(cfg).await {
                log::info!("Audio backend: SuperCollider");
                return Ok(AudioBackend::SuperCollider(b));
            }
            log::warn!("SuperCollider not available, trying PureData...");
        }
        "puredata" => { /* similar */ }
        "rust" => {
            return Ok(AudioBackend::RustSynth(RustSynthBackend::new(cfg)?));
        }
        _ => {}
    }

    // 2. Auto-detect
    if SuperColliderBackend::is_available().await {
        return Ok(AudioBackend::SuperCollider(SuperColliderBackend::connect(cfg).await?));
    }
    if PureDataBackend::is_available() {
        return Ok(AudioBackend::PureData(PureDataBackend::connect(cfg).await?));
    }

    // 3. Always-available fallback
    log::info!("Audio backend: built-in Rust soft-synth");
    Ok(AudioBackend::RustSynth(RustSynthBackend::new(cfg)?))
}

SuperCollider Synthesis Graph

The SuperCollider server receives OSC messages and maintains a running SynthDef graph. The Conductor never rebuilds the graph — it only sends parameter updates.

supercollider

// audio/sc/daemon_choir.scd

// ========== BOOT ==========
s.waitForBoot {
    // ===== GLOBAL FX BUS =====
    ~reverbBus = Bus.audio(s, 2);
    ~masterBus = Bus.audio(s, 2);

    // Master reverb — controlled by memory pressure
    SynthDef(\daemonReverb, {
        |in = 0, out = 0, room = 0.3, damp = 0.5, mix = 0.2, lpf = 8000|
        var sig = In.ar(in, 2);
        var wet = FreeVerb2.ar(sig[0], sig[1], mix, room, damp);
        var filtered = LPF.ar(wet, lpf);
        Out.ar(out, filtered);
    }).add;

    // Per-process voice — one running instance per allocated voice
    SynthDef(\processVoice, {
        |out = 0, freq = 220, amp = 0.0, pan = 0.0,
         waveform = 0,    // 0=sine, 1=tri, 2=saw, 3=noise
         roughness = 0.0, // ring modulation depth
         mod_freq = 1.0,  // LFO rate
         attack = 0.5, release = 1.0|

        var osc = Select.ar(waveform, [
            SinOsc.ar(freq),
            LFTri.ar(freq),
            LFSaw.ar(freq),
            WhiteNoise.ar,
        ]);

        // Roughness: CPU burstiness drives ring modulation
        var ring_mod = SinOsc.ar(freq * (1 + roughness * 3));
        var sig = osc * (1 - roughness) + (osc * ring_mod * roughness);

        // Amplitude envelope — glide to new amp over ~250ms
        var amp_lag = Lag.kr(amp, 0.25);
        var freq_lag = Lag.kr(freq, 0.5);

        sig = sig * amp_lag;
        sig = Pan2.ar(sig, pan);
        Out.ar(out, sig);
    }).add;

    // Network rhythm engine — drives percussive/rhythmic layer
    SynthDef(\netRhythm, {
        |out = 0, tempo_bpm = 60, density = 0.3, chaos = 0.0, amp = 0.3|
        var trig = Impulse.kr(tempo_bpm / 60.0);
        // Density: sub-divides beat with probability
        var dense_trig = trig + (CoinGate.kr(density, Dust.kr(tempo_bpm / 15.0)));
        var env = EnvGen.ar(Env.perc(0.005, 0.15), dense_trig);
        var noise = BPF.ar(
            WhiteNoise.ar,
            freq: LFNoise1.kr(chaos * 4 + 0.1).range(200, 800),
            rq: 0.3
        );
        Out.ar(out, Pan2.ar(noise * env * amp, 0));
    }).add;

    // Thunder burst — DDoS/Storm scene macro event
    SynthDef(\thunderBurst, {
        |out = 0, intensity = 0.8|
        var env = EnvGen.ar(Env([0,1,0.6,0], [0.01, 0.3, 2.0]), doneAction: 2);
        var rumble = LPF.ar(BrownNoise.ar * env * intensity, 200);
        var crack  = HPF.ar(WhiteNoise.ar * EnvGen.ar(Env.perc(0.001, 0.05), doneAction: Done.none) * intensity * 0.5, 3000);
        Out.ar(out, Pan2.ar(rumble + crack, 0));
    }).add;

    s.sync;

    // ===== INSTANTIATE GLOBAL SYNTHS =====
    ~reverbSynth = Synth(\daemonReverb, [\in, ~reverbBus, \out, ~masterBus]);
    ~netRhythm   = Synth(\netRhythm, [\out, ~reverbBus]);

    // Voice pool: 32 pre-allocated voice slots, initially silent
    ~voices = Array.fill(32, { |i|
        Synth(\processVoice, [\out, ~reverbBus, \amp, 0.0, \freq, 110 * (i + 1)])
    });

    // ===== OSC RESPONDERS =====

    // Update a voice parameter: /voice/set [voice_id, param_name, value]
    OSCdef(\voice_set, { |msg|
        var voice_id = msg[1].asInteger;
        var param    = msg[2].asSymbol;
        var value    = msg[3].asFloat;
        ~voices[voice_id].set(param, value);
    }, '/voice/set');

    // Update reverb (memory pressure)
    OSCdef(\reverb_set, { |msg|
        ~reverbSynth.set(\room, msg[1].asFloat, \mix, msg[2].asFloat,
                         \damp, msg[3].asFloat, \lpf, msg[4].asFloat);
    }, '/reverb/set');

    // Update net rhythm
    OSCdef(\net_rhythm, { |msg|
        ~netRhythm.set(\tempo_bpm, msg[1].asFloat, \density, msg[2].asFloat,
                       \chaos, msg[3].asFloat, \amp, msg[4].asFloat);
    }, '/net/rhythm');

    // Trigger one-shot thunder burst
    OSCdef(\thunder, { |msg|
        Synth(\thunderBurst, [\out, ~reverbBus, \intensity, msg[1].asFloat]);
    }, '/scene/thunder');

    // Scene transition global blend
    OSCdef(\scene_change, { |msg|
        var scene = msg[1].asSymbol;
        var blend_time = 3.0;
        switch(scene,
            \calm,     { ~reverbSynth.set(\room, 0.2, \mix, 0.15) },
            \busy,     { ~reverbSynth.set(\room, 0.35, \mix, 0.25) },
            \intense,  { ~reverbSynth.set(\room, 0.5, \mix, 0.4) },
            \storm,    { ~reverbSynth.set(\room, 0.8, \mix, 0.7); Synth(\thunderBurst, [\intensity, 0.9]) },
            \critical, { ~reverbSynth.set(\room, 0.9, \mix, 0.85, \lpf, 2000) }
        );
    }, '/scene/change');

    "Daemon Choir SC Server ready".postln;
};

OSC Dispatcher (Rust)

Rust

// conductor/src/audio/osc_dispatcher.rs

use rosc::{OscMessage, OscPacket, OscType, encoder};
use std::net::UdpSocket;

pub struct OscDispatcher {
    socket: UdpSocket,
    target_addr: std::net::SocketAddr,
}

impl OscDispatcher {
    pub fn new(target: &str) -> anyhow::Result<Self> {
        let socket = UdpSocket::bind("0.0.0.0:0")?;
        Ok(Self {
            socket,
            target_addr: target.parse()?,
        })
    }

    pub fn send_voice_param(&self, voice_id: u32, param: &str, value: f32) -> anyhow::Result<()> {
        self.send_message("/voice/set", vec![
            OscType::Int(voice_id as i32),
            OscType::String(param.to_string()),
            OscType::Float(value),
        ])
    }

    pub fn send_reverb(&self, room: f32, mix: f32, damp: f32, lpf: f32) -> anyhow::Result<()> {
        self.send_message("/reverb/set", vec![
            OscType::Float(room),
            OscType::Float(mix),
            OscType::Float(damp),
            OscType::Float(lpf),
        ])
    }

    pub fn send_net_rhythm(&self, bpm: f32, density: f32, chaos: f32, amp: f32) -> anyhow::Result<()> {
        self.send_message("/net/rhythm", vec![
            OscType::Float(bpm),
            OscType::Float(density),
            OscType::Float(chaos),
            OscType::Float(amp),
        ])
    }

    pub fn send_thunder(&self, intensity: f32) -> anyhow::Result<()> {
        self.send_message("/scene/thunder", vec![OscType::Float(intensity)])
    }

    pub fn send_scene_change(&self, scene: &str) -> anyhow::Result<()> {
        self.send_message("/scene/change", vec![OscType::String(scene.to_string())])
    }

    fn send_message(&self, addr: &str, args: Vec<OscType>) -> anyhow::Result<()> {
        let msg = OscPacket::Message(OscMessage {
            addr: addr.to_string(),
            args,
        });
        let buf = encoder::encode(&msg)?;
        self.socket.send_to(&buf, self.target_addr)?;
        Ok(())
    }
}

4.5 Visual Renderer
Purpose

The visual renderer provides a secondary sensory channel alongside audio. Two modes:

    Ratatui (TUI): A terminal UI for debugging and low-resource environments. Shows active voices, live metric bars, scene state, and dropped event counters.
    Bevy (3D): A real-time 3D "process constellation" where each process is a glowing entity in space, network flows are particle streams, and memory pressure is volumetric fog.

TUI Renderer (Ratatui)

Rust

// conductor/src/visual/tui.rs

use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    widgets::{BarChart, Block, Borders, Gauge, Paragraph, Table, Row, Cell},
    Terminal,
};

pub struct TuiRenderer {
    terminal: Terminal<CrosstermBackend<std::io::Stdout>>,
}

impl TuiRenderer {
    pub fn render(&mut self, state: &ConductorState) -> anyhow::Result<()> {
        self.terminal.draw(|f| {
            let size = f.size();

            // Layout: top bar (scene) | middle (voices + metrics) | bottom (network)
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3),   // Scene banner
                    Constraint::Min(10),     // Main content
                    Constraint::Length(5),   // Network rhythm
                ].as_ref())
                .split(size);

            // ===== SCENE BANNER =====
            let scene_color = match state.scene {
                Scene::Calm     => Color::Cyan,
                Scene::Busy     => Color::Green,
                Scene::Intense  => Color::Yellow,
                Scene::Storm    => Color::Magenta,
                Scene::Critical => Color::Red,
            };
            let scene_block = Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(scene_color))
                .title(format!(" 🎵 DAEMON CHOIR  |  Scene: {:?} ", state.scene));
            f.render_widget(scene_block, chunks[0]);

            // ===== VOICE TABLE =====
            let main_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(60), Constraint::Percentage(40)].as_ref())
                .split(chunks[1]);

            let rows: Vec<Row> = state.voices.iter().map(|v| {
                let bar = cpu_bar(v.cpu_avg);
                Row::new(vec![
                    Cell::from(v.pid.to_string()),
                    Cell::from(v.comm.clone()),
                    Cell::from(bar).style(Style::default().fg(Color::Green)),
                    Cell::from(format!("{:.0} Hz", v.freq)),
                    Cell::from(format!("{:.2}", v.amp)),
                ])
            }).collect();

            let voice_table = Table::new(rows, [
                Constraint::Length(8),
                Constraint::Length(16),
                Constraint::Length(12),
                Constraint::Length(8),
                Constraint::Length(6),
            ])
            .header(Row::new(vec!["PID", "PROCESS", "CPU", "FREQ", "AMP"])
                .style(Style::default().add_modifier(Modifier::BOLD)))
            .block(Block::default().borders(Borders::ALL).title(" Active Voices "));
            f.render_widget(voice_table, main_chunks[0]);

            // ===== METRIC GAUGES =====
            let gauge_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3),
                    Constraint::Length(3),
                    Constraint::Length(3),
                    Constraint::Min(0),
                ].as_ref())
                .split(main_chunks[1]);

            let cpu_gauge = Gauge::default()
                .block(Block::default().borders(Borders::ALL).title(" Global CPU "))
                .gauge_style(Style::default().fg(Color::Yellow))
                .ratio(state.global_cpu as f64);
            f.render_widget(cpu_gauge, gauge_chunks[0]);

            let mem_gauge = Gauge::default()
                .block(Block::default().borders(Borders::ALL).title(" Memory Pressure "))
                .gauge_style(Style::default().fg(Color::Red))
                .ratio(state.mem_pressure as f64);
            f.render_widget(mem_gauge, gauge_chunks[1]);

            // ===== NETWORK BAR =====
            let net_data = [
                ("TX pkts/s", state.net.tx_pps as u64),
                ("RX pkts/s", state.net.rx_pps as u64),
                ("BPM", state.net.current_bpm as u64),
            ];
            let net_bar = BarChart::default()
                .block(Block::default().borders(Borders::ALL).title(" Network Rhythm "))
                .bar_width(12)
                .data(&net_data);
            f.render_widget(net_bar, chunks[2]);
        })?;
        Ok(())
    }
}

fn cpu_bar(fraction: f32) -> String {
    let filled = (fraction * 10.0) as usize;
    format!("[{}{}] {:.0}%", "█".repeat(filled), "░".repeat(10 - filled), fraction * 100.0)
}

3D Renderer (Bevy)

Rust

// conductor/src/visual/bevy_renderer.rs

use bevy::prelude::*;

/// Marker component for process entities
#[derive(Component)]
pub struct ProcessNode {
    pub pid: u32,
    pub voice_id: usize,
}

/// Global scene state resource
#[derive(Resource)]
pub struct DaemonChoirState {
    pub current_scene: Scene,
    pub mem_pressure: f32,
    pub net_bpm: f32,
    pub voices: Vec<VoiceState>,
}

pub fn spawn_process_node(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    voice: &VoiceState,
) {
    // Position: place in 3D space based on voice identity hash
    let angle = (voice.voice_id as f32 / 32.0) * std::f32::consts::TAU;
    let radius = 4.0 + (voice.cpu_avg * 2.0);
    let pos = Vec3::new(
        angle.cos() * radius,
        (voice.freq / 440.0).log2() * 2.0,  // Height = pitch register
        angle.sin() * radius,
    );

    // Color: timbre family → hue
    let hue = (voice.exe_hash % 360) as f32;
    let color = Color::hsl(hue, 0.7, 0.4 + voice.amp * 0.4);

    // Size: CPU usage → sphere radius
    let sphere_r = 0.1 + voice.cpu_avg * 0.5;

    commands.spawn((
        PbrBundle {
            mesh: meshes.add(Sphere::new(sphere_r)),
            material: materials.add(StandardMaterial {
                base_color: color,
                emissive: color * 2.0,
                ..default()
            }),
            transform: Transform::from_translation(pos),
            ..default()
        },
        ProcessNode { pid: voice.pid, voice_id: voice.voice_id },
    ));
}

/// System: update node sizes and colors from state each frame
pub fn update_process_nodes(
    state: Res<DaemonChoirState>,
    mut query: Query<(&ProcessNode, &mut Transform, &Handle<StandardMaterial>)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (node, mut transform, mat_handle) in query.iter_mut() {
        if let Some(voice) = state.voices.iter().find(|v| v.voice_id == node.voice_id) {
            // Pulse size with CPU
            let target_scale = 1.0 + voice.cpu_avg * 3.0;
            transform.scale = transform.scale.lerp(Vec3::splat(target_scale), 0.1);

            // Update emissive intensity
            if let Some(mat) = materials.get_mut(mat_handle) {
                let intensity = 1.0 + voice.amp * 4.0;
                mat.emissive = mat.base_color * intensity;
            }
        }
    }
}

/// System: update fog density from memory pressure
pub fn update_fog(
    state: Res<DaemonChoirState>,
    mut fog: ResMut<FogSettings>,
) {
    let target_density = state.mem_pressure * 0.15;
    fog.color = Color::rgba(0.1, 0.08, 0.15, 1.0);
    fog.falloff = FogFalloff::Exponential { density: target_density };
}

4.6 Voice Allocation & Timbral Identity System
Purpose

With potentially hundreds of running processes, Daemon Choir maintains a fixed voice pool of 32 slots. Voice allocation decides which processes are currently "singing" and gives each a stable, learnable timbral identity.

Rust

// conductor/src/voice/allocator.rs

use std::collections::HashMap;

const VOICE_POOL_SIZE: usize = 32;

#[derive(Clone, Debug)]
pub struct VoiceAssignment {
    pub voice_id: usize,
    pub pid: u32,
    pub exe_hash: u64,
    pub comm: String,
    pub identity: VoiceIdentity,
    pub cpu_avg: f32,
    pub amp: f32,
    pub freq: f32,
}

/// Stable per-process musical identity derived from exe_hash
#[derive(Clone, Debug)]
pub struct VoiceIdentity {
    pub waveform: u8,        // 0=sine, 1=tri, 2=saw, 3=noise
    pub base_freq: f32,      // Root frequency for this process
    pub pan: f32,            // Stereo position -1.0 to 1.0
    pub filter_band: (f32, f32),  // (lo, hi) spectral band
}

impl VoiceIdentity {
    /// Deterministic: same exe_hash always gets same identity
    pub fn from_exe_hash(hash: u64) -> Self {
        let h = hash;
        // Map hash bands to musical parameters
        let waveform = (h % 4) as u8;
        // Quantize to pentatonic scale degrees (avoids dissonance between voices)
        let scale_degree = (h >> 4) % 12;
        let octave = 3 + ((h >> 8) % 3) as i32;
        let base_freq = pentatonic_freq(scale_degree as u8, octave);
        let pan = ((h >> 12) % 100) as f32 / 50.0 - 1.0;
        Self {
            waveform,
            base_freq,
            pan,
            filter_band: (200.0 + (h >> 16 % 800) as f32, 2000.0 + (h >> 24 % 6000) as f32),
        }
    }
}

fn pentatonic_freq(degree: u8, octave: i32) -> f32 {
    // C major pentatonic ratios: 1, 9/8, 5/4, 3/2, 5/3
    let ratios = [1.0_f32, 1.125, 1.25, 1.5, 1.667];
    let base = 32.703 * 2.0_f32.powi(octave); // C at given octave
    base * ratios[(degree as usize) % 5]
}

pub struct VoiceAllocator {
    /// voice_id → assignment (None = free)
    pool: [Option<VoiceAssignment>; VOICE_POOL_SIZE],
    /// pid → voice_id for quick lookup
    pid_to_voice: HashMap<u32, usize>,
}

impl VoiceAllocator {
    pub fn update(&mut self, top_processes: &[(u32, f64)], process_meta: &HashMap<u32, ProcessMeta>) {
        // 1. Evict voices whose PIDs are no longer in top-N
        let active_pids: std::collections::HashSet<u32> = top_processes.iter().map(|(p, _)| *p).collect();
        for slot in self.pool.iter_mut() {
            if let Some(ref v) = slot {
                if !active_pids.contains(&v.pid) {
                    self.pid_to_voice.remove(&v.pid);
                    *slot = None;
                }
            }
        }

        // 2. Assign free slots to new top-N entries
        for (pid, cpu_avg) in top_processes {
            if self.pid_to_voice.contains_key(pid) { continue; }
            if let Some(free_slot) = self.find_free_slot() {
                let meta = process_meta.get(pid).cloned().unwrap_or_default();
                let identity = VoiceIdentity::from_exe_hash(meta.exe_hash);
                let assignment = VoiceAssignment {
                    voice_id: free_slot,
                    pid: *pid,
                    exe_hash: meta.exe_hash,
                    comm: meta.comm.clone(),
                    identity,
                    cpu_avg: *cpu_avg as f32,
                    amp: 0.0,
                    freq: 0.0,
                };
                self.pool[free_slot] = Some(assignment);
                self.pid_to_voice.insert(*pid, free_slot);
            }
        }

        // 3. Update parameters for existing assignments
        for (pid, cpu_avg) in top_processes {
            if let Some(&vid) = self.pid_to_voice.get(pid) {
                if let Some(ref mut v) = self.pool[vid] {
                    v.cpu_avg = *cpu_avg as f32;
                    v.amp = map_cpu_to_amp(*cpu_avg as f32);
                    v.freq = map_cpu_to_freq(&v.identity, *cpu_avg as f32);
                }
            }
        }
    }

    fn find_free_slot(&self) -> Option<usize> {
        self.pool.iter().position(|s| s.is_none())
    }
}

/// Log-scale CPU → amplitude mapping (compressed, so even busy processes stay musical)
fn map_cpu_to_amp(cpu: f32) -> f32 {
    if cpu < 0.001 { return 0.0; }
    let log_amp = (cpu * 100.0 + 1.0).log10() / 2.0; // log10(101) ≈ 2.0
    (log_amp * 0.7).clamp(0.0, 0.7)
}

/// CPU burstiness → pitch drift (rising tension for sustained load)
fn map_cpu_to_freq(identity: &VoiceIdentity, cpu: f32) -> f32 {
    // Base frequency + upward drift of up to a minor third at 100% CPU
    let drift_semitones = cpu * 3.0;
    identity.base_freq * 2.0_f32.powf(drift_semitones / 12.0)
}

4.7 Scene & Macro Behavior Engine

The Mapping Engine computes OSC parameter updates for every dispatch cycle (default 20 Hz). It respects the current scene to apply scene-level scaling.

Rust

// conductor/src/mapping/engine.rs

pub struct MappingEngine {
    pub dispatch_rate_hz: u32,
    pub preset: MappingPreset,
}

#[derive(Clone)]
pub struct MappingPreset {
    pub name: String,
    /// Max reverb room size (reached at 100% memory pressure)
    pub max_reverb_room: f32,
    /// BPM range for network rhythm [min, max]
    pub net_bpm_range: (f32, f32),
    /// Volume ceiling for the master bus
    pub master_volume: f32,
}

impl MappingPreset {
    pub fn eno_ambient() -> Self {
        Self {
            name: "Eno Ambient".to_string(),
            max_reverb_room: 0.9,
            net_bpm_range: (40.0, 120.0),
            master_volume: 0.6,
        }
    }

    pub fn industrial() -> Self {
        Self {
            name: "Industrial".to_string(),
            max_reverb_room: 0.6,
            net_bpm_range: (80.0, 180.0),
            master_volume: 0.8,
        }
    }

    pub fn minimal() -> Self {
        Self {
            name: "Minimal".to_string(),
            max_reverb_room: 0.4,
            net_bpm_range: (40.0, 90.0),
            master_volume: 0.4,
        }
    }
}

impl MappingEngine {
    /// Called every 1/dispatch_rate_hz seconds
    pub fn compute_dispatch(&self, state: &ConductorState) -> DispatchBundle {
        let scene_mult = scene_multiplier(state.scene);

        // ===== MEMORY → REVERB =====
        let mem = state.mem_pressure.clamp(0.0, 1.0);
        let reverb_room  = mem * self.preset.max_reverb_room;
        let reverb_mix   = 0.1 + mem * 0.6;
        let reverb_damp  = 0.3 + mem * 0.4;
        let reverb_lpf   = 8000.0 - mem * 5000.0; // Dark and heavy at high pressure

        // ===== NETWORK → RHYTHM =====
        let (bpm_min, bpm_max) = self.preset.net_bpm_range;
        let net_norm = (state.net.tx_pps / 10_000.0).clamp(0.0, 1.0);
        let net_bpm      = bpm_min + net_norm * (bpm_max - bpm_min);
        let net_density  = (state.net.burst_coeff * 0.8).clamp(0.0, 1.0);
        let net_chaos    = if state.scene == Scene::Storm { 0.8 } else { net_norm * 0.3 };
        let net_amp      = (0.1 + net_norm * 0.5) * scene_mult;

        // ===== VOICES =====
        let voice_dispatches: Vec<VoiceDispatch> = state.voices.iter().map(|v| {
            let roughness = state.aggregator.get_burstiness(v.pid).clamp(0.0, 1.0);
            VoiceDispatch {
                voice_id: v.voice_id,
                freq: v.freq,
                amp: v.amp * scene_mult,
                pan: v.identity.pan,
                waveform: v.identity.waveform,
                roughness,
            }
        }).collect();

        DispatchBundle {
            reverb: ReverbParams { room: reverb_room, mix: reverb_mix, damp: reverb_damp, lpf: reverb_lpf },
            net_rhythm: NetRhythmParams { bpm: net_bpm, density: net_density, chaos: net_chaos, amp: net_amp },
            voices: voice_dispatches,
            scene_transition: state.pending_scene_transition,
        }
    }
}

fn scene_multiplier(scene: Scene) -> f32 {
    match scene {
        Scene::Calm     => 0.5,
        Scene::Busy     => 0.75,
        Scene::Intense  => 0.9,
        Scene::Storm    => 1.0,
        Scene::Critical => 1.0,
    }
}

4.8 Control API & Configuration Daemon

A lightweight HTTP API runs at localhost:9876 to allow external control, preset switching, and state inspection.

Rust

// conductor/src/api/server.rs

use axum::{Router, routing::{get, post}, Json, extract::State};

pub fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/health",         get(health_handler))
        .route("/state",          get(state_handler))
        .route("/voices",         get(voices_handler))
        .route("/scene",          get(scene_handler))
        .route("/preset",         post(set_preset_handler))
        .route("/volume",         post(set_volume_handler))
        .route("/mute",           post(mute_handler))
        .with_state(state)
}

async fn state_handler(State(s): State<Arc<AppState>>) -> Json<ConductorStateSnapshot> {
    let state = s.conductor_state.read().await;
    Json(ConductorStateSnapshot {
        scene: format!("{:?}", state.scene),
        global_cpu: state.global_cpu,
        mem_pressure: state.mem_pressure,
        net_bpm: state.net.current_bpm,
        active_voices: state.voices.len(),
        uptime_secs: state.uptime.elapsed().as_secs(),
        audio_backend: s.audio_backend_name.clone(),
        ebpf_active: s.ebpf_active,
    })
}

5. Data Models & Schemas
5.1 SQLite Schema

SQL

-- migrations/001_initial.sql

CREATE TABLE IF NOT EXISTS metric_snapshots (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp       DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    global_cpu      REAL NOT NULL,
    mem_pressure    REAL NOT NULL,
    net_tx_pps      REAL NOT NULL,
    net_rx_pps      REAL NOT NULL,
    net_bpm         REAL NOT NULL,
    active_voices   INTEGER NOT NULL,
    scene           TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS scene_transitions (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp       DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    from_scene      TEXT NOT NULL,
    to_scene        TEXT NOT NULL,
    trigger_reason  TEXT,
    duration_ms     INTEGER  -- how long the transition took
);

CREATE TABLE IF NOT EXISTS voice_registry (
    exe_hash        INTEGER PRIMARY KEY,
    comm            TEXT NOT NULL,
    waveform        INTEGER NOT NULL,
    base_freq       REAL NOT NULL,
    pan             REAL NOT NULL,
    first_seen      DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_seen       DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    total_activations INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE IF NOT EXISTS session_log (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    started_at      DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    ended_at        DATETIME,
    audio_backend   TEXT NOT NULL,
    ebpf_active     BOOLEAN NOT NULL,
    preset          TEXT NOT NULL,
    kernel_version  TEXT,
    total_events    INTEGER DEFAULT 0
);

CREATE TABLE IF NOT EXISTS anomaly_log (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp       DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    anomaly_type    TEXT NOT NULL,  -- "runaway_cpu", "memory_oom_risk", "ddos_suspected", "storm_entry"
    pid             INTEGER,
    comm            TEXT,
    metric_value    REAL,
    threshold       REAL,
    scene_at_time   TEXT
);

-- Performance indexes
CREATE INDEX idx_metrics_ts    ON metric_snapshots(timestamp);
CREATE INDEX idx_transitions_ts ON scene_transitions(timestamp);
CREATE INDEX idx_voices_comm   ON voice_registry(comm);

6. API Specifications
6.1 Control API (localhost:9876)

text

GET  /health
     Returns: { status: "ok", uptime_secs, audio_backend, ebpf_active, version }

GET  /state
     Returns: ConductorStateSnapshot {
         scene, global_cpu, mem_pressure, net_bpm,
         active_voices, uptime_secs, audio_backend, ebpf_active
     }

GET  /voices
     Returns: VoiceAssignment[] — current active voice pool

GET  /scene
     Returns: { current: Scene, history: SceneTransition[last 10] }

GET  /metrics/history?minutes=60
     Returns: MetricSnapshot[] — time-series of system metrics

GET  /anomalies?limit=20
     Returns: AnomalyLog[] — recent anomaly events

POST /preset
     Body: { name: "eno_ambient" | "industrial" | "minimal" | "silent" }
     Returns: { success, preset_applied }

POST /volume
     Body: { level: 0.0-1.0 }
     Returns: { success }

POST /mute
     Body: { muted: bool }
     Returns: { success }

POST /scene/force
     Body: { scene: "calm" | "busy" | "intense" | "storm" | "critical" }
     Returns: { success }  -- for testing/demo only; reverts to auto after 30s

GET  /config
     Returns: current config (sanitized, no secrets)

POST /config/reload
     Returns: { success, changes_applied }

6.2 OSC Message Contract (Conductor → Audio Backend)
OSC Address	Arguments	Description
/voice/set	[int voice_id, str param, float value]	Update a single voice parameter
/voice/set_all	[int voice_id, float freq, float amp, float pan, int waveform, float roughness]	Bulk update voice
/reverb/set	[float room, float mix, float damp, float lpf]	Update global reverb
/net/rhythm	[float bpm, float density, float chaos, float amp]	Update network rhythm layer
/scene/change	[str scene_name]	Trigger scene transition with blending
/scene/thunder	[float intensity]	One-shot thunder burst event
/master/volume	[float level]	Master output volume
/master/mute	[int muted]	Mute/unmute all output
/voice/silence	[int voice_id]	Silence a specific voice (process exited)
7. Directory Structure

text

daemon-choir/
├── Cargo.toml                          # Workspace root
├── Cargo.lock
│
├── daemon-choir-common/               # Shared types: eBPF ↔ userspace
│   ├── Cargo.toml
│   └── src/
│       └── lib.rs                     # ProcessEvent, CpuSnapshot, NetSnapshot, MemSnapshot
│
├── daemon-choir-ebpf/                 # eBPF programs (bpfel-unknown-none target)
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs                    # eBPF entry point
│       ├── process_monitor.rs         # sched tracepoints (fork, exec, exit)
│       ├── cpu_accounting.rs          # sched_stat_runtime accumulator
│       ├── net_traffic.rs             # dev_queue_xmit, netif_receive_skb counters
│       └── mm_pressure.rs             # Page fault + reclaim tracepoints
│
├── conductor/                         # Main userspace binary (Rust)
│   ├── Cargo.toml
│   ├── build.rs                       # eBPF compilation step
│   └── src/
│       ├── main.rs                    # Startup: load eBPF, select backend, run loops
│       │
│       ├── transport/
│       │   ├── ring_consumer.rs       # BPF ring buffer → tokio channels
│       │   ├── snapshot_timer.rs      # Periodic flush trigger
│       │   └── proc_fallback.rs       # /proc polling when eBPF unavailable
│       │
│       ├── aggregator/
│       │   ├── mod.rs                 # Aggregator struct
│       │   ├── process_window.rs      # Per-process rolling metric window
│       │   ├── net_window.rs          # Network metric rolling window
│       │   └── mem_window.rs          # Memory metric rolling window
│       │
│       ├── scene/
│       │   ├── engine.rs              # Scene state machine + hysteresis
│       │   └── types.rs               # Scene enum, GlobalMetrics
│       │
│       ├── voice/
│       │   ├── allocator.rs           # Voice pool management
│       │   ├── identity.rs            # Timbral identity from exe_hash
│       │   └── pentatonic.rs          # Scale/pitch helpers
│       │
│       ├── mapping/
│       │   ├── engine.rs              # Metric → OSC parameter mapping
│       │   ├── presets.rs             # Built-in presets (Eno, Industrial, Minimal)
│       │   └── smoothing.rs           # Lag filters, interpolation helpers
│       │
│       ├── audio/
│       │   ├── backend.rs             # AudioBackend trait
│       │   ├── backend_selector.rs    # Auto-detection logic
│       │   ├── osc_dispatcher.rs      # rosc UDP message dispatch
│       │   ├── supercollider.rs       # SC-specific health check + boot
│       │   ├── puredata.rs            # PD-specific health check + boot
│       │   └── rust_synth.rs          # Built-in soft synth (cpal + basic DSP)
│       │
│       ├── visual/
│       │   ├── tui.rs                 # Ratatui terminal renderer
│       │   └── bevy_renderer.rs       # Bevy 3D renderer + ECS systems
│       │
│       ├── api/
│       │   ├── server.rs              # Axum HTTP server
│       │   └── handlers.rs            # Route handlers
│       │
│       ├── storage/
│       │   ├── db.rs                  # SQLite connection (rusqlite)
│       │   ├── migrations.rs          # Migration runner
│       │   ├── metric_repo.rs         # Metric snapshot CRUD
│       │   ├── session_repo.rs        # Session log CRUD
│       │   ├── voice_repo.rs          # Voice registry CRUD
│       │   └── anomaly_repo.rs        # Anomaly log CRUD
│       │
│       ├── config/
│       │   ├── config.rs              # Config struct + TOML loader
│       │   ├── defaults.rs            # Default values
│       │   └── validator.rs           # Config validation
│       │
│       └── state.rs                   # ConductorState shared state (Arc<RwLock<>>)
│
├── audio/
│   ├── sc/
│   │   ├── daemon_choir.scd           # Main SuperCollider synthesis graph
│   │   ├── synthdef/
│   │   │   ├── process_voice.scd      # processVoice SynthDef
│   │   │   ├── net_rhythm.scd         # netRhythm SynthDef
│   │   │   ├── reverb.scd             # daemonReverb SynthDef
│   │   │   └── thunder.scd            # thunderBurst SynthDef
│   │   └── startup.sh                 # sclang / scsynth launch script
│   │
│   └── pd/
│       ├── daemon_choir.pd            # Main PureData patch
│       ├── abstractions/
│       │   ├── process_voice~.pd
│       │   ├── net_rhythm~.pd
│       │   └── reverb~.pd
│       └── startup.sh
│
├── config/
│   └── default.toml                   # Shipped default configuration
│
├── scripts/
│   ├── install.sh                     # One-liner installer
│   ├── install-capabilities.sh        # Set CAP_BPF + CAP_PERFMON on binary
│   └── check-kernel.sh                # Kernel version + eBPF feature check
│
├── Makefile
├── Dockerfile
└── README.md

8. Configuration System
8.1 Config File (TOML)

Stored at ~/.config/daemon-choir/config.toml

toml

[general]
preset = "eno_ambient"           # "eno_ambient" | "industrial" | "minimal" | "silent"
visual_mode = "tui"              # "tui" | "bevy" | "none"
control_api_port = 9876
log_level = "info"

[ebpf]
enabled = true
snapshot_rate_hz = 20            # How many times per second to emit kernel snapshots
process_events = true
cpu_accounting = true
net_traffic = true
mem_pressure = true
# Kernel requirement: 5.8+ for ring buffer; falls back to /proc if false
ringbuf_size_kb = 256

[proc_fallback]
# Used when eBPF is unavailable (kernel < 5.8, no capabilities, macOS, Windows)
enabled = true
poll_rate_hz = 5                 # /proc poll rate (coarser than eBPF)

[audio]
preferred_backend = "auto"       # "auto" | "supercollider" | "puredata" | "rust_synth"
master_volume = 0.7
muted = false

[audio.supercollider]
host = "127.0.0.1"
port = 57120
startup_script = "~/.config/daemon-choir/sc/daemon_choir.scd"
boot_timeout_secs = 10

[audio.puredata]
host = "127.0.0.1"
port = 9000
patch = "~/.config/daemon-choir/pd/daemon_choir.pd"

[audio.rust_synth]
# Built-in fallback — no external dependencies needed
sample_rate = 44100
buffer_size = 512
output_device = "default"

[voices]
pool_size = 32                   # Max simultaneous process voices
top_n_cpu = 28                   # Drive top N CPU consumers as active voices
pinned_processes = []            # Always give these exes a voice: ["sshd", "postgres"]
min_cpu_threshold = 0.001        # Ignore processes below this CPU fraction
# How fast voices respond to CPU changes (seconds)
amp_lag_secs = 0.25
freq_lag_secs = 0.5

[mapping]
# Memory pressure → reverb parameters
mem_reverb_max_room = 0.9
mem_reverb_max_mix = 0.8
mem_reverb_lpf_min = 1500.0     # Darkest filter at max pressure
mem_reverb_lpf_max = 8000.0

# Network → rhythm
net_bpm_min = 40.0
net_bpm_max = 180.0
net_pps_scale = 10000.0         # PPS value that maps to 100% of BPM range
net_burst_density_max = 0.9

# CPU → amplitude (log scale)
cpu_amp_ceiling = 0.7
cpu_roughness_scale = 1.0       # Multiplier for burstiness → ring mod depth

[scene]
hysteresis_ticks = 5            # Ticks before confirming a scene transition
calm_cpu_threshold = 0.10
busy_cpu_threshold = 0.10
intense_cpu_threshold = 0.50
storm_pps_threshold = 50000
critical_cpu_threshold = 0.90
critical_mem_threshold = 0.90

[storage]
db_path = "~/.config/daemon-choir/daemon_choir.db"
log_metrics = true
metrics_retention_days = 7
log_anomalies = true
log_sessions = true

[logging]
level = "info"
file = "~/.config/daemon-choir/daemon_choir.log"
max_size_mb = 50

8.2 Config Loader

Rust

// conductor/src/config/config.rs

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Config {
    pub general:   GeneralConfig,
    pub ebpf:      EbpfConfig,
    pub proc_fallback: ProcFallbackConfig,
    pub audio:     AudioConfig,
    pub voices:    VoiceConfig,
    pub mapping:   MappingConfig,
    pub scene:     SceneConfig,
    pub storage:   StorageConfig,
    pub logging:   LogConfig,
}

impl Config {
    pub fn load(path: Option<PathBuf>) -> anyhow::Result<Self> {
        let path = path.unwrap_or_else(Self::default_path);

        // Write defaults if no config exists
        if !path.exists() {
            let defaults = Self::defaults();
            let toml = toml::to_string_pretty(&defaults)?;
            std::fs::create_dir_all(path.parent().unwrap())?;
            std::fs::write(&path, toml)?;
            return Ok(defaults);
        }

        let raw = std::fs::read_to_string(&path)?;
        let config: Config = toml::from_str(&raw)
            .map_err(|e| anyhow::anyhow!("Config parse error at {}: {}", path.display(), e))?;

        config.validate()?;
        Ok(config)
    }

    pub fn default_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("~/.config"))
            .join("daemon-choir")
            .join("config.toml")
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        if self.ebpf.snapshot_rate_hz == 0 || self.ebpf.snapshot_rate_hz > 1000 {
            anyhow::bail!("ebpf.snapshot_rate_hz must be 1-1000");
        }
        if self.voices.pool_size > 128 {
            anyhow::bail!("voices.pool_size must be ≤ 128");
        }
        if self.mapping.net_bpm_min >= self.mapping.net_bpm_max {
            anyhow::bail!("net_bpm_min must be < net_bpm_max");
        }
        Ok(())
    }
}

9. Audio Mapping Deep Dive
9.1 Complete Mapping Table
Kernel Metric	Smoothing	Musical Parameter	Range	Perception
Per-process CPU %	250ms lag	Voice amplitude	0.0 – 0.7	"Louder = busier"
CPU burstiness (CoV)	500ms lag	Roughness (ring mod depth)	0.0 – 1.0	"Jagged = periodic/looping"
CPU sustained high	1s lag	Pitch drift upward	0–3 semitones	"Rising = building tension"
Process fork	Instant	Voice attack transient	n/a	"Birth note"
Process exit	Instant	Voice release envelope	1–2s decay	"Fading death"
Global CPU	1s lag	Scene scale multiplier	0.5 – 1.0	"Busier = louder overall"
Net TX packets/s	250ms lag	Rhythm tempo (BPM)	40 – 180	"Faster net = faster beat"
Net burst coefficient	500ms lag	Beat density/fills	0.0 – 0.9	"Bursty = syncopated rhythm"
Unique remote IPs	2s window	Rhythm chaos	0.0 – 0.8	"Many sources = chaotic rhythm"
Net packets/s > 50k	Instant	Thunder burst	0.8 – 1.0	"DDoS = literal thunder"
Free memory %	1s lag	Reverb room size	0.2 – 0.9	"Less memory = bigger/wetter room"
Major fault rate	500ms lag	Reverb wet mix	0.1 – 0.8	"Thrashing = drenched in reverb"
Swap in rate	1s lag	Master LPF cutoff	1500 – 8000 Hz	"Swapping = darker, murkier"
Memory pressure >90%	Instant	Scene → Critical	n/a	"Emergency = full dark room"
9.2 Preset Definitions
Preset	BPM Range	Max Reverb	Master Vol	Character
Eno Ambient	40–120	0.9	0.6	Slow, spacious, meditative
Industrial	80–180	0.6	0.8	Driving, mechanical, high-energy
Minimal	40–90	0.4	0.4	Subtle, sparse, background
Silent	n/a	n/a	0.0	No audio output
9.3 Scene Audio Behavior
Scene	Trigger Conditions	Audio Character	Visual Character
Calm	CPU < 10%, low net, normal mem	Slow drifting sine tones, sparse rhythm, wide reverb	Dim glowing orbs, slow drift
Busy	CPU 10–50%, moderate net	Richer harmonic texture, medium tempo, alive	Brighter orbs, active movement
Intense	CPU 50–80%	Dense voices, fast rhythm, dissonant drifts	Bright, fast-moving constellation
Storm	Net PPS > 50k, many IPs	Thunder bursts, rain noise layer, driving rhythm	Particle storm, lightning flashes
Critical	CPU > 90% or mem > 90%	Dissonant cluster, maximum reverb, dark low-pass filter	Red fog, pulsing warning
10. eBPF Program Deep Dive
10.1 Privilege Requirements
Capability	Required For	Linux Version
CAP_BPF	Loading eBPF programs and maps	5.8+
CAP_PERFMON	Attaching to perf-related tracepoints	5.8+
CAP_NET_ADMIN	Attaching to network hooks	All
CAP_SYS_ADMIN	Fallback if CAP_BPF is not available	All (older method)
10.2 Setting Capabilities Without Root

Bash

# scripts/install-capabilities.sh
# Grant required capabilities to the conductor binary so it doesn't need sudo

BINARY=$(which daemon-choir || echo "/usr/local/bin/daemon-choir")
sudo setcap cap_bpf,cap_perfmon,cap_net_admin+eip "$BINARY"

echo "Capabilities set. daemon-choir can now run without sudo."
echo "Verify with: getcap $BINARY"

10.3 Kernel Feature Detection

Rust

// conductor/src/transport/kernel_check.rs

use std::process::Command;

pub struct KernelFeatures {
    pub version_major: u32,
    pub version_minor: u32,
    pub ringbuf_supported: bool,
    pub bpf_supported: bool,
    pub unprivileged_bpf_disabled: bool,
}

impl KernelFeatures {
    pub fn detect() -> Self {
        let version = Self::read_kernel_version();
        let (major, minor) = parse_version(&version);

        // Ring buffer requires 5.8+
        let ringbuf_supported = major > 5 || (major == 5 && minor >= 8);

        // Check if unprivileged BPF is disabled (common on hardened distros)
        let unprivileged_disabled = std::fs::read_to_string(
            "/proc/sys/kernel/unprivileged_bpf_disabled"
        )
        .ok()
        .and_then(|s| s.trim().parse::<u8>().ok())
        .map(|v| v > 0)
        .unwrap_or(true); // Assume disabled if we can't read

        KernelFeatures {
            version_major: major,
            version_minor: minor,
            ringbuf_supported,
            bpf_supported: major >= 4,
            unprivileged_bpf_disabled: unprivileged_disabled,
        }
    }
}

10.4 eBPF Loader (Aya)

Rust

// conductor/src/transport/loader.rs

use aya::{Ebpf, EbpfLoader, programs::{TracePoint, KProbe}};

pub struct EbpfLoader {
    pub bpf: Ebpf,
}

impl EbpfLoader {
    pub fn load() -> anyhow::Result<Self> {
        // eBPF bytecode is embedded at compile time via include_bytes_aligned!
        let mut bpf = aya::EbpfLoader::new()
            .load(include_bytes_aligned!(
                "../../target/bpfel-unknown-none/release/daemon-choir-ebpf"
            ))?;

        // Attach process lifecycle tracepoints
        let prog: &mut TracePoint = bpf.program_mut("on_process_fork").unwrap().try_into()?;
        prog.load()?;
        prog.attach("sched", "sched_process_fork")?;

        let prog: &mut TracePoint = bpf.program_mut("on_process_exec").unwrap().try_into()?;
        prog.load()?;
        prog.attach("sched", "sched_process_exec")?;

        let prog: &mut TracePoint = bpf.program_mut("on_process_exit").unwrap().try_into()?;
        prog.load()?;
        prog.attach("sched", "sched_process_exit")?;

        // Attach CPU accounting
        let prog: &mut TracePoint = bpf.program_mut("on_sched_runtime").unwrap().try_into()?;
        prog.load()?;
        prog.attach("sched", "sched_stat_runtime")?;

        // Attach memory fault tracepoint
        let prog: &mut TracePoint = bpf.program_mut("on_major_fault").unwrap().try_into()?;
        prog.load()?;
        prog.attach("exceptions", "page_fault_kernel")?;

        // Attach network kprobe
        let prog: &mut KProbe = bpf.program_mut("on_tx").unwrap().try_into()?;
        prog.load()?;
        prog.attach("dev_queue_xmit", 0)?;

        log::info!("All eBPF programs loaded and attached");
        Ok(Self { bpf })
    }
}

11. Visual Renderer Deep Dive
11.1 TUI Layout Reference

text

┌────────────────────────────────────────────────────────────────────────┐
│  🎵 DAEMON CHOIR  |  Scene: CALM   |  Backend: SuperCollider  | 2m 14s │
├─────────────────────────────────┬──────────────────────────────────────┤
│  ACTIVE VOICES                  │  SYSTEM METRICS                      │
│  PID     PROCESS    CPU    FREQ │                                      │
│  ─────────────────────────────  │  CPU   [████████░░░░░░░░] 52%        │
│  1482    postgres   ██░░  110Hz │  MEM   [████░░░░░░░░░░░░] 28%        │
│  3291    node       ████ 165Hz  │  SWAP  [░░░░░░░░░░░░░░░░]  0%        │
│  5512    python3    ██░░  220Hz │                                      │
│  9021    cargo      ████ 330Hz  │  REVERB  room:0.28  mix:0.19         │
│  11042   sshd       ░░░░  82Hz  │  LPF     7200 Hz                     │
│  15331   nginx      ░░░░  55Hz  │                                      │
│  ...     (24 more silent)       │  NET RHYTHM                          │
│                                 │  BPM: 73   density: 0.21  chaos: 0.1 │
├─────────────────────────────────┴──────────────────────────────────────┤
│  NETWORK                                                                │
│  TX  [███░░░░░░░░░░░░░░░]  1.2k pps   RX [█████░░░░░░░░░░]  3.4k pps  │
├─────────────────────────────────────────────────────────────────────────┤
│  eBPF: active  |  Events: 482k  |  Dropped: 0  |  Voices: 6/32 active  │
└─────────────────────────────────────────────────────────────────────────┘

11.2 Bevy 3D Scene Structure
ECS System	Runs When	Does
spawn_process_node	On voice allocation	Creates sphere entity with identity-derived color/position
despawn_process_node	On voice deallocation	Plays decay animation, then removes entity
update_process_nodes	Every frame	Updates scale, emissive intensity from current CPU/amp
update_fog	Every frame	Sets fog density from memory pressure
update_particle_stream	Every frame	Updates network particle flow rate
flash_thunder	On Storm scene entry	Triggers brief white-flash bloom event
orbit_camera	Every frame	Slow auto-orbit for ambient effect; right-drag for user control
12. Privacy & Safety Model
12.1 What Daemon Choir Never Collects
Data Type	Status	Reason
Syscall arguments (filenames, strings)	❌ Never	Only counters and timings — no payloads
Network packet contents	❌ Never	Only packet/byte counts per interface
Process command-line arguments	❌ Never	Only comm (16-byte name) via bpf_get_current_comm
User credentials or environment variables	❌ Never	Not accessible via selected tracepoints
File paths	❌ Never	Not captured in any eBPF program
Process memory contents	❌ Never	eBPF programs do not read process memory
12.2 What Daemon Choir Does Collect (Locally, Optionally)
Data	Stored In	Retention	User Control
Metric snapshots (CPU, mem, net)	SQLite	7 days (configurable)	log_metrics = false disables
Scene transitions	SQLite	7 days	Configurable
Voice registry (comm names, hashes)	SQLite	Indefinite	Deletable via API
Anomaly log	SQLite	7 days	Configurable
Session logs	SQLite	7 days	Configurable

All storage is strictly local. There is no telemetry endpoint, no crash reporter, no update checker calling any remote.
13. Security Model & Privilege Boundaries
13.1 Trust Zones

text

┌─────────────────────────────────────────────────┐
│  KERNEL SPACE (eBPF programs)                   │
│  - Verifier-validated, sandboxed                │
│  - No unbounded loops                           │
│  - No unsafe memory access                      │
│  - Read-only observation of kernel events       │
│  - Writes ONLY to BPF maps owned by Conductor   │
└──────────────────┬──────────────────────────────┘
                   │  ring buffer (one-way, kernel → user)
┌──────────────────▼──────────────────────────────┐
│  USERSPACE TRUSTED (Conductor process)          │
│  - CAP_BPF + CAP_PERFMON (not root)             │
│  - Reads ring buffer                            │
│  - Sends OSC to audio backend (UDP localhost)   │
│  - Serves control API (localhost only)          │
│  - Writes to local SQLite                       │
└──────────────────┬──────────────────────────────┘
                   │  OSC UDP (localhost only)
┌──────────────────▼──────────────────────────────┐
│  AUDIO BACKEND (SuperCollider / PD)             │
│  - Receives OSC parameter updates               │
│  - Generates audio output to sound card        │
│  - No kernel access                             │
│  - No network access required                   │
└─────────────────────────────────────────────────┘

13.2 Control API Localhost Enforcement

Rust

// conductor/src/api/server.rs

use axum::middleware;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

pub async fn run_api_server(state: Arc<AppState>, port: u16) -> anyhow::Result<()> {
    // Bind ONLY to loopback — never 0.0.0.0
    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);

    let app = build_router(state)
        .layer(middleware::from_fn(localhost_only_middleware));

    axum::serve(
        tokio::net::TcpListener::bind(addr).await?,
        app
    ).await?;
    Ok(())
}

async fn localhost_only_middleware(
    req: axum::http::Request<axum::body::Body>,
    next: axum::middleware::Next,
) -> axum::response::Response {
    // Paranoia check: even though we bind loopback, verify remote addr
    if let Some(addr) = req.extensions().get::<axum::extract::ConnectInfo<SocketAddr>>() {
        if !addr.ip().is_loopback() {
            return axum::http::StatusCode::FORBIDDEN.into_response();
        }
    }
    next.run(req).await
}

13.3 eBPF Safety Guarantees
Guarantee	Mechanism
No infinite loops	Linux BPF verifier rejects programs with unbounded loops
No OOB memory access	BPF verifier validates all pointer arithmetic
No side effects on observed processes	Tracepoints are read-only; programs cannot modify kernel data structures via safe API
Memory budget	BPF maps have fixed, declared sizes; ring buffer is bounded
CPU budget	BPF programs have instruction count limits enforced by verifier
13.4 Threat Model Summary
Threat	Mitigation
eBPF program crashes kernel	BPF verifier rejects unsafe programs before load; Aya won't load unverified code
Conductor leaks process data	Only comm (name) and counters collected; no args, no paths, no strings
Control API exposed externally	Bound to 127.0.0.1 only; additional localhost middleware check
Audio backend receives malicious OSC	OSC only sent from localhost Conductor; audio backend has no kernel access
SQLite DB contains sensitive data	Contains only counters and process names; no payloads; user-deletable
Conductor uses too many resources	CPU budget enforced via per-process monitoring; self-monitoring with auto-throttle
Binary run as root unnecessarily	setcap grants minimum capabilities; root not required
14. Storage & Persistence
14.1 Database Connection

Rust

// conductor/src/storage/db.rs

use rusqlite::{Connection, params};
use std::path::Path;

pub struct Database {
    conn: Mutex<Connection>,
}

impl Database {
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        std::fs::create_dir_all(path.parent().unwrap())?;
        let conn = Connection::open(path)?;

        // Enable WAL for concurrent reads alongside writes
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        conn.execute_batch("PRAGMA busy_timeout=5000;")?;
        conn.execute_batch("PRAGMA synchronous=NORMAL;")?;
        conn.execute_batch("PRAGMA cache_size=-16000;")?; // 16MB cache

        let db = Database { conn: Mutex::new(conn) };
        db.run_migrations()?;
        Ok(db)
    }
}

14.2 Metric Repository

Rust

// conductor/src/storage/metric_repo.rs

impl Database {
    pub fn insert_metric_snapshot(&self, snap: &MetricSnapshot) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO metric_snapshots (timestamp, global_cpu, mem_pressure,
             net_tx_pps, net_rx_pps, net_bpm, active_voices, scene)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                snap.timestamp, snap.global_cpu, snap.mem_pressure,
                snap.net_tx_pps, snap.net_rx_pps, snap.net_bpm,
                snap.active_voices, format!("{:?}", snap.scene)
            ],
        )?;
        Ok(())
    }

    pub fn prune_metrics_older_than(&self, days: u32) -> anyhow::Result<u64> {
        let conn = self.conn.lock().unwrap();
        let deleted = conn.execute(
            "DELETE FROM metric_snapshots WHERE timestamp < datetime('now', ?1)",
            params![format!("-{} days", days)],
        )?;
        Ok(deleted as u64)
    }
}

15. Logging, Observability & Debugging
15.1 Structured Logging

Rust

// conductor/src/main.rs

use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, fmt, EnvFilter};
use tracing_appender::rolling::{RollingFileAppender, Rotation};

pub fn init_logging(cfg: &LogConfig) {
    let file_appender = RollingFileAppender::new(
        Rotation::DAILY,
        cfg.log_dir.clone(),
        "daemon_choir.log",
    );
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    tracing_subscriber::registry()
        .with(EnvFilter::new(&cfg.level))
        .with(fmt::layer().with_writer(non_blocking).json())
        .with(fmt::layer().with_writer(std::io::stdout).pretty())
        .init();
}

15.2 Key Observability Metrics
Metric	How Exposed	Meaning
ring_buffer_drops	Control API /state	Events dropped because ring buffer was full — indicates too-high snapshot rate
voice_evictions_per_sec	Control API /voices	How fast the voice pool is churning — high = system very dynamic
osc_dispatch_latency_ms	Debug log	Time from metric ingestion to OSC send
ebpf_attach_errors	Startup log	Number of programs that failed to attach
scene_transition_count	SQLite + API	Total scene changes since session start
active_voice_count	TUI + API	Currently active synth voices
proc_fallback_active	TUI + API	Whether /proc fallback is in use
audio_backend_name	TUI + API	Which audio backend is running
15.3 Debug Mode

When log_level = "debug":

    Per-OSC-message dispatch logged with full parameter values
    Per-voice parameter updates logged every dispatch cycle
    Ring buffer drain statistics logged per-cycle
    Scene evaluation decision logged with metric values
    Voice allocation/eviction events logged with exe_hash and reasoning

16. Testing Strategy
16.1 Test Pyramid

text

                    ┌────────────────┐
                    │   E2E (5%)     │
                    │  Full stack    │
                    │  + SC audio   │
                    └───────┬────────┘
             ┌──────────────┴───────────────┐
             │    Integration Tests (25%)   │
             │  Conductor + mock eBPF feed  │
             │  + mock OSC receiver         │
             └──────────────┬───────────────┘
        ┌────────────────────┴──────────────────────┐
        │              Unit Tests (70%)             │
        │  Aggregator, Mapping, Voice, Scene, Config │
        └────────────────────────────────────────────┘

16.2 Unit Tests

Rust

// conductor/src/mapping/engine_test.rs

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_to_reverb_mapping() {
        let engine = MappingEngine::with_preset(MappingPreset::eno_ambient());
        let state = ConductorState::with_mem_pressure(0.0);
        let bundle = engine.compute_dispatch(&state);
        assert!(bundle.reverb.room < 0.25, "Idle memory should produce small room");
        assert!(bundle.reverb.lpf > 7000.0, "Idle memory should produce bright sound");

        let state = ConductorState::with_mem_pressure(1.0);
        let bundle = engine.compute_dispatch(&state);
        assert!(bundle.reverb.room > 0.85, "Max memory pressure should produce large room");
        assert!(bundle.reverb.lpf < 2000.0, "Max memory pressure should produce dark sound");
    }

    #[test]
    fn test_network_to_bpm_mapping() {
        let engine = MappingEngine::with_preset(MappingPreset::eno_ambient());

        // Zero network activity → minimum BPM
        let state = ConductorState::with_net_pps(0.0);
        let bundle = engine.compute_dispatch(&state);
        assert!((bundle.net_rhythm.bpm - 40.0).abs() < 1.0);

        // Saturated network → maximum BPM
        let state = ConductorState::with_net_pps(10_000.0);
        let bundle = engine.compute_dispatch(&state);
        assert!((bundle.net_rhythm.bpm - 120.0).abs() < 5.0);
    }

    #[test]
    fn test_voice_identity_stability() {
        let identity_a = VoiceIdentity::from_exe_hash(0xdeadbeef_u64);
        let identity_b = VoiceIdentity::from_exe_hash(0xdeadbeef_u64);
        // Same hash must always produce same identity
        assert_eq!(identity_a.waveform, identity_b.waveform);
        assert!((identity_a.base_freq - identity_b.base_freq).abs() < 0.001);
        assert!((identity_a.pan - identity_b.pan).abs() < 0.001);
    }

    #[test]
    fn test_voice_identities_are_distinct() {
        // Different processes should get different identities
        let a = VoiceIdentity::from_exe_hash(0x1111_u64);
        let b = VoiceIdentity::from_exe_hash(0x2222_u64);
        // At least one parameter should differ
        let same = a.waveform == b.waveform
            && (a.base_freq - b.base_freq).abs() < 0.001
            && (a.pan - b.pan).abs() < 0.001;
        assert!(!same, "Different exe_hashes should produce different identities");
    }

    #[test]
    fn test_scene_hysteresis_prevents_flapping() {
        let mut engine = SceneEngine::new(5); // 5 tick hysteresis
        let busy_metrics = GlobalMetrics { global_cpu: 0.6, ..Default::default() };

        // 4 ticks of busy — should NOT transition yet
        for _ in 0..4 {
            assert_eq!(engine.evaluate(&busy_metrics), None);
        }
        // 5th tick — should transition
        assert_eq!(engine.evaluate(&busy_metrics), Some(Scene::Intense));
    }
}

16.3 Integration Tests (Mock eBPF Feed)

Rust

// tests/integration/conductor_test.rs

#[tokio::test]
async fn test_storm_scene_triggers_thunder_osc() {
    // Create a mock OSC receiver
    let osc_receiver = MockOscReceiver::bind("127.0.0.1:57999").await;

    let cfg = Config::test_config_with_osc_port(57999);
    let conductor = Conductor::new(cfg).await.unwrap();

    // Inject a synthetic high-PPS net snapshot (simulating DDoS)
    conductor.inject_test_snapshot(NetSnapshot {
        tx_packets_delta: 60_000,
        rx_packets_delta: 60_000,
        unique_remote_ips: 600,
        ..Default::default()
    }).await;

    // Wait for dispatch cycle
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Verify thunder OSC was sent
    let messages = osc_receiver.received_messages();
    let thunder_msg = messages.iter().find(|m| m.addr == "/scene/thunder");
    assert!(thunder_msg.is_some(), "Storm entry should trigger thunder OSC message");

    let scene_msg = messages.iter().find(|m| m.addr == "/scene/change");
    assert!(scene_msg.is_some());
    assert_eq!(scene_msg.unwrap().args[0], OscType::String("storm".to_string()));
}

#[tokio::test]
async fn test_process_exit_silences_voice() {
    let osc_receiver = MockOscReceiver::bind("127.0.0.1:57998").await;
    let cfg = Config::test_config_with_osc_port(57998);
    let conductor = Conductor::new(cfg).await.unwrap();

    // Assign voice to PID 12345
    conductor.inject_process_event(ProcessEvent {
        kind: ProcessEventKind::Fork,
        pid: 12345,
        ..Default::default()
    }).await;

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Kill the process
    conductor.inject_process_event(ProcessEvent {
        kind: ProcessEventKind::Exit,
        pid: 12345,
        ..Default::default()
    }).await;

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Find the silence message for this voice
    let messages = osc_receiver.received_messages();
    let silence_msg = messages.iter().find(|m| m.addr == "/voice/silence");
    assert!(silence_msg.is_some(), "Process exit should silence the allocated voice");
}

16.4 Mapping Learnability Tests

Rust

// tests/mapping_learnability_test.rs

/// Ensure that mapping relationships are monotonic
/// (more CPU always means louder, more memory pressure always means more reverb)
#[test]
fn test_mappings_are_monotonic() {
    let engine = MappingEngine::with_preset(MappingPreset::eno_ambient());

    let cpus = [0.0, 0.1, 0.3, 0.5, 0.7, 0.9, 1.0_f32];
    let mut last_amp = -1.0_f32;
    for cpu in cpus {
        let state = ConductorState::with_single_process_cpu(cpu);
        let bundle = engine.compute_dispatch(&state);
        let amp = bundle.voices[0].amp;
        assert!(amp >= last_amp, "Higher CPU must produce >= amplitude (monotonic)");
        last_amp = amp;
    }

    let pressures = [0.0, 0.2, 0.4, 0.6, 0.8, 1.0_f32];
    let mut last_room = -1.0_f32;
    for pressure in pressures {
        let state = ConductorState::with_mem_pressure(pressure);
        let bundle = engine.compute_dispatch(&state);
        assert!(bundle.reverb.room >= last_room, "Higher mem pressure must produce >= room size");
        last_room = bundle.reverb.room;
    }
}

17. Build, Packaging & Installation
17.1 Makefile

Makefile

.PHONY: all build build-ebpf build-conductor test lint clean install dev check-kernel

KERNEL_VERSION := $(shell uname -r | cut -d. -f1,2)
TARGET_EBPF    := bpfel-unknown-none
TARGET_HOST    := $(shell rustup show | grep "Default host" | cut -d: -f2 | tr -d ' ')

# Build eBPF programs (requires bpf target)
build-ebpf:
	cargo build --package daemon-choir-ebpf \
	  --target $(TARGET_EBPF) \
	  --release

# Build userspace Conductor
build-conductor:
	cargo build --package conductor --release

# Full build (eBPF first, then conductor embeds it)
build: build-ebpf build-conductor
	@echo "Build complete: target/release/daemon-choir"

# Run all tests
test:
	cargo test --workspace -- --test-threads=4

# Integration tests (require Linux + eBPF capable kernel)
test-integration:
	cargo test --test integration -- --nocapture

# Lint
lint:
	cargo clippy --workspace -- -D warnings
	cargo fmt --all -- --check

# Check kernel compatibility
check-kernel:
	@scripts/check-kernel.sh

# Install binary + capabilities
install: build
	sudo install -m 755 target/release/daemon-choir /usr/local/bin/daemon-choir
	sudo setcap cap_bpf,cap_perfmon,cap_net_admin+eip /usr/local/bin/daemon-choir
	@echo "Installed. Run: daemon-choir"

# Dev mode with auto-reload
dev:
	cargo watch -x "run --package conductor -- --config config/default.toml"

# Generate default config
init:
	daemon-choir --init
	@echo "Config written to ~/.config/daemon-choir/config.toml"

# Cross-platform release builds
release:
	# Linux amd64 (primary — eBPF works)
	cargo build --package conductor --release --target x86_64-unknown-linux-gnu
	# Linux arm64 (Raspberry Pi, ARM servers)
	cargo build --package conductor --release --target aarch64-unknown-linux-gnu
	# macOS arm64 (proc fallback only)
	cargo build --package conductor --release --target aarch64-apple-darwin
	# macOS x86_64 (proc fallback only)
	cargo build --package conductor --release --target x86_64-apple-darwin

clean:
	cargo clean

17.2 Installation Script

Bash

#!/bin/bash
# scripts/install.sh

set -e

echo "🎵 Installing Daemon Choir..."

# Check kernel version
KERNEL_MAJOR=$(uname -r | cut -d. -f1)
KERNEL_MINOR=$(uname -r | cut -d. -f2)
if [ "$KERNEL_MAJOR" -lt 5 ] || ([ "$KERNEL_MAJOR" -eq 5 ] && [ "$KERNEL_MINOR" -lt 8 ]); then
    echo "⚠️  Kernel $(uname -r) detected. eBPF ring buffer requires 5.8+."
    echo "   Daemon Choir will use /proc fallback mode (reduced resolution)."
fi

# Detect platform
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)
[ "$ARCH" = "x86_64"  ] && ARCH="amd64"
[ "$ARCH" = "aarch64" ] && ARCH="arm64"

# Download binary
echo "📥 Downloading daemon-choir-${OS}-${ARCH}..."
curl -sSL \
  "https://github.com/your-org/daemon-choir/releases/latest/download/daemon-choir-${OS}-${ARCH}" \
  -o /tmp/daemon-choir
chmod +x /tmp/daemon-choir
sudo mv /tmp/daemon-choir /usr/local/bin/daemon-choir

# Set capabilities (Linux only)
if [ "$OS" = "linux" ]; then
    echo "🔐 Setting eBPF capabilities (no sudo required at runtime)..."
    sudo setcap cap_bpf,cap_perfmon,cap_net_admin+eip /usr/local/bin/daemon-choir
fi

# Initialize config
echo "⚙️  Initializing configuration..."
daemon-choir --init

# Check for SuperCollider
if command -v sclang > /dev/null 2>&1; then
    echo "✅ SuperCollider found — will use as primary audio backend."
else
    echo "ℹ️  SuperCollider not found. Using built-in Rust synth."
    echo "   For richer sound: https://supercollider.github.io/"
fi

echo ""
echo "✅ Daemon Choir installed!"
echo ""
echo "   Start:        daemon-choir"
echo "   With TUI:     daemon-choir --visual tui"
echo "   With 3D:      daemon-choir --visual bevy"
echo "   Config:       ~/.config/daemon-choir/config.toml"
echo "   Control API:  http://localhost:9876/state"

17.3 SuperCollider Setup

Bash

# Install SuperCollider (required for full audio quality)
# macOS
brew install supercollider

# Ubuntu/Debian
sudo apt install supercollider

# Copy synthesis graph to config dir
daemon-choir --install-sc-patches
# Writes SC patches to ~/.config/daemon-choir/sc/

# Test SuperCollider connectivity
daemon-choir --test-audio

18. Platform Support Matrix
Feature	Linux (amd64)	Linux (arm64)	macOS (Apple Silicon)	macOS (Intel)	Windows (amd64)
eBPF Instrumentation	✅ (kernel ≥5.8)	✅ (kernel ≥5.8)	❌	❌	❌
/proc Fallback	✅	✅	✅ (sysctl)	✅ (sysctl)	✅ (WMI)
SuperCollider Backend	✅	✅	✅	✅	✅
PureData Backend	✅	✅	✅	✅	✅
Built-in Rust Synth	✅	✅	✅	✅	✅
Ratatui TUI	✅	✅	✅	✅	✅
Bevy 3D Visualizer	✅	✅	✅	✅	✅
No-sudo operation	✅ (setcap)	✅ (setcap)	✅	✅	⚠️ (admin first run)
Single binary	✅	✅	✅	✅	✅
Systemd service	✅	✅	❌	❌	❌
launchd service	❌	❌	✅	✅	❌
19. Performance Targets & Benchmarks
Metric	Target	Notes
Conductor CPU overhead	< 0.5% P99	Measured on 8-core machine with 200 active processes
Conductor memory	< 50MB RSS	Includes SQLite page cache
Ring buffer drain latency	< 5ms P99	Time from kernel write to userspace read
Aggregation cycle time	< 1ms	Per dispatch tick at 20 Hz
OSC dispatch latency	< 2ms	From metric ingestion to UDP send
Voice allocation update	< 0.5ms	Per 20 Hz tick, 32 voices
SQLite insert (metric)	< 3ms	WAL mode, 20 Hz = 20 writes/sec
TUI render cycle	< 16ms	60 FPS target for smooth bars
Bevy 3D frame time	< 33ms	30 FPS target for spatial renderer
eBPF program overhead	< 0.1% CPU	Measured at 100k sched events/sec
Startup time	< 3s	Including eBPF load, attach, SC health check
Blocklist load	n/a	No blocklists — Daemon Choir is an observer
19.1 Self-Monitoring (Conductor Observes Itself)

The Conductor monitors its own resource usage. If it detects it is consuming > 2% CPU, it automatically reduces the snapshot rate:

Rust

// conductor/src/resource_guard.rs

pub struct ResourceGuard {
    self_pid: u32,
    max_cpu_fraction: f32,
    current_rate: u32,
}

impl ResourceGuard {
    pub fn check_and_throttle(&mut self, aggregator: &Aggregator) -> Option<u32> {
        if let Some(self_cpu) = aggregator.processes.get(&self.self_pid) {
            if self_cpu.cpu_avg() > self.max_cpu_fraction as f64 {
                let new_rate = (self.current_rate / 2).max(5); // Floor: 5 Hz
                if new_rate != self.current_rate {
                    log::warn!(
                        "Conductor CPU {}% exceeds budget, throttling snapshot rate {} → {} Hz",
                        self_cpu.cpu_avg() * 100.0, self.current_rate, new_rate
                    );
                    self.current_rate = new_rate;
                    return Some(new_rate);
                }
            }
        }
        None
    }
}

20. Error Handling Strategy
20.1 Error Categories

Rust

// conductor/src/errors.rs

#[derive(Debug, thiserror::Error)]
pub enum DaemonChoirError {
    #[error("eBPF load failed: {0}")]
    EbpfLoad(#[from] aya::EbpfError),

    #[error("eBPF program attach failed for {program}: {source}")]
    EbpfAttach { program: String, source: aya::programs::ProgramError },

    #[error("Kernel version {major}.{minor} < 5.8; eBPF ring buffer not available")]
    KernelTooOld { major: u32, minor: u32 },

    #[error("No audio backend available: tried {tried:?}")]
    NoAudioBackend { tried: Vec<String> },

    #[error("SuperCollider not responding at {addr}")]
    AudioBackendUnresponsive { addr: String },

    #[error("OSC send failed: {0}")]
    OscDispatch(String),

    #[error("Ring buffer overrun: {dropped} events dropped")]
    RingBufferOverrun { dropped: u64 },

    #[error("Config error: {0}")]
    Config(String),

    #[error("Storage error: {0}")]
    Storage(#[from] rusqlite::Error),
}

20.2 Error Policy (Critical Rule: Never Drop Audio for Metric Errors)
Error Type	Behavior	Audio Impact
eBPF load fails	Log error, fall back to /proc polling	None — fallback provides data
Ring buffer overrun	Log drop count, reduce snapshot rate	None — stale metrics, not silence
OSC send fails	Retry 3×, then log and continue	Brief audio glitch at most
Audio backend crash	Attempt reconnect loop, notify TUI	Silence until reconnected
SQLite write fails	Log and discard write; never block dispatch	None
Config validation fails	Use defaults for invalid fields; log	None
Bevy/TUI render fails	Log, disable visual mode; audio continues	None

Rust

// conductor/src/main_loop.rs

loop {
    // Every dispatch tick
    tokio::select! {
        _ = dispatch_ticker.tick() => {
            // Aggregation failure must NEVER stop audio
            if let Err(e) = aggregator.tick() {
                log::warn!("Aggregation tick error (non-fatal): {}", e);
                // Use last-known-good state for this tick
            }

            let dispatch = mapping_engine.compute_dispatch(&state);

            // OSC failure must NEVER panic
            if let Err(e) = osc_dispatcher.send_bundle(&dispatch).await {
                log::warn!("OSC dispatch error (retrying): {}", e);
                metrics.osc_errors.fetch_add(1, Ordering::Relaxed);
            }
        }

        // Ring buffer events — failures are logged but loop continues
        Some(event) = process_rx.recv() => {
            if let Err(e) = aggregator.ingest_process_event(event) {
                log::debug!("Process event ingest error: {}", e);
            }
        }
    }
}

21. Dependency Registry
21.1 Rust Dependencies

toml

# Cargo.toml (workspace)

[workspace]
members = [
    "daemon-choir-common",
    "daemon-choir-ebpf",
    "conductor",
]

# conductor/Cargo.toml
[dependencies]

# eBPF
aya = { version = "0.12", features = ["async_tokio"] }
aya-bpf = "0.12"
aya-log = "0.2"

# Async runtime
tokio = { version = "1", features = ["full"] }

# OSC
rosc = "0.10"

# HTTP API
axum = { version = "0.7", features = ["macros"] }
tower = "0.4"
tower-http = "0.5"

# Config
serde = { version = "1", features = ["derive"] }
toml = "0.8"
dirs = "5"

# Logging
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }
tracing-appender = "0.2"

# Storage
rusqlite = { version = "0.31", features = ["bundled"] }

# Error handling
thiserror = "1"
anyhow = "1"

# TUI
ratatui = "0.26"
crossterm = "0.27"

# 3D (optional feature)
bevy = { version = "0.13", optional = true, features = ["dynamic_linking"] }

# Audio (built-in fallback synth)
cpal = "0.15"
dasp = "0.11"

# Hashing (voice identity)
fnv = "1"

# Utilities
uuid = { version = "1", features = ["v4"] }

[features]
default = ["tui"]
bevy_visual = ["bevy"]
full = ["tui", "bevy_visual"]

21.2 External Tools Required
Tool	Required For	Install
Rust (nightly)	eBPF program compilation (bpf target)	rustup toolchain install nightly
bpf-linker	Linking eBPF programs	cargo install bpf-linker
llvm	eBPF compilation target support	apt install llvm / brew install llvm
SuperCollider	Primary audio backend (optional)	brew install supercollider / apt
PureData	Secondary audio backend (optional)	apt install puredata / brew
Linux kernel ≥ 5.8	eBPF ring buffer	Kernel upgrade if needed
setcap	Capability assignment	apt install libcap2-bin
22. Milestone & Phased Rollout Plan
Phase 0 — Proof of Sound (Week 1–2)

Goal: Validate the aesthetic before any kernel work.

    Poll /proc/stat and /proc/net/dev at 5 Hz
    Map CPU, mem, and net to OSC parameters manually
    Implement the full SuperCollider synthesis graph (processVoice, netRhythm, daemonReverb, thunderBurst)
    Implement OSC dispatcher in Rust
    Implement the Eno Ambient and Industrial presets
    Verify the soundscape is musically coherent and listenable over 30+ minutes
    Validate the three core metaphors: CPU=voice, net=rhythm, mem=reverb

Deliverable: A working ambient sound monitor using /proc. Proves the concept sounds good before kernel work begins.
Phase 1 — eBPF Foundation (Weeks 3–6)

Goal: Replace /proc with real eBPF instrumentation.

    Set up Aya workspace with daemon-choir-common shared crate
    Implement process_monitor.rs eBPF program (fork/exec/exit)
    Implement cpu_accounting.rs with ring buffer snapshot
    Implement ring buffer consumer in Conductor
    Implement snapshot timer + flush mechanism
    Implement kernel feature detection + /proc fallback
    Implement setcap installer script
    Verify process voice allocation works with real kernel data
    Unit tests for aggregator, scene engine, voice allocator

Deliverable: Conductor running on live eBPF data. Richer, lower-latency voice behavior.
Phase 2 — Network & Memory (Weeks 7–9)

Goal: Complete the kernel instrumentation with net and memory.

    Implement net_traffic.rs eBPF program (per-CPU counters)
    Implement mm_pressure.rs eBPF program (major fault tracepoint)
    Implement NetMetricWindow and MemMetricWindow aggregation
    Complete the mapping engine with full net→rhythm and mem→reverb
    Implement Storm scene with thunder burst trigger
    Implement Critical scene with full dark-room reverb
    Verify DDoS simulation sounds like thunderstorm (inject synthetic high-PPS events)
    Verify memory pressure simulation sounds like "wet fog"

Deliverable: All three metaphors live from kernel data. All five scenes working.
Phase 3 — Visual & Control (Weeks 10–12)

Goal: Ratatui TUI, control API, and config system.

    Implement Ratatui TUI with voice table, metric gauges, network bars, and scene banner
    Implement control API (Axum, localhost:9876)
    Implement TOML config system with hot-reload
    Implement preset switching (Eno, Industrial, Minimal, Silent)
    Implement SQLite storage with metric retention
    Implement anomaly logging
    Integration tests with mock OSC receiver

Deliverable: Full system with TUI, configurable presets, control API, and audit logs.
Phase 4 — Bevy 3D & Polish (Weeks 13–16)

Goal: 3D visualizer and production hardening.

    Implement Bevy ECS scene (process constellation, fog, particles)
    Implement process node spawn/despawn with identity-derived color/position
    Implement memory pressure → volumetric fog
    Implement network particle stream
    Implement Storm → lightning flash bloom
    PureData backend implementation
    Built-in Rust soft-synth fallback (cpal + basic oscillators)
    Cross-platform release builds (Linux amd64/arm64, macOS)
    Performance benchmarking against budget targets
    Resource guard (self-throttling under load)

Deliverable: v1.0 release candidate with full feature set.
Phase 5 — Release & Documentation (Weeks 17–18)

    Full README with quickstart, SC setup, kernel requirements
    SuperCollider patch documentation
    Configuration reference
    Security model documentation
    Signed release binaries
    Installation script tested on Ubuntu 22.04, Ubuntu 24.04, Debian 12, Fedora 39, Arch, macOS 14+
    80% unit test coverage

Deliverable: v1.0 public release.
23. Open Questions & Future Work
23.1 Open Technical Questions
Question	Status	Notes
macOS kernel instrumentation (DTrace/sysctl)?	Open	DTrace could replace eBPF on macOS; not as low-overhead
cgroup-level voice grouping (Docker containers)?	Open	Could assign one voice per container rather than per-PID — better for microservice systems
eBPF CO-RE (Compile Once Run Everywhere)?	Backlog	BTF-based portability to avoid kernel-specific offsets
MIDI output for DAW integration?	Backlog	Emit MIDI CC from metric mappings; useful for live performance use
Multi-machine mode (SSH into remote server)?	Backlog	Forward eBPF snapshots over SSH tunnel; monitor remote servers locally
Windows ETW support?	Backlog	WFP/ETW could provide similar instrumentation on Windows
Headphone vs. speaker acoustic calibration?	Open	Spatial rendering differs significantly between stereo speaker setups and headphones
23.2 Potential Future Modules
Module	Description	Priority
Container Voice Grouping	Group per-PID voices into per-container instrument sections	High
GPU Metrics Layer	NVML/ROCm instrumentation → GPU load adds new instrument family	High
MIDI Output	Emit MIDI CC/notes from metrics for DAW/hardware synth control	Medium
Alert Mode	Configurable threshold → one-shot alert sound (non-ambient, attention-getting)	Medium
Session Recording	Record metric history + audio to file for playback/analysis	Medium
Remote Server Monitoring	Forward snapshots over SSH; monitor fleet of servers as multi-channel spatial audio	High
SuperCollider Preset Editor	In-browser SynthDef editor; live-patch the synthesis graph	Low
Distributed Choir Mode	Multiple machines' metrics merge into one spatial soundscape	Low
Discord/Slack Integration	Scene transitions → webhook notifications	Low
23.3 Known Limitations at v1.0

    eBPF requires Linux ≥ 5.8. Systems on older kernels (including many older LTS installations) fall back to /proc polling at 5 Hz, which is perceptually
