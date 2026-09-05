use std::{
    hint::black_box,
    mem::size_of,
    time::{Duration, Instant},
};

use noon_core::{GeometryRef, ObjectId, Style, Transform2D, Vec2};
use noon_render_wgpu::{CircleInstance, FramePreparer, RenderOrderKey};
use noon_runtime::{FrameChanges, FrameObjectState, FrameState};

const DEFAULT_SIZES: [usize; 3] = [1_000, 10_000, 100_000];
const DEFAULT_WARMUPS: usize = 10;
const DEFAULT_SAMPLES: usize = 100;

#[derive(Clone, Copy)]
struct Config {
    warmups: usize,
    samples: usize,
}

#[derive(Clone, Copy)]
struct Timing {
    median: Duration,
    p95: Duration,
    p99: Duration,
}

fn main() {
    let (config, sizes) = parse_args();
    println!(
        "Noon frame preparation benchmark ({} warmups, {} samples)",
        config.warmups, config.samples
    );
    println!();
    println!(
        "| Objects | Operation | Median | p95 | p99 | Repacked | Upload bytes | Full / operation |"
    );
    println!("|---:|---|---:|---:|---:|---:|---:|---:|");
    for object_count in sizes {
        benchmark_size(object_count, config);
    }
}

fn benchmark_size(object_count: usize, config: Config) {
    let mut frame = build_frame(object_count);

    let mut full_preparer = FramePreparer::new();
    full_preparer.prepare(&frame);
    let full = measure(config, |_| {
        let prepared = full_preparer.prepare(black_box(&frame));
        black_box(prepared.stats.instances_repacked);
    });

    let mut keyed_preparer = FramePreparer::new();
    let keyed_order = (0..object_count)
        .map(|index| RenderOrderKey::new((index % 7) as i32, index as u64))
        .collect::<Vec<_>>();
    keyed_preparer
        .set_render_order_keys(&frame, &keyed_order)
        .expect("benchmark render-order key count must match frame");
    keyed_preparer.prepare(&frame);
    let explicit_order = measure(config, |_| {
        let prepared = keyed_preparer.prepare(black_box(&frame));
        black_box(prepared.stats.instances_repacked);
    });

    let mut static_preparer = FramePreparer::new();
    static_preparer.prepare(&frame);
    let static_changes = FrameChanges::default();
    let unchanged = measure(config, |_| {
        let prepared = static_preparer.prepare_incremental(black_box(&frame), &static_changes);
        black_box(prepared.stats.instances_repacked);
    });

    let mut dirty_preparer = FramePreparer::new();
    dirty_preparer.prepare(&frame);
    let target = object_count / 2;
    let dirty_changes = FrameChanges::objects(vec![target]);
    let one_changed = measure(config, |iteration| {
        frame.objects[target].transform.translation.x = iteration as f32;
        let prepared = dirty_preparer.prepare_incremental(black_box(&frame), &dirty_changes);
        black_box(prepared.stats.instances_repacked);
    });

    print_row(
        object_count,
        "full rebuild / default order",
        full,
        object_count,
        object_count * size_of::<CircleInstance>(),
        full,
    );
    print_row(
        object_count,
        "full rebuild / explicit z order",
        explicit_order,
        object_count,
        object_count * size_of::<CircleInstance>(),
        full,
    );
    print_row(object_count, "unchanged", unchanged, 0, 0, full);
    print_row(
        object_count,
        "one changed",
        one_changed,
        1,
        size_of::<CircleInstance>(),
        full,
    );
}

fn build_frame(object_count: usize) -> FrameState {
    assert!(object_count > 0, "benchmark sizes must be positive");
    FrameState {
        time: 0.0,
        objects: (0..object_count)
            .map(|index| FrameObjectState {
                id: ObjectId::new(index as u64),
                content: noon_core::ObjectContentRef::Geometry(GeometryRef::circle(0.5)),
                text_bounds: None,
                transform: Transform2D {
                    translation: Vec2::new(index as f32, 0.0),
                    ..Transform2D::IDENTITY
                },
                style: Style::default(),
                appearance: 1.0,
            })
            .collect(),
        presences: vec![true; object_count],
        reveals: vec![1.0; object_count],
        morphs: vec![0.0; object_count],
        render_geometries: vec![None; object_count],
        render_transforms: vec![None; object_count],
    }
}

fn measure(config: Config, mut operation: impl FnMut(usize)) -> Timing {
    for iteration in 0..config.warmups {
        operation(iteration);
    }
    let mut durations = Vec::with_capacity(config.samples);
    for sample in 0..config.samples {
        let started = Instant::now();
        operation(config.warmups + sample);
        durations.push(started.elapsed());
    }
    durations.sort_unstable();
    Timing {
        median: percentile(&durations, 0.50),
        p95: percentile(&durations, 0.95),
        p99: percentile(&durations, 0.99),
    }
}

fn percentile(sorted: &[Duration], percentile: f64) -> Duration {
    let rank = (percentile * sorted.len() as f64).ceil() as usize;
    sorted[rank.saturating_sub(1).min(sorted.len() - 1)]
}

fn print_row(
    object_count: usize,
    operation: &str,
    timing: Timing,
    repacked: usize,
    upload_bytes: usize,
    full: Timing,
) {
    let speedup = full.median.as_secs_f64() / timing.median.as_secs_f64();
    println!(
        "| {object_count} | {operation} | {:.6} ms | {:.6} ms | {:.6} ms | {repacked} | {upload_bytes} | {speedup:.1}x |",
        milliseconds(timing.median),
        milliseconds(timing.p95),
        milliseconds(timing.p99),
    );
}

fn milliseconds(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
}

fn parse_args() -> (Config, Vec<usize>) {
    let mut config = Config {
        warmups: DEFAULT_WARMUPS,
        samples: DEFAULT_SAMPLES,
    };
    let mut sizes = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--warmups" => config.warmups = parse_positive("warmups", args.next()),
            "--samples" => config.samples = parse_positive("samples", args.next()),
            _ => sizes.push(
                argument
                    .parse()
                    .ok()
                    .filter(|value| *value > 0)
                    .unwrap_or_else(|| panic!("object count must be positive, got {argument}")),
            ),
        }
    }
    if sizes.is_empty() {
        sizes.extend(DEFAULT_SIZES);
    }
    (config, sizes)
}

fn parse_positive(name: &str, value: Option<String>) -> usize {
    let value = value.unwrap_or_else(|| panic!("--{name} requires a value"));
    value
        .parse()
        .ok()
        .filter(|parsed| *parsed > 0)
        .unwrap_or_else(|| panic!("{name} must be a positive integer, got {value}"))
}
