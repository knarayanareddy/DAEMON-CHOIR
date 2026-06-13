use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateSnapshot {
    pub timestamp_us: u64,
    pub window_ms: u64,
    pub stale: bool,
    pub lost_events: u64,
    pub metrics: MetricMap,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricMap {
    #[serde(rename = "cpu.sched.latency_p95")]
    pub cpu_sched_latency_p95: f64,
    #[serde(rename = "cpu.sched.context_rate")]
    pub cpu_sched_context_rate: f64,
    #[serde(rename = "mem.reclaim.pressure")]
    pub mem_reclaim_pressure: f64,
    #[serde(rename = "net.tx.bytes_norm")]
    pub net_tx_bytes_norm: f64,
    #[serde(rename = "net.rx.bytes_norm")]
    pub net_rx_bytes_norm: f64,
    #[serde(rename = "proc.exec.rate")]
    pub proc_exec_rate: f64,
}

impl Default for MetricMap {
    fn default() -> Self {
        Self {
            cpu_sched_latency_p95: 0.0,
            cpu_sched_context_rate: 0.0,
            mem_reclaim_pressure: 0.0,
            net_tx_bytes_norm: 0.0,
            net_rx_bytes_norm: 0.0,
            proc_exec_rate: 0.0,
        }
    }
}

impl MetricMap {
    pub fn get_value(&self, name: &str) -> Option<f64> {
        match name {
            "cpu.sched.latency_p95" => Some(self.cpu_sched_latency_p95),
            "cpu.sched.context_rate" => Some(self.cpu_sched_context_rate),
            "mem.reclaim.pressure" => Some(self.mem_reclaim_pressure),
            "net.tx.bytes_norm" => Some(self.net_tx_bytes_norm),
            "net.rx.bytes_norm" => Some(self.net_rx_bytes_norm),
            "proc.exec.rate" => Some(self.proc_exec_rate),
            _ => None,
        }
    }
}

/// Computes Exponential Moving Average (EMA).
/// Formula: EMA_t = alpha * Value_t + (1 - alpha) * EMA_t-1
pub fn compute_ema(current: f64, previous: f64, alpha: f64) -> f64 {
    alpha * current + (1.0 - alpha) * previous
}

/// Helper to clamp a value between a min and max and map to [0.0, 1.0].
pub fn normalize_min_max(val: f64, min: f64, max: f64) -> f64 {
    if min >= max {
        return 0.0;
    }
    let clamped = val.clamp(min, max);
    (clamped - min) / (max - min)
}
