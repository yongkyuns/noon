use std::{hint::black_box, time::Instant};

use noon_compile::CompiledTrack;
use noon_core::{
    CompositionTimeMap, Property, RateFunction, TrackId, TrackTiming, TrackValues, Vec2,
};
use noon_runtime::TimelineEventScheduler;

fn main() {
    let mut samples = 1_000usize;
    let mut warmups = 100usize;
    let mut totals = vec![1_000usize, 10_000, 100_000];
    let mut active_counts = vec![0usize, 1, 100, 10_000];

    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--samples" => {
                index += 1;
                samples = parse_positive(&args, index, "samples");
            }
            "--warmups" => {
                index += 1;
                warmups = parse_positive(&args, index, "warmups");
            }
            "--groups" => {
                index += 1;
                totals = parse_list(&args, index, "groups");
            }
            "--active" => {
                index += 1;
                active_counts = parse_list_allow_zero(&args, index, "active");
            }
            unknown => panic!("unknown argument: {unknown}"),
        }
        index += 1;
    }

    println!("Noon timeline scheduler steady-forward benchmark");
    println!("warmups={warmups}, samples={samples}");
    println!("total_groups,active_groups,p50_us,p95_us,p99_us,requested_groups");

    for total in totals {
        for &active in &active_counts {
            if active > total {
                continue;
            }
            let result = benchmark_case(total, active, warmups, samples);
            println!(
                "{total},{active},{:.6},{:.6},{:.6},{}",
                result.p50_us, result.p95_us, result.p99_us, result.requested_groups,
            );
        }
    }
}

struct ResultSummary {
    p50_us: f64,
    p95_us: f64,
    p99_us: f64,
    requested_groups: usize,
}

fn benchmark_case(total: usize, active: usize, warmups: usize, samples: usize) -> ResultSummary {
    let tracks = build_tracks(total, active);
    let mut scheduler = TimelineEventScheduler::new(&tracks);
    scheduler.seek(0.0);

    for frame in 0..warmups {
        let time = (frame + 1) as f64 / 60.0;
        black_box(scheduler.advance(time));
    }

    let mut timings = Vec::with_capacity(samples);
    let mut requested_groups = 0;
    for sample in 0..samples {
        let time = (warmups + sample + 1) as f64 / 60.0;
        let started = Instant::now();
        requested_groups = black_box(scheduler.advance(time));
        timings.push(started.elapsed().as_secs_f64() * 1_000_000.0);
    }
    timings.sort_by(f64::total_cmp);

    let stats = scheduler.last_stats();
    assert_eq!(
        stats.events_crossed, 0,
        "steady case crossed an event boundary"
    );
    assert_eq!(stats.active_groups, active);
    assert_eq!(requested_groups, active);

    ResultSummary {
        p50_us: percentile(&timings, 0.50),
        p95_us: percentile(&timings, 0.95),
        p99_us: percentile(&timings, 0.99),
        requested_groups,
    }
}

fn build_tracks(total: usize, active: usize) -> Vec<CompiledTrack> {
    (0..total)
        .map(|index| {
            let start_time = if index < active {
                0.0
            } else {
                100_000.0 + index as f64
            };
            position_track(index as u64, index as u32, start_time, 10_000.0)
        })
        .collect()
}

fn position_track(id: u64, object: u32, start_time: f64, duration: f64) -> CompiledTrack {
    CompiledTrack {
        id: TrackId::new(id),
        object_index: object,
        property: Property::Position,
        values: TrackValues::Vec2 {
            from: Vec2::ZERO,
            to: Vec2::new(1.0, 0.0),
        },
        timing: TrackTiming {
            start_time,
            duration,
            easing: RateFunction::Linear,
        },
        time_map: CompositionTimeMap::default(),
        transform_geometry_plan: None,
        reconciled: false,
    }
}

fn percentile(sorted: &[f64], fraction: f64) -> f64 {
    let rank = (fraction * sorted.len() as f64).ceil() as usize;
    sorted[rank.saturating_sub(1).min(sorted.len() - 1)]
}

fn parse_positive(args: &[String], index: usize, name: &str) -> usize {
    let value = args
        .get(index)
        .unwrap_or_else(|| panic!("missing value for --{name}"));
    let parsed = value
        .parse::<usize>()
        .unwrap_or_else(|_| panic!("--{name} must be a positive integer"));
    assert!(parsed > 0, "--{name} must be a positive integer");
    parsed
}

fn parse_list(args: &[String], index: usize, name: &str) -> Vec<usize> {
    let values = parse_list_allow_zero(args, index, name);
    assert!(
        values.iter().all(|value| *value > 0),
        "--{name} values must be positive"
    );
    values
}

fn parse_list_allow_zero(args: &[String], index: usize, name: &str) -> Vec<usize> {
    let value = args
        .get(index)
        .unwrap_or_else(|| panic!("missing value for --{name}"));
    let values = value
        .split(',')
        .map(|item| {
            item.parse::<usize>()
                .unwrap_or_else(|_| panic!("--{name} must be a comma-separated integer list"))
        })
        .collect::<Vec<_>>();
    assert!(!values.is_empty(), "--{name} must not be empty");
    values
}
