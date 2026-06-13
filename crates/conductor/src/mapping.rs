use std::sync::{Arc, RwLock};
use tokio::sync::mpsc;
use tracing::{info, warn};
use daemon_choir_common::metrics::StateSnapshot;
use daemon_choir_common::config::MappingRule;

#[derive(Debug, Clone)]
pub struct MappedParam {
    pub osc_address: String,
    pub value: f64,
}

pub struct MappingEngine {
    rx: mpsc::Receiver<StateSnapshot>,
    tx: mpsc::Sender<Vec<MappedParam>>,
    rules: Arc<RwLock<Vec<MappingRule>>>,
}

impl MappingEngine {
    pub fn new(
        rx: mpsc::Receiver<StateSnapshot>,
        tx: mpsc::Sender<Vec<MappedParam>>,
        rules: Arc<RwLock<Vec<MappingRule>>>,
    ) -> Self {
        Self { rx, tx, rules }
    }

    pub async fn run(mut self) {
        info!("Starting Mapping Engine...");
        
        while let Some(snapshot) = self.rx.recv().await {
            let active_rules = match self.rules.read() {
                Ok(lock) => lock.clone(),
                Err(_) => {
                    warn!("Mapping rules lock poisoned. Skipping this frame.");
                    continue;
                }
            };

            let mut mapped_params = Vec::new();

            for rule in &active_rules {
                if let Some(val) = snapshot.metrics.get_value(&rule.source_metric) {
                    let mapped_val = apply_transform(
                        val,
                        &rule.transform,
                        rule.input_range[0],
                        rule.input_range[1],
                        rule.output_range[0],
                        rule.output_range[1],
                    );
                    
                    mapped_params.push(MappedParam {
                        osc_address: rule.osc_address.clone(),
                        value: mapped_val,
                    });
                }
            }

            // Send mapped params to the OSC dispatcher
            if let Err(e) = self.tx.try_send(mapped_params) {
                match e {
                    mpsc::error::TrySendError::Full(_) => {
                        warn!("OSC Dispatcher queue full! Dropping mapped parameters (backpressure).");
                    }
                    mpsc::error::TrySendError::Closed(_) => {
                        info!("OSC Dispatcher channel closed. Mapping Engine exiting.");
                        break;
                    }
                }
            }
        }
    }
}

fn apply_transform(
    x: f64,
    transform: &str,
    in_min: f64,
    in_max: f64,
    out_min: f64,
    out_max: f64,
) -> f64 {
    // Clamp/normalize input to [in_min, in_max]
    if in_min >= in_max {
        return out_min;
    }
    let clamped_input = x.clamp(in_min, in_max);
    let ratio = (clamped_input - in_min) / (in_max - in_min);

    match transform.to_lowercase().as_str() {
        "exponential" => {
            if out_min > 0.0 && out_max > 0.0 {
                out_min * (out_max / out_min).powf(ratio)
            } else {
                // Fallback to quadratic mapping if bounds include 0 or negative values
                out_min + (out_max - out_min) * ratio.powi(2)
            }
        }
        "logarithmic" => {
            // log(1 + ratio * 9) / log(10) maps 0.0-1.0 to 0.0-1.0 logarithmically
            let log_ratio = (1.0 + ratio * 9.0).ln() / 10.0f64.ln();
            out_min + (out_max - out_min) * log_ratio
        }
        "step" => {
            let steps = 5.0; // 5 discrete steps
            let stepped_ratio = (ratio * steps).floor() / steps;
            out_min + (out_max - out_min) * stepped_ratio
        }
        "linear" | _ => {
            out_min + (out_max - out_min) * ratio
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_apply_transform_linear() {
        let val = apply_transform(0.5, "linear", 0.0, 1.0, 100.0, 200.0);
        assert!((val - 150.0).abs() < 1e-5);
    }

    #[test]
    fn test_apply_transform_step() {
        let val = apply_transform(0.24, "step", 0.0, 1.0, 0.0, 100.0);
        assert_eq!(val, 20.0); // (0.24 * 5).floor() / 5 = 0.2, 0.2 * 100 = 20
    }
}
