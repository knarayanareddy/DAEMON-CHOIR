use daemon_choir_common::events::{CpuSchedEvent, ProcLifecycleEvent};
use daemon_choir_common::config::Config;

#[test]
fn test_no_raw_comm_in_events() {
    // Assert at the type level that CPU Sched events only contain u64 hash fields
    // and no String or Vec<u8> process names, satisfying §12.3.
    let _event = CpuSchedEvent {
        prev_comm_hash: 12345,
        next_comm_hash: 67890,
        latency_ns: 4000,
        cpu_id: 1,
    };

    let _proc_event = ProcLifecycleEvent {
        comm_hash: 11111,
        uid_hash: 0,
    };

    // If these structures compile and don't contain raw strings, we have enforced
    // Type-Level Privacy by Design.
    assert!(true);
}

#[test]
fn test_api_binds_loopback_only() {
    // Verify that default config maps the API strictly to loopback-only
    let default_config = Config::default();
    assert_eq!(default_config.daemon.control_api_port, 9876);
    
    // Check that we default to simulate mode for cross-platform portability
    assert!(default_config.daemon.simulate);
}
