use std::net::UdpSocket;
use std::sync::{Arc, RwLock, atomic::{AtomicU64, Ordering}};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc;
use tracing::{info, warn, error};
use rosc::{OscPacket, OscMessage, OscType, OscBundle};
use daemon_choir_common::config::OscBackendConfig;
use crate::mapping::MappedParam;

#[derive(Clone)]
pub struct OscMetrics {
    pub osc_dispatch_total: Arc<AtomicU64>,
    pub osc_dispatch_success_total: Arc<AtomicU64>,
}

impl Default for OscMetrics {
    fn default() -> Self {
        Self {
            osc_dispatch_total: Arc::new(AtomicU64::new(0)),
            osc_dispatch_success_total: Arc::new(AtomicU64::new(0)),
        }
    }
}

#[derive(Clone)]
pub struct PrometheusHistogram {
    pub sum_us: Arc<AtomicU64>,
    pub count: Arc<AtomicU64>,
    pub bucket_le_5: Arc<AtomicU64>,
    pub bucket_le_10: Arc<AtomicU64>,
    pub bucket_le_20: Arc<AtomicU64>,
}

impl Default for PrometheusHistogram {
    fn default() -> Self {
        Self {
            sum_us: Arc::new(AtomicU64::new(0)),
            count: Arc::new(AtomicU64::new(0)),
            bucket_le_5: Arc::new(AtomicU64::new(0)),
            bucket_le_10: Arc::new(AtomicU64::new(0)),
            bucket_le_20: Arc::new(AtomicU64::new(0)),
        }
    }
}

impl PrometheusHistogram {
    pub fn observe(&self, ms: f64) {
        let us = (ms * 1000.0) as u64;
        self.sum_us.fetch_add(us, Ordering::Relaxed);
        self.count.fetch_add(1, Ordering::Relaxed);
        if ms <= 5.0 {
            self.bucket_le_5.fetch_add(1, Ordering::Relaxed);
        }
        if ms <= 10.0 {
            self.bucket_le_10.fetch_add(1, Ordering::Relaxed);
        }
        if ms <= 20.0 {
            self.bucket_le_20.fetch_add(1, Ordering::Relaxed);
        }
    }
}

pub struct OscDispatcher {
    rx: mpsc::Receiver<Vec<MappedParam>>,
    backends: Arc<RwLock<Vec<OscBackendConfig>>>,
    socket: UdpSocket,
    metrics: OscMetrics,
    latency_histogram: PrometheusHistogram,
    snapshot_timestamp_us: Arc<AtomicU64>, // Tracks latest snapshot timestamp to measure latency
}

impl OscDispatcher {
    pub fn new(
        rx: mpsc::Receiver<Vec<MappedParam>>,
        backends: Arc<RwLock<Vec<OscBackendConfig>>>,
        metrics: OscMetrics,
        latency_histogram: PrometheusHistogram,
        snapshot_timestamp_us: Arc<AtomicU64>,
    ) -> Self {
        let socket = UdpSocket::bind("127.0.0.1:0").unwrap_or_else(|e| {
            error!("Failed to bind local OSC UDP socket: {}", e);
            panic!("Could not initialize OSC dispatcher: {}", e);
        });
        socket.set_nonblocking(true).ok();

        Self {
            rx,
            backends,
            socket,
            metrics,
            latency_histogram,
            snapshot_timestamp_us,
        }
    }

    pub async fn run(mut self) {
        info!("Starting OSC Dispatcher...");
        
        while let Some(params) = self.rx.recv().await {
            if params.is_empty() {
                continue;
            }

            let active_backends = match self.backends.read() {
                Ok(lock) => lock.clone(),
                Err(_) => {
                    warn!("OSC Backends lock poisoned. Skipping dispatch.");
                    continue;
                }
            };

            // Package all mapped parameters into an OSC 1.1 bundle to resolve network jitter & packet reordering §5.4
            let mut osc_messages = Vec::new();
            for param in &params {
                osc_messages.push(OscPacket::Message(OscMessage {
                    addr: param.osc_address.clone(),
                    args: vec![OscType::Float(param.value as f32)],
                }));
            }

            // Convert SystemTime to NTP OscTime format (32-bit seconds, 32-bit fraction)
            let now = SystemTime::now();
            let duration = now.duration_since(UNIX_EPOCH).unwrap_or_default();
            let secs = duration.as_secs() as u32;
            let nanos = duration.subsec_nanos() as u64;
            let frac = ((nanos << 32) / 1_000_000_000) as u32;
            let osc_timetag = rosc::OscTime::from((secs, frac));

            let bundle = OscPacket::Bundle(OscBundle {
                timetag: osc_timetag,
                content: osc_messages,
            });

            let encoded_packet = match rosc::encoder::encode(&bundle) {
                Ok(buf) => buf,
                Err(e) => {
                    error!("Failed to encode OSC Bundle: {}", e);
                    continue;
                }
            };

            for backend in &active_backends {
                self.metrics.osc_dispatch_total.fetch_add(1, Ordering::Relaxed);
                
                if let Err(e) = self.socket.send_to(&encoded_packet, &backend.address) {
                    warn!(backend = backend.name.as_str(), addr = backend.address.as_str(), "Failed to dispatch OSC Bundle packet: {}", e);
                } else {
                    self.metrics.osc_dispatch_success_total.fetch_add(1, Ordering::Relaxed);
                }
            }

            // Observe dispatch latency end-to-end (from aggregator timestamp to OSC socket send) §11.1
            let now_us = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_micros() as u64;
            let start_us = self.snapshot_timestamp_us.load(Ordering::Relaxed);
            if start_us > 0 && now_us >= start_us {
                let diff_ms = (now_us - start_us) as f64 / 1000.0;
                self.latency_histogram.observe(diff_ms);
            }
        }
    }
}
