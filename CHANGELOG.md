# Changelog

All notable changes to the DAEMON CHOIR daemon are documented here.

## [2.0.0] - 2025-06-12 (Production Engineering Spec)

### Added
- Created complete Technical Specification, OpenAPI 3.0 descriptor, and JSON schema.
- Added Architectural Decision Records (ADRs) 1 through 6.
- Implemented unprivileged capabilities drop path post-loading using `libc::PR_SET_NO_NEW_PRIVS`.
- Implemented real-time OSC 1.1 bundling with NTP timetags to resolve packet jitter.
- Built standard-compliant Prometheus metrics endpoint with atomic histogram buckets.
- Added non-blocking decoupled SQLite thread channels to prevent pipeline stalls.
- Built a crash-proof terminal dashboard UI with safe result handling.
- Added FNV-1a UID hashing in kernel-space to eliminate plaintext exfiltration.
- Built real kernel scheduling queue latency measurement using wakeup BPF maps and exit cleanup.
- Created `Makefile` and `clang` compilation targets for real BPF ELF bytecode compilation.
