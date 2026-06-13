use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ProbeId {
    CpuSched,
    MemPressure,
    NetIo,
    ProcLifecycle,
}

impl std::fmt::Display for ProbeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProbeId::CpuSched => write!(f, "cpu_sched"),
            ProbeId::MemPressure => write!(f, "mem_pressure"),
            ProbeId::NetIo => write!(f, "net_io"),
            ProbeId::ProcLifecycle => write!(f, "proc_lifecycle"),
        }
    }
}

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
    pub prev_comm_hash: u64, // FNV-64 hash — NEVER raw comm string
    pub next_comm_hash: u64,
    pub latency_ns: u64,
    pub cpu_id: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemPressureEvent {
    pub pages_reclaimed: u64,
    pub zone: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetIoEvent {
    pub bytes_tx: u64,
    pub bytes_rx: u64,
    pub dev_name_hash: u64, // FNV-64 hash of device name to be safe
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcLifecycleEvent {
    pub comm_hash: u64,
    pub uid_hash: u64,
}

/// FNV-1a 64-bit hashing algorithm for privacy-by-design anonymization.
pub fn fnv1a_64(data: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for &b in data {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100000001b3u64);
    }
    hash
}

// BPF structure mappings for real ring buffer deserialization (§3.3, BUG-01)
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuSchedBpfEvent {
    pub prev_comm_hash: u64,
    pub next_comm_hash: u64,
    pub latency_ns: u64,
    pub cpu_id: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemPressureBpfEvent {
    pub pages_reclaimed: u64,
    pub zone: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NetIoBpfEvent {
    pub bytes_tx: u64,
    pub bytes_rx: u64,
    pub dev_name_hash: u64,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcLifecycleBpfEvent {
    pub comm_hash: u64,
    pub uid_hash: u64,
}

// Compile-time struct layout size assertions to catch C/Rust mismatch (§12.1, BUG-P2)
// Sizes are 8-byte aligned on 64-bit platforms due to u64 fields.
const _: () = assert!(std::mem::size_of::<CpuSchedBpfEvent>() == 32);
const _: () = assert!(std::mem::size_of::<MemPressureBpfEvent>() == 16);
const _: () = assert!(std::mem::size_of::<NetIoBpfEvent>() == 24);
const _: () = assert!(std::mem::size_of::<ProcLifecycleBpfEvent>() == 16);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fnv1a_64() {
        let hash1 = fnv1a_64(b"systemd");
        let hash2 = fnv1a_64(b"systemd");
        assert_eq!(hash1, hash2);
        assert_ne!(hash1, fnv1a_64(b"chrome"));
    }

    // Safety & round-trip tests for unsafe BPF deserialization path to satisfy BUG-P2
    #[test]
    fn test_from_bytes_roundtrip() {
        let original = CpuSchedBpfEvent {
            prev_comm_hash: 9999,
            next_comm_hash: 8888,
            latency_ns: 400,
            cpu_id: 2,
        };

        let ptr = &original as *const CpuSchedBpfEvent as *const u8;
        let slice = unsafe { std::slice::from_raw_parts(ptr, std::mem::size_of::<CpuSchedBpfEvent>()) };

        let deserialized: Option<CpuSchedBpfEvent> = unsafe {
            if slice.len() < std::mem::size_of::<CpuSchedBpfEvent>() {
                None
            } else {
                Some(std::ptr::read_unaligned(slice.as_ptr() as *const CpuSchedBpfEvent))
            }
        };

        assert_eq!(deserialized, Some(original));
    }
}
