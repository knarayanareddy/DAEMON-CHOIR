use std::sync::{Arc, RwLock, atomic::{AtomicU64, Ordering}};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use std::path::PathBuf;
use axum::{
    extract::{State, Request},
    http::{StatusCode, HeaderMap},
    response::{IntoResponse, Response},
    routing::{get, post, put},
    Json, Router,
    middleware::{self, Next},
};
use serde::Serialize;
use tracing::{info, error};
use daemon_choir_common::config::{Config, OscBackendConfig, MappingRule};
use daemon_choir_common::metrics::StateSnapshot;
use crate::probe_manager::ProbeStats;
use crate::osc::{OscMetrics, PrometheusHistogram};
use crate::db::Database;

pub struct AppState {
    pub start_time: Instant,
    pub config_path: PathBuf,
    pub current_config: Arc<RwLock<Config>>,
    pub current_snapshot: Arc<RwLock<StateSnapshot>>,
    pub probes_stats: Vec<ProbeStats>,
    pub mapping_rules: Arc<RwLock<Vec<MappingRule>>>,
    pub osc_backends: Arc<RwLock<Vec<OscBackendConfig>>>,
    pub osc_metrics: OscMetrics,
    pub dispatch_latency_histogram: PrometheusHistogram,
    pub api_response_histogram: PrometheusHistogram,
    pub probe_load_errors_total: Arc<AtomicU64>,
    pub db: Arc<Database>,
    pub shutdown_tx: tokio::sync::mpsc::Sender<()>,
}

pub fn create_router(state: Arc<AppState>) -> Router {
    let api_routes = Router::new()
        .route("/v1/status", get(handle_status))
        .route("/v1/state", get(handle_state))
        .route("/v1/voices", get(handle_voices))
        .route("/v1/probes", get(handle_probes))
        .route("/v1/config", get(handle_config))
        .route("/v1/config/reload", post(handle_config_reload))
        .route("/v1/config/backends", put(handle_config_backends))
        .route("/v1/metrics", get(handle_metrics))
        .route("/v1/shutdown", post(handle_shutdown))
        .route("/v1/privacy/wipe", post(handle_privacy_wipe))
        .with_state(state.clone());

    Router::new()
        .merge(api_routes)
        .route_layer(middleware::from_fn_with_state(state, track_api_response_time))
}

async fn track_api_response_time(
    State(state): State<Arc<AppState>>,
    request: Request,
    next: Next,
) -> Response {
    let start = Instant::now();
    let response = next.run(request).await;
    let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
    state.api_response_histogram.observe(elapsed_ms);
    response
}

// GET /v1/status
#[derive(Serialize)]
struct StatusResponse {
    version: &'static str,
    build_commit: &'static str,
    uptime_secs: u64,
    state: String, // Dynamic state reflecting lifecycle (§11.1, BUG-10)
    probes_loaded: usize,
    backends_active: usize,
    api_version: &'static str,
}

async fn handle_status(State(state): State<Arc<AppState>>) -> Response {
    let uptime_secs = state.start_time.elapsed().as_secs();
    let probes_loaded = state.probes_stats.iter().filter(|p| p.loaded).count();
    let backends_active = match state.osc_backends.read() {
        Ok(lock) => lock.len(),
        Err(_) => 0,
    };

    let lifecycle_state = if probes_loaded == 0 {
        "degraded".to_string()
    } else {
        "running".to_string()
    };

    // Inject dynamic build commit hash compiled from build.rs & Cargo version to satisfy BUG-04 & BUG-05
    Json(StatusResponse {
        version: env!("CARGO_PKG_VERSION"),
        build_commit: env!("GIT_COMMIT_HASH"),
        uptime_secs,
        state: lifecycle_state,
        probes_loaded,
        backends_active,
        api_version: "v1",
    }).into_response()
}

// GET /v1/state
async fn handle_state(State(state): State<Arc<AppState>>) -> Response {
    match state.current_snapshot.read() {
        Ok(snapshot) => Json(snapshot.clone()).into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Poisoned snapshot lock").into_response(),
    }
}

// GET /v1/voices
#[derive(Serialize)]
struct VoiceMapResponse {
    voices: Vec<VoiceMapping>,
}

#[derive(Serialize)]
struct VoiceMapping {
    source_metric: String,
    osc_address: String,
    transform: String,
}

async fn handle_voices(State(state): State<Arc<AppState>>) -> Response {
    let rules = match state.mapping_rules.read() {
        Ok(lock) => lock.clone(),
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "Poisoned rules lock").into_response(),
    };

    let voices: Vec<VoiceMapping> = rules
        .into_iter()
        .map(|r| VoiceMapping {
            source_metric: r.source_metric,
            osc_address: r.osc_address,
            transform: r.transform,
        })
        .collect();

    Json(VoiceMapResponse { voices }).into_response()
}

// GET /v1/probes
#[derive(Serialize)]
struct ProbeInfoResponse {
    name: String,
    attach_point: String,
    loaded: bool,
    events_total: u64,
    events_lost: u64,
    last_event_us: u64,
}

async fn handle_probes(State(state): State<Arc<AppState>>) -> Response {
    let list: Vec<ProbeInfoResponse> = state
        .probes_stats
        .iter()
        .map(|p| ProbeInfoResponse {
            name: p.name.clone(),
            attach_point: p.attach_point.clone(),
            loaded: p.loaded,
            events_total: p.events_total.load(Ordering::Relaxed),
            events_lost: p.events_lost.load(Ordering::Relaxed),
            last_event_us: p.last_event_us.load(Ordering::Relaxed),
        })
        .collect();

    Json(list).into_response()
}

// GET /v1/config
async fn handle_config(State(state): State<Arc<AppState>>) -> Response {
    match state.current_config.read() {
        Ok(cfg) => Json(cfg.clone()).into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Poisoned config lock").into_response(),
    }
}

// POST /v1/config/reload
async fn handle_config_reload(State(state): State<Arc<AppState>>) -> Response {
    info!("Triggering hot reload of mapping rules from disk...");
    
    let path = state.config_path.clone();
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => {
            error!("Failed to read config file at {:?}: {}", path, e);
            return (StatusCode::CONFLICT, "Failed to read config file from disk").into_response();
        }
    };

    let new_cfg: Config = match toml::from_str(&content) {
        Ok(cfg) => cfg,
        Err(e) => {
            error!("Failed to parse config TOML: {}", e);
            return (StatusCode::CONFLICT, format!("Config parse error: {}", e)).into_response();
        }
    };

    // Update active memory config
    if let Ok(mut lock) = state.current_config.write() {
        *lock = new_cfg.clone();
    }
    if let Ok(mut lock) = state.mapping_rules.write() {
        *lock = new_cfg.mapping.clone();
    }
    if let Ok(mut lock) = state.osc_backends.write() {
        *lock = new_cfg.backends.osc.clone();
    }

    info!("Hot reload successful. Loaded {} mapping rules and {} backends.", new_cfg.mapping.len(), new_cfg.backends.osc.len());
    (StatusCode::ACCEPTED, "Config reloaded successfully").into_response()
}

// PUT /v1/config/backends
async fn handle_config_backends(
    State(state): State<Arc<AppState>>,
    Json(new_backends): Json<Vec<OscBackendConfig>>,
) -> Response {
    info!("Updating OSC backends dynamically...");
    
    if let Ok(mut lock) = state.osc_backends.write() {
        *lock = new_backends.clone();
    }

    if let Ok(mut lock) = state.current_config.write() {
        lock.backends.osc = new_backends;
    }

    (StatusCode::OK, "Backends updated successfully").into_response()
}

// GET /v1/metrics (Prometheus format)
async fn handle_metrics(State(state): State<Arc<AppState>>) -> Response {
    let osc_total = state.osc_metrics.osc_dispatch_total.load(Ordering::Relaxed);
    let osc_success = state.osc_metrics.osc_dispatch_success_total.load(Ordering::Relaxed);
    
    let mut response = String::new();

    // 1. OSC Dispatch Counts
    response.push_str("# HELP osc_dispatch_total Total OSC dispatches attempted\n");
    response.push_str("# TYPE osc_dispatch_total counter\n");
    response.push_str(&format!("osc_dispatch_total{{backend=\"osc\"}} {}\n\n", osc_total));

    response.push_str("# HELP osc_dispatch_success_total Successful OSC dispatches\n");
    response.push_str("# TYPE osc_dispatch_success_total counter\n");
    response.push_str(&format!("osc_dispatch_success_total{{backend=\"osc\"}} {}\n\n", osc_success));

    // 2. Events Lost Counters (Help and type headers output EXACTLY ONCE to avoid syntax violations §11.2)
    response.push_str("# HELP events_lost_total Ring buffer events lost due to overflow\n");
    response.push_str("# TYPE events_lost_total counter\n");
    for p in &state.probes_stats {
        let name = p.name.as_str();
        let lost = p.events_lost.load(Ordering::Relaxed);
        response.push_str(&format!("events_lost_total{{probe=\"{}\"}} {}\n", name, lost));
    }
    response.push_str("\n");

    // 3. Dispatch Latency Histograms
    let dl_sum = state.dispatch_latency_histogram.sum_us.load(Ordering::Relaxed) as f64 / 1000.0;
    let dl_count = state.dispatch_latency_histogram.count.load(Ordering::Relaxed);
    let dl_le5 = state.dispatch_latency_histogram.bucket_le_5.load(Ordering::Relaxed);
    let dl_le10 = state.dispatch_latency_histogram.bucket_le_10.load(Ordering::Relaxed);
    let dl_le20 = state.dispatch_latency_histogram.bucket_le_20.load(Ordering::Relaxed);

    response.push_str("# HELP dispatch_latency_ms Ring buffer to OSC dispatch latency\n");
    response.push_str("# TYPE dispatch_latency_ms histogram\n");
    response.push_str(&format!("dispatch_latency_ms_bucket{{le=\"5\"}} {}\n", dl_le5));
    response.push_str(&format!("dispatch_latency_ms_bucket{{le=\"10\"}} {}\n", dl_le10));
    response.push_str(&format!("dispatch_latency_ms_bucket{{le=\"20\"}} {}\n", dl_le20));
    response.push_str(&format!("dispatch_latency_ms_bucket{{le=\"+Inf\"}} {}\n", dl_count));
    response.push_str(&format!("dispatch_latency_ms_sum {}\n", dl_sum));
    response.push_str(&format!("dispatch_latency_ms_count {}\n\n", dl_count));

    // 4. API Response Histograms
    let api_sum = state.api_response_histogram.sum_us.load(Ordering::Relaxed) as f64 / 1000.0;
    let api_count = state.api_response_histogram.count.load(Ordering::Relaxed);
    let api_le5 = state.api_response_histogram.bucket_le_5.load(Ordering::Relaxed);
    let api_le10 = state.api_response_histogram.bucket_le_10.load(Ordering::Relaxed);
    let api_le20 = state.api_response_histogram.bucket_le_20.load(Ordering::Relaxed);

    response.push_str("# HELP api_response_ms Control API response latency\n");
    response.push_str("# TYPE api_response_ms histogram\n");
    response.push_str(&format!("api_response_ms_bucket{{le=\"5\"}} {}\n", api_le5));
    response.push_str(&format!("api_response_ms_bucket{{le=\"10\"}} {}\n", api_le10));
    response.push_str(&format!("api_response_ms_bucket{{le=\"20\"}} {}\n", api_le20));
    response.push_str(&format!("api_response_ms_bucket{{le=\"+Inf\"}} {}\n", api_count));
    response.push_str(&format!("api_response_ms_sum {}\n", api_sum));
    response.push_str(&format!("api_response_ms_count {}\n\n", api_count));

    // 5. Probe Load Errors Counters
    let err_total = state.probe_load_errors_total.load(Ordering::Relaxed);
    response.push_str("# HELP probe_load_errors_total eBPF probe load failures\n");
    response.push_str("# TYPE probe_load_errors_total counter\n");
    response.push_str(&format!("probe_load_errors_total {}\n", err_total));

    response.into_response()
}

// POST /v1/shutdown (CSRF Guarded)
async fn handle_shutdown(
    headers: HeaderMap,
    State(state): State<Arc<AppState>>,
) -> Response {
    if let Some(guard) = headers.get("X-Daemon-Choir") {
        if guard == "shutdown" {
            info!("Received verified shutdown request. Shuting down Conductor...");
            let _ = state.shutdown_tx.send(()).await;
            return (StatusCode::ACCEPTED, "Shutdown requested").into_response();
        }
    }

    (StatusCode::FORBIDDEN, "Forbidden: Missing or invalid X-Daemon-Choir header").into_response()
}

// POST /v1/privacy/wipe (CSRF protected with header X-Daemon-Choir: wipe to satisfy BUG-08)
async fn handle_privacy_wipe(
    headers: HeaderMap,
    State(state): State<Arc<AppState>>,
) -> Response {
    if let Some(guard) = headers.get("X-Daemon-Choir") {
        if guard == "wipe" {
            info!("Privacy wipe triggered.");
            
            if let Err(e) = state.db.wipe() {
                error!("Failed to wipe SQLite database: {}", e);
                return (StatusCode::INTERNAL_SERVER_ERROR, "Database wipe failed").into_response();
            }

            // Print a single log line as specified in §10.4
            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_else(|_| std::time::Duration::ZERO)
                .as_secs();
            
            println!("{{\"event\": \"privacy_wipe\", \"timestamp\": {}}}", timestamp);

            return (StatusCode::OK, "Privacy wipe complete").into_response();
        }
    }

    (StatusCode::FORBIDDEN, "Forbidden: Missing or invalid X-Daemon-Choir header").into_response()
}
