use criterion::{criterion_group, criterion_main, Criterion};
use daemon_choir_common::events::{RawEvent, EventPayload, CpuSchedEvent, ProbeId};

fn bench_aggregation(c: &mut Criterion) {
    let mut events = Vec::new();
    for i in 0..1000 {
        events.push(RawEvent {
            probe: ProbeId::CpuSched,
            timestamp_ns: 123456789 + i,
            payload: EventPayload::CpuSched(CpuSchedEvent {
                prev_comm_hash: 1111,
                next_comm_hash: 2222,
                latency_ns: 3000 + (i % 2000),
                cpu_id: (i % 8) as u32,
            }),
        });
    }

    c.bench_function("aggregate_1000_raw_events", |b| {
        b.iter(|| {
            let mut cpu_latencies: Vec<u64> = Vec::new();
            let mut context_switches = 0;

            for event in &events {
                if let EventPayload::CpuSched(e) = &event.payload {
                    cpu_latencies.push(e.latency_ns);
                    context_switches += 1;
                }
            }

            if !cpu_latencies.is_empty() {
                cpu_latencies.sort_unstable();
                let idx = (cpu_latencies.len() as f64 * 0.95).floor() as usize;
                let _p95 = cpu_latencies[idx.min(cpu_latencies.len() - 1)];
            }
            
            let _switches = context_switches;
        })
    });
}

criterion_group!(benches, bench_aggregation);
criterion_main!(benches);
