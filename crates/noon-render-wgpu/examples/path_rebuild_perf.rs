use std::{
    hint::black_box,
    time::{Duration, Instant},
};

use noon_core::{GeometryRef, ObjectId, Style, Transform2D, Vec2, VectorPath};
use noon_render_wgpu::FramePreparer;
use noon_runtime::{FrameObjectState, FrameState};

fn main() {
    let mut segments = 20_000usize;
    let mut samples = 30usize;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--segments" => segments = args.next().unwrap().parse().unwrap(),
            "--samples" => samples = args.next().unwrap().parse().unwrap(),
            other => panic!("unknown argument: {other}"),
        }
    }
    assert!(segments >= 3 && samples > 0);
    let frame = build_frame(segments);
    let mut preparer = FramePreparer::new();
    black_box(preparer.prepare(&frame));
    let mut timings = Vec::with_capacity(samples);
    for _ in 0..samples {
        let started = Instant::now();
        let prepared = preparer.prepare(black_box(&frame));
        black_box(prepared.path_vertices.len());
        black_box(prepared.path_indices.len());
        timings.push(started.elapsed());
    }
    timings.sort_unstable();
    println!(
        "segments={segments},samples={samples},p50_ms={:.6},p95_ms={:.6},p99_ms={:.6}",
        ms(percentile(&timings, 0.50)),
        ms(percentile(&timings, 0.95)),
        ms(percentile(&timings, 0.99))
    );
}

fn build_frame(segments: usize) -> FrameState {
    let mut path = VectorPath::new().move_to(Vec2::new(1.0, 0.0));
    for index in 1..segments {
        let angle = std::f32::consts::TAU * index as f32 / segments as f32;
        let radius = 1.0 + 0.08 * (angle * 7.0).sin();
        path = path.line_to(Vec2::new(radius * angle.cos(), radius * angle.sin()));
    }
    path = path.close();
    FrameState {
        time: 0.0,
        objects: vec![FrameObjectState {
            id: ObjectId::new(1),
            content: noon_core::ObjectContentRef::Geometry(GeometryRef::path(path)),
            text_bounds: None,
            transform: Transform2D::IDENTITY,
            style: Style::default(),
            appearance: 1.0,
        }],
        presences: vec![true],
        reveals: vec![1.0],
        morphs: vec![0.0],
        render_geometries: vec![None],
        render_transforms: vec![None],
    }
}

fn percentile(sorted: &[Duration], fraction: f64) -> Duration {
    let rank = (fraction * sorted.len() as f64).ceil() as usize;
    sorted[rank.saturating_sub(1).min(sorted.len() - 1)]
}

fn ms(value: Duration) -> f64 {
    value.as_secs_f64() * 1000.0
}
