use std::{hint::black_box, time::Instant};

use noon_core::{StrokeCap, StrokeJoin, Vec2, VectorPath};
use noon_geometry::{plan_morph, tessellate_styled_with_fill, MorphOptions};

fn generated_path(seed: usize, point_count: usize, closed: bool) -> VectorPath {
    let phase = seed as f32 * 0.173_205_08;
    let mut points = Vec::with_capacity(point_count);
    for index in 0..point_count {
        let angle = std::f32::consts::TAU * index as f32 / point_count as f32 + phase;
        let harmonic = (angle * 3.0 + phase * 0.37).sin() * 0.22;
        let radius = 1.0 + harmonic + (index as f32 * 0.117 + phase).cos() * 0.08;
        points.push(Vec2::new(angle.cos() * radius, angle.sin() * radius));
    }

    let mut path = VectorPath::new().move_to(points[0]);
    for index in 1..points.len() {
        let previous = points[index - 1];
        let next = points[index];
        let delta = next - previous;
        path = match index % 3 {
            0 => path.cubic_to(previous + delta * 0.3, previous + delta * 0.7, next),
            1 => path.quadratic_to((previous + next) * 0.5, next),
            _ => path.line_to(next),
        };
    }
    if closed {
        path.close()
    } else {
        path
    }
}

fn bench_tessellation(count: usize, point_count: usize) {
    let paths: Vec<_> = (0..count)
        .map(|seed| generated_path(seed, point_count, seed % 2 == 0))
        .collect();
    let start = Instant::now();
    let mut vertices = 0_usize;
    let mut indices = 0_usize;
    for path in &paths {
        let mesh = tessellate_styled_with_fill(
            black_box(path),
            0.06,
            StrokeJoin::Round,
            StrokeCap::Round,
            false,
        )
        .expect("benchmark path should tessellate");
        vertices += mesh.vertices.len();
        indices += mesh.indices.len();
        black_box(&mesh);
    }
    let elapsed = start.elapsed();
    println!(
        "tessellate paths={count:>6} points/path={point_count:>3} elapsed={elapsed:?} vertices={vertices} indices={indices}"
    );
}

fn bench_morph_planning(count: usize, point_count: usize, samples_per_contour: usize) {
    let pairs: Vec<_> = (0..count)
        .map(|seed| {
            (
                generated_path(seed, point_count, true),
                generated_path(seed + 10_000, point_count + 5, true),
            )
        })
        .collect();
    let options = MorphOptions {
        samples_per_contour,
        flatten_tolerance: 0.01,
    };
    let start = Instant::now();
    let mut points = 0_usize;
    for (source, target) in &pairs {
        let plan = plan_morph(black_box(source), black_box(target), options)
            .expect("benchmark morph should plan");
        points += plan.point_count();
        black_box(&plan);
    }
    let elapsed = start.elapsed();
    println!(
        "plan_morph pairs={count:>6} source_points={point_count:>3} samples/contour={samples_per_contour:>3} elapsed={elapsed:?} planned_points={points}"
    );
}

fn main() {
    println!("Noon vector geometry scale benchmark (release mode recommended)");
    for count in [100, 1_000, 10_000] {
        bench_tessellation(count, 32);
    }
    for count in [100, 1_000] {
        bench_morph_planning(count, 32, 128);
    }
}
