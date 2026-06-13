use criterion::{criterion_group, criterion_main, Criterion};
use daemon_choir_common::config::MappingRule;

fn bench_mapping_transform(c: &mut Criterion) {
    let rule = MappingRule {
        source_metric: "cpu.sched.latency_p95".to_string(),
        osc_address: "/choir/voice/0/frequency".to_string(),
        transform: "exponential".to_string(),
        input_range: [0.0, 1.0],
        output_range: [220.0, 880.0],
    };

    c.bench_function("apply_transform_exponential", |b| {
        b.iter(|| {
            let _val = apply_transform(
                0.42,
                &rule.transform,
                rule.input_range[0],
                rule.input_range[1],
                rule.output_range[0],
                rule.output_range[1],
            );
        })
    });
}

fn apply_transform(
    x: f64,
    transform: &str,
    in_min: f64,
    in_max: f64,
    out_min: f64,
    out_max: f64,
) -> f64 {
    if in_min >= in_max {
        return out_min;
    }
    let clamped_input = x.clamp(in_min, in_max);
    let ratio = (clamped_input - in_min) / (in_max - in_min);

    match transform {
        "exponential" => {
            if out_min > 0.0 && out_max > 0.0 {
                out_min * (out_max / out_min).powf(ratio)
            } else {
                out_min + (out_max - out_min) * ratio.powi(2)
            }
        }
        "logarithmic" => {
            let log_ratio = (1.0 + ratio * 9.0).ln() / 10.0f64.ln();
            out_min + (out_max - out_min) * log_ratio
        }
        "step" => {
            let steps = 5.0;
            let stepped_ratio = (ratio * steps).floor() / steps;
            out_min + (out_max - out_min) * stepped_ratio
        }
        "linear" | _ => {
            out_min + (out_max - out_min) * ratio
        }
    }
}

criterion_group!(benches, bench_mapping_transform);
criterion_main!(benches);
