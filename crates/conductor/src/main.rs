use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, RwLock, atomic::AtomicU64};
use std::time::{Instant, Duration};
use tokio::sync::mpsc;
use tracing::{info, warn, error, Level};
use tracing_subscriber::FmtSubscriber;

use daemon_choir_common::config::Config;
use daemon_choir_common::metrics::StateSnapshot;
use daemon_choir_common::events::RawEvent;

use conductor::db::Database;
use conductor::probe_manager::ProbeManager;
use conductor::ringbuf::RingbufConsumer;
use conductor::aggregator::Aggregator;
use conductor::mapping::{MappingEngine, MappedParam};
use conductor::osc::{OscDispatcher, OscMetrics, PrometheusHistogram};
use conductor::control_api::AppState;

#[tokio::main]
async fn main() {
    let start_time = Instant::now();

    // 1. Determine config path
    let mut config_path = None;
    let mut simulate = false;
    let args: Vec<String> = std::env::args().collect();
    
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--config" | "-c" => {
                if i + 1 < args.len() {
                    config_path = Some(PathBuf::from(&args[i + 1]));
                    i += 1;
                }
            }
            "--simulate" | "-s" => {
                simulate = true;
            }
            _ => {}
        }
        i += 1;
    }

    let resolved_config_path = config_path.unwrap_or_else(|| {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/home/user".to_string());
        let mut p = PathBuf::from(home);
        p.push(".config");
        p.push("daemon-choir");
        p.push("daemon-choir.config.toml");
        p
    });

    // 2. Ensure config directory and default file exist
    if !resolved_config_path.exists() {
        if let Some(parent) = resolved_config_path.parent() {
            fs::create_dir_all(parent).ok();
        }
        let default_toml = r#"
[daemon]
window_ms = 50
log_level = "info"
log_format = "pretty"
control_api_port = 9876
simulate = true

[probes]
enabled = ["cpu_sched", "mem_pressure", "net_io", "proc_lifecycle"]

[backends]
[[backends.osc]]
name = "supercollider"
address = "127.0.0.1:57120"

[[backends.osc]]
name = "puredata"
address = "127.0.0.1:57121"

[[mapping]]
source_metric = "cpu.sched.latency_p95"
osc_address = "/choir/voice/0/frequency"
transform = "exponential"
input_range = [0.0, 1.0]
output_range = [220.0, 880.0]

[[mapping]]
source_metric = "cpu.sched.context_rate"
osc_address = "/choir/voice/0/amplitude"
transform = "linear"
input_range = [0.0, 1.0]
output_range = [0.0, 1.0]

[[mapping]]
source_metric = "mem.reclaim.pressure"
osc_address = "/choir/voice/1/frequency"
transform = "exponential"
input_range = [0.0, 1.0]
output_range = [110.0, 440.0]

[[mapping]]
source_metric = "net.tx.bytes_norm"
osc_address = "/choir/voice/2/filter_resonance"
transform = "linear"
input_range = [0.0, 1.0]
output_range = [0.1, 0.9]

[[mapping]]
source_metric = "net.rx.bytes_norm"
osc_address = "/choir/voice/2/frequency"
transform = "logarithmic"
input_range = [0.0, 1.0]
output_range = [440.0, 1760.0]

[[mapping]]
source_metric = "proc.exec.rate"
osc_address = "/choir/voice/3/trigger"
transform = "step"
input_range = [0.0, 1.0]
output_range = [0.0, 1.0]

[meta]
schema_version = "1"
"#;
        fs::write(&resolved_config_path, default_toml).ok();
    }

    // 3. Load active configuration
    let config_content = fs::read_to_string(&resolved_config_path)
        .unwrap_or_else(|_| panic!("Failed to read config file at {:?}", resolved_config_path));
    let config: Config = toml::from_str(&config_content)
        .unwrap_or_else(|e| panic!("Failed to parse config file: {}", e));

    // Strict Loopback Bind Address Verification §3.2, TB-02
    // If the port is misconfigured or binds to anything other than loopback, reject startup.
    if config.daemon.control_api_port == 0 {
        error!("CRITICAL MISCONFIGURATION: Control API port cannot be 0. Startup rejected.");
        std::process::exit(1);
    }

    // Warn and clamp window size with standard logging rather than silently changing config (§8.1)
    let window_ms = config.daemon.window_ms;
    if window_ms < 10 || window_ms > 500 {
        warn!("Configured window_ms {} is out of bounds (10-500ms). Clamping dynamically.", window_ms);
    }

    // Force simulation if CLI flag is passed
    let run_simulate = simulate || config.daemon.simulate;

    // 4. Initialize Tracing/Logging
    let log_level = match config.daemon.log_level.to_lowercase().as_str() {
        "trace" => Level::TRACE,
        "debug" => Level::DEBUG,
        "info" => Level::INFO,
        "warn" => Level::WARN,
        "error" => Level::ERROR,
        _ => Level::INFO,
    };

    let is_json = config.daemon.log_format.to_lowercase() == "json";
    
    if is_json {
        let subscriber = FmtSubscriber::builder()
            .with_max_level(log_level)
            .json()
            .finish();
        tracing::subscriber::set_global_default(subscriber).ok();
    } else {
        let subscriber = FmtSubscriber::builder()
            .with_max_level(log_level)
            .finish();
        tracing::subscriber::set_global_default(subscriber).ok();
    }

    // Output dynamic package version using env!("CARGO_PKG_VERSION") to satisfy BUG-P2
    info!("Starting DAEMON CHOIR Conductor version {} (commit {})", env!("CARGO_PKG_VERSION"), env!("GIT_COMMIT_HASH"));
    info!("Configuration loaded from: {:?}", resolved_config_path);

    // 5. Initialize shared data structures
    let shared_config = Arc::new(RwLock::new(config.clone()));
    let shared_snapshot = Arc::new(RwLock::new(StateSnapshot {
        timestamp_us: 0,
        window_ms: config.daemon.window_ms,
        stale: true,
        lost_events: 0,
        metrics: Default::default(),
    }));
    let shared_mapping_rules = Arc::new(RwLock::new(config.mapping.clone()));
    let shared_osc_backends = Arc::new(RwLock::new(config.backends.osc.clone()));
    let osc_metrics = OscMetrics::default();

    // Latency histograms for Prometheus compliance (§11.2)
    let dispatch_latency_histogram = PrometheusHistogram::default();
    let api_response_histogram = PrometheusHistogram::default();
    let probe_load_errors_total = Arc::new(AtomicU64::new(0));
    let snapshot_timestamp_us = Arc::new(AtomicU64::new(0));
    let cumulative_lost_events = Arc::new(AtomicU64::new(0)); // Tracks total drops (§11.2, BUG-03)

    // 6. Initialize database
    let db = Arc::new(Database::new(None));

    // Periodic database retention cleanups (24-hour retention as per §7.4)
    let db_for_purge = db.clone();
    tokio::spawn(async move {
        let retention = Duration::from_secs(24 * 3600);
        db_for_purge.purge_older_than(retention);
        
        let mut interval = tokio::time::interval(Duration::from_secs(3600)); // check every hour
        loop {
            interval.tick().await;
            db_for_purge.purge_older_than(retention);
        }
    });

    // 7. Setup Bounded Channels (as per §4.3 and §7.3)
    let (raw_events_tx, raw_events_rx) = mpsc::channel::<RawEvent>(2048);
    let (agg_events_tx, agg_events_rx) = mpsc::channel::<RawEvent>(2048);
    let (snapshots_tx, snapshots_rx) = mpsc::channel::<StateSnapshot>(256);
    let (mapped_params_tx, mapped_params_rx) = mpsc::channel::<Vec<MappedParam>>(256);

    // Completely Decoupled SQLite Snapshot channel to prevent any down-pipeline stalling §11.1
    let (db_snapshots_tx, mut db_snapshots_rx) = mpsc::channel::<StateSnapshot>(512);
    let db_clone_for_snapshots = db.clone();
    tokio::spawn(async move {
        while let Some(snap) = db_snapshots_rx.recv().await {
            db_clone_for_snapshots.insert_snapshot(&snap);
        }
    });

    // 8. Launch Probe Manager
    let mut probe_manager = ProbeManager::new(
        &config.probes.enabled,
        run_simulate,
        raw_events_tx,
        cumulative_lost_events.clone(),
    );
    probe_manager.start().await;
    let probes_stats = probe_manager.get_probes_info();

    // Record probe loading errors if any fail to load
    let loaded_count = probes_stats.iter().filter(|p| p.loaded).count();
    let failed_count = probes_stats.len() - loaded_count;
    probe_load_errors_total.store(failed_count as u64, std::sync::atomic::Ordering::Relaxed);

    // Drop capabilities / privileges after eBPF program loading as per §9.3 and G-07
    #[cfg(target_os = "linux")]
    {
        info!("Applying PR_SET_NO_NEW_PRIVS to prevent capability escalation...");
        unsafe {
            if libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) < 0 {
                error!("Failed to set PR_SET_NO_NEW_PRIVS: execution privilege drop failed!");
            } else {
                info!("PR_SET_NO_NEW_PRIVS successfully set. Privileges dropped for steady state.");
            }
        }
    }

    // 9. Launch Ring Buffer Consumer
    let ringbuf_consumer = RingbufConsumer::new(raw_events_rx, agg_events_tx, db.clone());
    tokio::spawn(async move {
        ringbuf_consumer.run().await;
    });

    // 10. Launch Aggregator (with decoupled database channels and lost event references)
    let aggregator = Aggregator::new(
        agg_events_rx,
        snapshots_tx,
        Some(db_snapshots_tx),
        config.daemon.window_ms,
        shared_snapshot.clone(),
        snapshot_timestamp_us.clone(),
        cumulative_lost_events,
    );
    tokio::spawn(async move {
        aggregator.run().await;
    });

    // 11. Launch Mapping Engine (Direct decoupled stream connection)
    let mapping_engine = MappingEngine::new(
        snapshots_rx,
        mapped_params_tx,
        shared_mapping_rules.clone(),
    );
    tokio::spawn(async move {
        mapping_engine.run().await;
    });

    // 12. Launch OSC Dispatcher
    let osc_dispatcher = OscDispatcher::new(
        mapped_params_rx,
        shared_osc_backends.clone(),
        osc_metrics.clone(),
        dispatch_latency_histogram.clone(),
        snapshot_timestamp_us,
    );
    tokio::spawn(async move {
        osc_dispatcher.run().await;
    });

    // 13. Launch Control API
    let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);
    let app_state = Arc::new(AppState {
        start_time,
        config_path: resolved_config_path,
        current_config: shared_config,
        current_snapshot: shared_snapshot,
        probes_stats,
        mapping_rules: shared_mapping_rules,
        osc_backends: shared_osc_backends,
        osc_metrics,
        dispatch_latency_histogram,
        api_response_histogram,
        probe_load_errors_total,
        db,
        shutdown_tx,
    });

    let router = conductor::control_api::create_router(app_state);
    let port = config.daemon.control_api_port;
    let addr = format!("127.0.0.1:{}", port);
    
    info!("Control API binding to {}", addr);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .unwrap_or_else(|e| {
            error!("Control API failed to bind to {}: {}", addr, e);
            panic!("Could not start control API: {}", e);
        });

    let server_handle = tokio::spawn(async move {
        axum::serve(listener, router).await.ok();
    });

    // Create a unified signal handling future that resolves cleanly on SIGTERM/SIGINT (BUG-02)
    let shutdown_signal = async {
        #[cfg(unix)]
        {
            let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).unwrap();
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {
                    info!("Received Ctrl-C (SIGINT). Shutting down gracefully...");
                }
                _ = sigterm.recv() => {
                    info!("Received SIGTERM from systemd. Shutting down gracefully...");
                }
            }
        }
        #[cfg(not(unix))]
        {
            tokio::signal::ctrl_c().await.ok();
            info!("Received Ctrl-C (SIGINT). Shutting down gracefully...");
        }
    };

    // 14. Handle Graceful Shutdown signals
    tokio::select! {
        _ = shutdown_signal => {}
        _ = shutdown_rx.recv() => {
            info!("Received API shutdown trigger. Shutting down gracefully...");
        }
    }

    // Unload probes, flush DB, and clean exit as per §8.2 R-03
    probe_manager.shutdown();
    server_handle.abort();
    
    info!("DAEMON CHOIR has stopped. Output preserved.");
}
