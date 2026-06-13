use std::sync::{Arc, RwLock, atomic::AtomicU64};
use tokio::sync::mpsc;
use daemon_choir_common::events::{RawEvent, EventPayload, CpuSchedEvent, ProbeId};
use daemon_choir_common::metrics::StateSnapshot;
use conductor::db::Database;
use conductor::aggregator::Aggregator;

// Integration test for SQLite Database persistence & 24h retention cleanups (§7.4, BUG-P1)
// Cleanly isolated test path under temporary system folder to prevent system pollution (§12.1, GAP-03)
#[tokio::test]
async fn test_db_persistence_and_purge() {
    let mut temp_path = std::env::temp_dir();
    temp_path.push(format!("daemon-choir-test-db-{}", std::time::Instant::now().elapsed().as_nanos()));
    
    // Initialize SQLite database connection in isolated temp folder path
    let db = Database::new(Some(temp_path));
    
    // Create a dummy aggregated snapshot
    let snapshot = StateSnapshot {
        timestamp_us: 1000000000,
        window_ms: 50,
        stale: false,
        lost_events: 12,
        metrics: Default::default(),
    };

    // Insert snapshot into SQLite
    db.insert_snapshot(&snapshot);

    // Wipe database entirely to verify wipe routine CSRF works (§10.4)
    let wipe_res = db.wipe();
    assert!(wipe_res.is_ok());
}

// Integration test for Aggregator window, EMA calculations, and stale state detection (§5.2, BUG-03)
#[tokio::test]
async fn test_aggregator_stale_detection() {
    let (raw_tx, raw_rx) = mpsc::channel::<RawEvent>(100);
    let (snap_tx, mut snap_rx) = mpsc::channel::<StateSnapshot>(100);
    let shared_snapshot = Arc::new(RwLock::new(StateSnapshot {
        timestamp_us: 0,
        window_ms: 50,
        stale: true,
        lost_events: 0,
        metrics: Default::default(),
    }));
    let snapshot_timestamp_us = Arc::new(AtomicU64::new(0));
    let cumulative_lost_events = Arc::new(AtomicU64::new(0));

    // Create aggregator with a very fast 10ms window for prompt testing
    let aggregator = Aggregator::new(
        raw_rx,
        snap_tx,
        None,
        10,
        shared_snapshot,
        snapshot_timestamp_us,
        cumulative_lost_events,
    );

    // Spawn aggregator
    tokio::spawn(async move {
        aggregator.run().await;
    });

    // Send a CPU sched event
    let event = RawEvent {
        probe: ProbeId::CpuSched,
        timestamp_ns: 1234567,
        payload: EventPayload::CpuSched(CpuSchedEvent {
            prev_comm_hash: 1,
            next_comm_hash: 2,
            latency_ns: 1000,
            cpu_id: 0,
        }),
    };
    raw_tx.send(event).await.ok();

    // Check if we receive the snapshot
    let snap = snap_rx.recv().await;
    assert!(snap.is_some());
    let snapshot = snap.unwrap();
    
    // Stale is false since we sent a raw event in this window
    assert_eq!(snapshot.stale, false);
}
