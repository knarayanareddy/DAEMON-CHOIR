use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub daemon: DaemonConfig,
    #[serde(default)]
    pub probes: ProbesConfig,
    #[serde(default)]
    pub backends: BackendsConfig,
    #[serde(default)]
    pub mapping: Vec<MappingRule>,
    #[serde(default)]
    pub meta: MetaConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonConfig {
    #[serde(default = "default_window_ms")]
    pub window_ms: u64,
    #[serde(default = "default_log_level")]
    pub log_level: String,
    #[serde(default = "default_log_format")]
    pub log_format: String,
    #[serde(default = "default_control_api_port")]
    pub control_api_port: u16,
    #[serde(default = "default_simulate")]
    pub simulate: bool,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            window_ms: default_window_ms(),
            log_level: default_log_level(),
            log_format: default_log_format(),
            control_api_port: default_control_api_port(),
            simulate: default_simulate(),
        }
    }
}

fn default_window_ms() -> u64 { 50 }
fn default_log_level() -> String { "info".to_string() }
fn default_log_format() -> String { "json".to_string() }
fn default_control_api_port() -> u16 { 9876 }
fn default_simulate() -> bool { true } // default to true so it runs out-of-the-box everywhere!

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbesConfig {
    #[serde(default = "default_probes_enabled")]
    pub enabled: Vec<String>,
}

impl Default for ProbesConfig {
    fn default() -> Self {
        Self {
            enabled: default_probes_enabled(),
        }
    }
}

fn default_probes_enabled() -> Vec<String> {
    vec![
        "cpu_sched".to_string(),
        "mem_pressure".to_string(),
        "net_io".to_string(),
        "proc_lifecycle".to_string(),
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BackendsConfig {
    #[serde(default)]
    pub osc: Vec<OscBackendConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OscBackendConfig {
    pub name: String,
    pub address: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MappingRule {
    pub source_metric: String,
    pub osc_address: String,
    #[serde(default = "default_transform")]
    pub transform: String, // linear | exponential | logarithmic | step
    #[serde(default = "default_input_range")]
    pub input_range: [f64; 2],
    #[serde(default = "default_output_range")]
    pub output_range: [f64; 2],
}

fn default_transform() -> String { "linear".to_string() }
fn default_input_range() -> [f64; 2] { [0.0, 1.0] }
fn default_output_range() -> [f64; 2] { [0.0, 1.0] }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetaConfig {
    #[serde(default = "default_schema_version")]
    pub schema_version: String,
}

impl Default for MetaConfig {
    fn default() -> Self {
        Self {
            schema_version: default_schema_version(),
        }
    }
}

fn default_schema_version() -> String { "1".to_string() }

impl Default for Config {
    fn default() -> Self {
        Self {
            daemon: DaemonConfig::default(),
            probes: ProbesConfig::default(),
            backends: BackendsConfig::default(),
            mapping: vec![],
            meta: MetaConfig::default(),
        }
    }
}
