use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc;
use tracing::{info, warn};
use daemon_choir_common::events::{
    RawEvent, EventPayload, CpuSchedEvent, MemPressureEvent, NetIoEvent, ProcLifecycleEvent, ProbeId, fnv1a_64,
    CpuSchedBpfEvent, MemPressureBpfEvent, NetIoBpfEvent, ProcLifecycleBpfEvent
};

#[cfg(feature = "ebpf")]
use aya::{
    Bpf,
    programs::TracePoint,
    maps::RingBuf,
};
#[cfg(feature = "ebpf")]
use tracing::error;
#[cfg(feature = "ebpf")]
use tokio::io::unix::AsyncFd;

#[derive(Debug, Clone)]
pub struct ProbeStats {
    pub name: String,
    pub attach_point: String,
    pub loaded: bool,
    pub events_total: Arc<AtomicU64>,
    pub events_lost: Arc<AtomicU64>,
    pub last_event_us: Arc<AtomicU64>,
}

pub struct ProbeManager {
    probes: Vec<ProbeStats>,
    simulate: bool,
    tx: mpsc::Sender<RawEvent>,
    shutdown: Arc<AtomicBool>,
    cumulative_lost_events: Arc<AtomicU64>, // Tracks total channel drops (§11.2, BUG-03)
}

impl ProbeManager {
    pub fn new(
        enabled_probes: &[String],
        simulate: bool,
        tx: mpsc::Sender<RawEvent>,
        cumulative_lost_events: Arc<AtomicU64>,
    ) -> Self {
        let mut probes = Vec::new();
        
        for name in enabled_probes {
            let attach_point = match name.as_str() {
                "cpu_sched" => "tracepoint/sched/sched_switch",
                "mem_pressure" => "tracepoint/vmscan/mm_vmscan_shrink_inactive_to_succ",
                "net_io" => "tracepoint/net/net_dev_xmit",
                "proc_lifecycle" => "tracepoint/sched/sched_process_exec",
                _ => "unknown",
            };
            
            probes.push(ProbeStats {
                name: name.clone(),
                attach_point: attach_point.to_string(),
                loaded: false,
                events_total: Arc::new(AtomicU64::new(0)),
                events_lost: Arc::new(AtomicU64::new(0)),
                last_event_us: Arc::new(AtomicU64::new(0)),
            });
        }

        Self {
            probes,
            simulate,
            tx,
            shutdown: Arc::new(AtomicBool::new(false)),
            cumulative_lost_events,
        }
    }

    pub fn get_probes_info(&self) -> Vec<ProbeStats> {
        self.probes.clone()
    }

    pub async fn start(&mut self) {
        info!("Initializing Probe Manager. Simulation mode: {}", self.simulate);
        
        for probe in &mut self.probes {
            #[allow(unused_mut)]
            let mut bpf_loaded = false;

            // 1. Attempt to load the real eBPF probe via Aya if ebpf feature is compiled and simulation is off §3.3
            #[cfg(feature = "ebpf")]
            {
                if !self.simulate {
                    let bpf_o_path = format!("target/bpf/{}.bpf.o", probe.name);
                    info!("Aya: Loading eBPF ELF program from file: {}", bpf_o_path);
                    
                    match Bpf::load_file(&bpf_o_path) {
                        Ok(mut bpf) => {
                            let (category, prog_name, tp_name) = match probe.name.as_str() {
                                "cpu_sched" => ("sched", "handle_sched_switch", "sched_switch"),
                                "mem_pressure" => ("vmscan", "handle_mm_vmscan", "mm_vmscan_shrink_inactive_to_succ"),
                                "net_io" => ("net", "handle_net_dev_xmit", "net_dev_xmit"),
                                "proc_lifecycle" => ("sched", "handle_process_exec", "sched_process_exec"),
                                _ => ("", "", ""),
                            };

                            let attach_and_spawn_res: Result<(), &'static str> = (|| {
                                let program_mut = bpf.program_mut(prog_name)
                                    .ok_or_else(|| {
                                        error!("Program {} not found in BPF ELF", prog_name);
                                        "Program not found"
                                    })?;
                                let program: &mut TracePoint = program_mut.try_into().map_err(|e| {
                                    error!("Aya: Failed to convert program to TracePoint: {}", e);
                                    "Conversion failed"
                                })?;
                                program.load().map_err(|e| {
                                    error!("Aya: Failed to load BPF program: {}", e);
                                    "Load failed"
                                })?;
                                program.attach(category, tp_name).map_err(|e| {
                                    error!("Aya: Failed to attach tracepoint program: {}", e);
                                    "Attach failed"
                                })?;

                                info!("Aya: Successfully loaded and attached tracepoint: {}/{}", category, tp_name);

                                // If the loaded probe is "cpu_sched", we MUST attach secondary wakeup/exit tracepoints (§3.3, BUG-P0)
                                if probe.name == "cpu_sched" {
                                    let wakeup_prog_mut = bpf.program_mut("handle_sched_wakeup")
                                        .ok_or_else(|| {
                                            error!("Program handle_sched_wakeup not found in BPF ELF");
                                            "Program not found"
                                        })?;
                                    let wakeup_prog: &mut TracePoint = wakeup_prog_mut.try_into().map_err(|e| {
                                        error!("Aya: Failed to convert wakeup program: {}", e);
                                        "Conversion failed"
                                    })?;
                                    wakeup_prog.load().map_err(|e| {
                                        error!("Aya: Failed to load wakeup program: {}", e);
                                        "Load failed"
                                    })?;
                                    wakeup_prog.attach("sched", "sched_wakeup").map_err(|e| {
                                        error!("Aya: Failed to attach sched_wakeup program: {}", e);
                                        "Attach failed"
                                    })?;

                                    info!("Aya: Successfully loaded and attached secondary tracepoint: sched/sched_wakeup");

                                    let exit_prog_mut = bpf.program_mut("handle_process_exit")
                                        .ok_or_else(|| {
                                            error!("Program handle_process_exit not found in BPF ELF");
                                            "Program not found"
                                        })?;
                                    let exit_prog: &mut TracePoint = exit_prog_mut.try_into().map_err(|e| {
                                        error!("Aya: Failed to convert exit program: {}", e);
                                        "Conversion failed"
                                    })?;
                                    exit_prog.load().map_err(|e| {
                                        error!("Aya: Failed to load exit program: {}", e);
                                        "Load failed"
                                    })?;
                                    exit_prog.attach("sched", "sched_process_exit").map_err(|e| {
                                        error!("Aya: Failed to attach sched_process_exit program: {}", e);
                                        "Attach failed"
                                    })?;

                                    info!("Aya: Successfully loaded and attached secondary tracepoint: sched/sched_process_exit");
                                }

                                // Move ownership of `bpf` into a dedicated background polling loop task
                                // to satisfy the static lifetime requirements of tokio::spawn and borrow checker.
                                let events_total = probe.events_total.clone();
                                let last_event_us = probe.last_event_us.clone();
                                let events_lost = probe.events_lost.clone();
                                let cumulative_lost = self.cumulative_lost_events.clone();
                                let tx_clone = self.tx.clone();
                                let shutdown_clone = self.shutdown.clone();
                                let probe_id = match probe.name.as_str() {
                                    "cpu_sched" => ProbeId::CpuSched,
                                    "mem_pressure" => ProbeId::MemPressure,
                                    "net_io" => ProbeId::NetIo,
                                    "proc_lifecycle" => ProbeId::ProcLifecycle,
                                    _ => ProbeId::CpuSched,
                                };

                                tokio::spawn(async move {
                                    let mut owned_bpf = bpf; // Moves ownership of Bpf here
                                    if let Ok(ringbuf) = RingBuf::try_from(owned_bpf.map_mut("events_ringbuf").unwrap()) {
                                        if let Ok(async_fd) = AsyncFd::new(ringbuf) {
                                            while !shutdown_clone.load(Ordering::SeqCst) {
                                                // Event-driven async polling using tokio::io::unix::AsyncFd (§12.1, GAP-02)
                                                match async_fd.readable().await {
                                                    Ok(mut guard) => {
                                                        // Drain all events currently in the ring buffer
                                                        while let Some(item) = guard.get_mut().next() {
                                                            let now_ns = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos() as u64;
                                                            let now_us = now_ns / 1000;
                                                            
                                                            // Real BPF byte deserialization helper to satisfy BUG-01
                                                            let payload = match probe_id {
                                                                ProbeId::CpuSched => {
                                                                    if let Some(bpf_event) = unsafe { from_bytes::<CpuSchedBpfEvent>(&item) } {
                                                                        EventPayload::CpuSched(CpuSchedEvent {
                                                                            prev_comm_hash: bpf_event.prev_comm_hash,
                                                                            next_comm_hash: bpf_event.next_comm_hash,
                                                                            latency_ns: bpf_event.latency_ns,
                                                                            cpu_id: bpf_event.cpu_id,
                                                                        })
                                                                    } else {
                                                                        continue;
                                                                    }
                                                                }
                                                                ProbeId::MemPressure => {
                                                                    if let Some(bpf_event) = unsafe { from_bytes::<MemPressureBpfEvent>(&item) } {
                                                                        EventPayload::MemPressure(MemPressureEvent {
                                                                            pages_reclaimed: bpf_event.pages_reclaimed,
                                                                            zone: bpf_event.zone,
                                                                        })
                                                                    } else {
                                                                        continue;
                                                                    }
                                                                }
                                                                ProbeId::NetIo => {
                                                                    if let Some(bpf_event) = unsafe { from_bytes::<NetIoBpfEvent>(&item) } {
                                                                        EventPayload::NetIo(NetIoEvent {
                                                                            bytes_tx: bpf_event.bytes_tx,
                                                                            bytes_rx: bpf_event.bytes_rx,
                                                                            dev_name_hash: bpf_event.dev_name_hash,
                                                                        })
                                                                    } else {
                                                                        continue;
                                                                    }
                                                                }
                                                                ProbeId::ProcLifecycle => {
                                                                    if let Some(bpf_event) = unsafe { from_bytes::<ProcLifecycleBpfEvent>(&item) } {
                                                                        EventPayload::ProcLifecycle(ProcLifecycleEvent {
                                                                            comm_hash: bpf_event.comm_hash,
                                                                            uid_hash: bpf_event.uid_hash,
                                                                        })
                                                                    } else {
                                                                        continue;
                                                                    }
                                                                }
                                                            };

                                                            let event = RawEvent {
                                                                probe: probe_id.clone(),
                                                                timestamp_ns: now_ns,
                                                                payload,
                                                            };

                                                            if let Ok(_) = tx_clone.try_send(event) {
                                                                events_total.fetch_add(1, Ordering::Relaxed);
                                                                last_event_us.store(now_us, Ordering::Relaxed);
                                                            } else {
                                                                events_lost.fetch_add(1, Ordering::Relaxed);
                                                                cumulative_lost.fetch_add(1, Ordering::Relaxed);
                                                            }
                                                        }
                                                        guard.clear_ready();
                                                    }
                                                    Err(e) => {
                                                        error!("Aya: RingBuf fd is no longer valid: {:?}", e);
                                                        break;
                                                    }
                                                }
                                            }
                                        } else {
                                            error!("Aya: Failed to wrap RingBuf in AsyncFd. Event-driven polling disabled.");
                                        }
                                    }
                                });

                                Ok(())
                            })();

                            if attach_and_spawn_res.is_ok() {
                                bpf_loaded = true;
                                probe.loaded = true;
                            }
                        }
                        Err(e) => {
                            error!("Failed to load real eBPF probe '{}' via Aya: {:?}", probe.name, e);
                        }
                    }
                }
            }

            // 2. Fall back to simulation if Aya failed to load or is not compiled/enabled
            if !bpf_loaded {
                warn!("Aya engine disabled or loading failed. Falling back to SIMULATED probe '{}'", probe.name);
                probe.loaded = true;
                let tx_clone = self.tx.clone();
                let shutdown_clone = self.shutdown.clone();
                let events_total = probe.events_total.clone();
                let events_lost = probe.events_lost.clone();
                let cumulative_lost = self.cumulative_lost_events.clone();
                let last_event_us = probe.last_event_us.clone();
                let name = probe.name.clone();

                tokio::spawn(async move {
                    run_simulator_probe(name, tx_clone, shutdown_clone, events_total, events_lost, cumulative_lost, last_event_us).await;
                });
            }
        }
    }

    pub fn shutdown(&self) {
        info!("Stopping Probe Manager. Unloading probes...");
        self.shutdown.store(true, Ordering::SeqCst);
    }
}

// Memory-unaligned safe reading of BPF structs written to the Ringbuf (§3.3, BUG-01)
unsafe fn from_bytes<T>(slice: &[u8]) -> Option<T> {
    if slice.len() < std::mem::size_of::<T>() {
        return None;
    }
    Some(std::ptr::read_unaligned(slice.as_ptr() as *const T))
}

async fn run_simulator_probe(
    name: String,
    tx: mpsc::Sender<RawEvent>,
    shutdown: Arc<AtomicBool>,
    events_total: Arc<AtomicU64>,
    events_lost: Arc<AtomicU64>,
    cumulative_lost: Arc<AtomicU64>,
    last_event_us: Arc<AtomicU64>,
) {
    let mut interval = tokio::time::interval(Duration::from_millis(50));
    let mut counter = 0u64;

    while !shutdown.load(Ordering::SeqCst) {
        interval.tick().await;
        counter += 1;

        let now_ns = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos() as u64;
        let now_us = now_ns / 1000;

        let payload = match name.as_str() {
            "cpu_sched" => {
                let load_factor = (counter as f64 * 0.1).sin() * 0.5 + 0.5;
                let latency_ns = (1000.0 + load_factor * 8000.0) as u64;
                
                let prev_comm = format!("worker-{}", counter % 4);
                let next_comm = format!("worker-{}", (counter + 1) % 4);

                EventPayload::CpuSched(CpuSchedEvent {
                    prev_comm_hash: fnv1a_64(prev_comm.as_bytes()),
                    next_comm_hash: fnv1a_64(next_comm.as_bytes()),
                    latency_ns,
                    cpu_id: (counter % 8) as u32,
                })
            }
            "mem_pressure" => {
                let cycle = counter % 20;
                let pages = if cycle == 0 { 256 } else if cycle == 5 { 64 } else { 0 };
                EventPayload::MemPressure(MemPressureEvent {
                    pages_reclaimed: pages,
                    zone: 1,
                })
            }
            "net_io" => {
                let sin_val = (counter as f64 * 0.05).sin();
                let tx_bytes = (1000.0 + (sin_val * 800.0)) as u64;
                let rx_bytes = (1200.0 + (sin_val * 900.0)) as u64;

                EventPayload::NetIo(NetIoEvent {
                    bytes_tx: tx_bytes,
                    bytes_rx: rx_bytes,
                    dev_name_hash: fnv1a_64(b"eth0"),
                })
            }
            "proc_lifecycle" => {
                let cycle = counter % 30;
                let (comm, uid) = if cycle == 15 {
                    ("sh", 1000)
                } else if cycle == 28 {
                    ("cargo", 1000)
                } else {
                    ("", 0)
                };

                if comm.is_empty() {
                    continue;
                }

                EventPayload::ProcLifecycle(ProcLifecycleEvent {
                    comm_hash: fnv1a_64(comm.as_bytes()),
                    uid_hash: uid as u64,
                })
            }
            _ => continue,
        };

        let event = RawEvent {
            probe: match name.as_str() {
                "cpu_sched" => ProbeId::CpuSched,
                "mem_pressure" => ProbeId::MemPressure,
                "net_io" => ProbeId::NetIo,
                "proc_lifecycle" => ProbeId::ProcLifecycle,
                _ => continue,
            },
            timestamp_ns: now_ns,
            payload,
        };

        match tx.try_send(event) {
            Ok(_) => {
                events_total.fetch_add(1, Ordering::Relaxed);
                last_event_us.store(now_us, Ordering::Relaxed);
            }
            Err(mpsc::error::TrySendError::Full(_)) => {
                events_lost.fetch_add(1, Ordering::Relaxed);
                cumulative_lost.fetch_add(1, Ordering::Relaxed);
                warn!(probe = name.as_str(), "Ring buffer channel is FULL! Event dropped (backpressure).");
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                break;
            }
        }
    }
}
