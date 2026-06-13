use std::sync::{Arc, RwLock, atomic::AtomicU64};
use std::time::Instant;
use std::path::PathBuf;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use tower::ServiceExt; // for oneshot
use serde_json::Value;

use daemon_choir_common::config::Config;
use daemon_choir_common::metrics::StateSnapshot;
use conductor::control_api::{create_router, AppState};
use conductor::osc::OscMetrics;
use conductor::db::Database;

fn setup_test_app_state() -> Arc<AppState> {
    let start_time = Instant::now();
    let config = Config::default();
    
    let shared_config = Arc::new(RwLock::new(config.clone()));
    let shared_snapshot = Arc::new(RwLock::new(StateSnapshot {
        timestamp_us: 100000,
        window_ms: config.daemon.window_ms,
        stale: false,
        lost_events: 0,
        metrics: Default::default(),
    }));
    let shared_mapping_rules = Arc::new(RwLock::new(config.mapping.clone()));
    let shared_osc_backends = Arc::new(RwLock::new(config.backends.osc.clone()));
    let osc_metrics = OscMetrics::default();

    let dispatch_latency_histogram = conductor::osc::PrometheusHistogram::default();
    let api_response_histogram = conductor::osc::PrometheusHistogram::default();
    let probe_load_errors_total = Arc::new(AtomicU64::new(0));

    // Isolated temporary database path
    let mut temp_path = std::env::temp_dir();
    temp_path.push(format!("daemon-choir-test-api-{}", Instant::now().elapsed().as_nanos()));
    let db = Arc::new(Database::new(Some(temp_path)));

    let (shutdown_tx, _) = tokio::sync::mpsc::channel(1);

    Arc::new(AppState {
        start_time,
        config_path: PathBuf::from("test.toml"),
        current_config: shared_config,
        current_snapshot: shared_snapshot,
        probes_stats: vec![],
        mapping_rules: shared_mapping_rules,
        osc_backends: shared_osc_backends,
        osc_metrics,
        dispatch_latency_histogram,
        api_response_histogram,
        probe_load_errors_total,
        db,
        shutdown_tx,
    })
}

#[tokio::test]
async fn test_route_status() {
    let app_state = setup_test_app_state();
    let router = create_router(app_state);

    let response = router
        .oneshot(
            Request::builder()
                .uri("/v1/status")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_route_state() {
    let app_state = setup_test_app_state();
    let router = create_router(app_state);

    let response = router
        .oneshot(
            Request::builder()
                .uri("/v1/state")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_route_voices() {
    let app_state = setup_test_app_state();
    let router = create_router(app_state);

    let response = router
        .oneshot(
            Request::builder()
                .uri("/v1/voices")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_route_shutdown_unauthorized() {
    let app_state = setup_test_app_state();
    let router = create_router(app_state);

    // Call /v1/shutdown without CSRF guard header -> expect 403 Forbidden (§12.1, BUG-P1)
    let response = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/shutdown")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}
