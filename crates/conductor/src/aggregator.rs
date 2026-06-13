use std::sync::{Arc, RwLock, atomic::{AtomicU64, Ordering}};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc;
use tracing::{info, warn};
use daemon_choir_common::events::{RawEvent, EventPayload};
use daemon_choir_common::metrics::{StateSnapshot, MetricMap, compute_ema, normalize_min_max};

pub struct Aggregator {
    rx: mpsc::Receiver<RawEvent>,
    tx: mpsc::Sender<StateSnapshot>,
    db_tx: Option<mpsc::Sender<StateSnapshot>>, // Optional non-blocking DB persistence channel
    window_ms: u64,
    shared_state: Arc<RwLock<StateSnapshot>>,
    snapshot_timestamp_us: Arc<AtomicU64>, // Shared timestamp to record latest snapshot time for dispatch latency metrics
    cumulative_lost_events: Arc<AtomicU64>, // Shared reference to read lost events delta per window (§11.2, BUG-03)
}

impl Aggregator {
    pub fn new(
        rx: mpsc::Receiver<RawEvent>,
        tx: mpsc::Sender<StateSnapshot>,
        db_tx: Option<mpsc::Sender<StateSnapshot>>,
        window_ms: u64,
        shared_state: Arc<RwLock<StateSnapshot>>,
        snapshot_timestamp_us: Arc<AtomicU64>,
        cumulative_lost_events: Arc<AtomicU64>,
    ) -> Self {
        let clamped_window = window_ms.clamp(10, 500);
        if clamped_window != window_ms {
            warn!("Requested window_ms {} was clamped to {}ms", window_ms, clamped_window);
        }

        Self {
            rx,
            tx,
            db_tx,
            window_ms: clamped_window,
            shared_state,
            snapshot_timestamp_us,
            cumulative_lost_events,
        }
    }

    pub async fn run(mut self) {
        info!("Starting Aggregation Engine with window_ms = {}ms", self.window_ms);
        
        let mut interval = tokio::time::interval(Duration::from_millis(self.window_ms));
        
        // Starvation Bug Fix: Consume the first immediate tick so we wait a full window before aggregating §11.1
        interval.tick().await;

        let mut current_window_events: Vec<RawEvent> = Vec::new();
        let alpha = 0.3;
        
        let mut ema_cpu_latency = 0.0;
        let mut ema_cpu_rate = 0.0;
        let mut ema_mem_pressure = 0.0;
        let mut ema_net_tx = 0.0;
        let mut ema_net_rx = 0.0;
        let mut ema_proc_exec = 0.0;

        let mut consecutive_empty_windows = 0;

        loop {
            tokio::select! {
                maybe_event = self.rx.recv() => {
                    match maybe_event {
                        Some(event) => {
                            current_window_events.push(event);
                        }
                        None => {
                            info!("Raw event channel closed. Aggregator shutting down.");
                            break;
                        }
                    }
                }
                _ = interval.tick() => {
                    let now_us = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or(Duration::ZERO)
                        .as_micros() as u64;

                    // Record latest snapshot timestamp for dispatch latency tracking
                    self.snapshot_timestamp_us.store(now_us, Ordering::Relaxed);

                    // Track dynamic stale state: stale=true if no telemetry for 5 consecutive windows §11.1, BUG-03
                    if current_window_events.is_empty() {
                        consecutive_empty_windows += 1;
                    } else {
                        consecutive_empty_windows = 0;
                    }
                    let stale = consecutive_empty_windows >= 5;

                    // Swap lost events count for this window delta (§11.2, BUG-03)
                    let lost_events_delta = self.cumulative_lost_events.swap(0, Ordering::SeqCst);

                    let mut cpu_latencies: Vec<u64> = Vec::new();
                    let mut context_switches = 0u64;
                    let mut pages_reclaimed = 0u64;
                    let mut bytes_tx = 0u64;
                    let mut bytes_rx = 0u64;
                    let mut proc_execs = 0u64;

                    for event in &current_window_events {
                        match &event.payload {
                            EventPayload::CpuSched(e) => {
                                cpu_latencies.push(e.latency_ns);
                                context_switches += 1;
                            }
                            EventPayload::MemPressure(e) => {
                                pages_reclaimed += e.pages_reclaimed;
                            }
                            EventPayload::NetIo(e) => {
                                bytes_tx += e.bytes_tx;
                                bytes_rx += e.bytes_rx;
                            }
                            EventPayload::ProcLifecycle(_) => {
                                proc_execs += 1;
                            }
                        }
                    }

                    let raw_p95_latency = if !cpu_latencies.is_empty() {
                        cpu_latencies.sort_unstable();
                        let idx = (cpu_latencies.len() as f64 * 0.95).floor() as usize;
                        let idx_clamped = idx.min(cpu_latencies.len() - 1);
                        cpu_latencies[idx_clamped] as f64
                    } else {
                        0.0
                    };

                    let raw_cpu_rate = context_switches as f64;
                    let raw_mem_pressure = pages_reclaimed as f64;
                    let raw_net_tx = bytes_tx as f64;
                    let raw_net_rx = bytes_rx as f64;
                    let raw_proc_exec = proc_execs as f64;

                    ema_cpu_latency = compute_ema(raw_p95_latency, ema_cpu_latency, alpha);
                    ema_cpu_rate = compute_ema(raw_cpu_rate, ema_cpu_rate, alpha);
                    ema_mem_pressure = compute_ema(raw_mem_pressure, ema_mem_pressure, alpha);
                    ema_net_tx = compute_ema(raw_net_tx, ema_net_tx, alpha);
                    ema_net_rx = compute_ema(raw_net_rx, ema_net_rx, alpha);
                    ema_proc_exec = compute_ema(raw_proc_exec, ema_proc_exec, alpha);

                    let norm_cpu_latency = normalize_min_max(ema_cpu_latency, 1000.0, 20000.0);
                    let norm_cpu_rate = normalize_min_max(ema_cpu_rate, 0.0, 80.0);
                    let norm_mem_pressure = normalize_min_max(ema_mem_pressure, 0.0, 500.0);
                    let norm_net_tx = normalize_min_max(ema_net_tx, 0.0, 30000.0);
                    let norm_net_rx = normalize_min_max(ema_net_rx, 0.0, 30000.0);
                    let norm_proc_exec = normalize_min_max(ema_proc_exec, 0.0, 3.0);

                    let metrics = MetricMap {
                        cpu_sched_latency_p95: norm_cpu_latency,
                        cpu_sched_context_rate: norm_cpu_rate,
                        mem_reclaim_pressure: norm_mem_pressure,
                        net_tx_bytes_norm: norm_net_tx,
                        net_rx_bytes_norm: norm_net_rx,
                        proc_exec_rate: norm_proc_exec,
                    };

                    let snapshot = StateSnapshot {
                        timestamp_us: now_us,
                        window_ms: self.window_ms,
                        stale,
                        lost_events: lost_events_delta,
                        metrics,
                    };

                    if let Ok(mut lock) = self.shared_state.write() {
                        *lock = snapshot.clone();
                    }

                    // Forward snapshot down the audio pipeline
                    if let Err(e) = self.tx.try_send(snapshot.clone()) {
                        match e {
                            mpsc::error::TrySendError::Full(_) => {
                                warn!("Mapping engine queue full! Dropping aggregated state snapshot (backpressure).");
                            }
                            mpsc::error::TrySendError::Closed(_) => {
                                info!("Mapping engine channel closed. Aggregator shutting down.");
                                break;
                            }
                        }
                    }

                    // DB Tap non-blocking try_send: guarantees zero pipeline stalling on database backpressure §11.1
                    if let Some(db_tx) = &self.db_tx {
                        if let Err(_) = db_tx.try_send(snapshot) {
                            // SQLite queue is full; drop snapshot write to prioritize audio continuity (§1.2)
                        }
                    }

                    current_window_events.clear();
                }
            }
        }
    }
}
