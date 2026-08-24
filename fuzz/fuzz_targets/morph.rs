#![no_main]

use libfuzzer_sys::fuzz_target;
use noon_core::{Vec2, VectorPath};
use noon_geometry::{plan_morph, MorphOptions};

const MAX_POINTS: usize = 48;

fn scalar(lo: u8, hi: u8) -> f32 {
    let raw = i16::from_le_bytes([lo, hi]);
    f32::from(raw) / 4096.0
}

fn polygon(data: &[u8]) -> VectorPath {
    let points = (data.len() / 4).min(MAX_POINTS);
    if points == 0 {
        return VectorPath::new();
    }
    let to_point = |index: usize| {
        let base = index * 4;
        Vec2::new(
            scalar(data[base], data[base + 1]),
            scalar(data[base + 2], data[base + 3]),
        )
    };
    let mut path = VectorPath::new().move_to(to_point(0));
    for index in 1..points {
        path = path.line_to(to_point(index));
    }
    if data.first().copied().unwrap_or_default() & 1 != 0 {
        path = path.close();
    }
    path
}

fuzz_target!(|data: &[u8]| {
    if data.len() > 4096 {
        return;
    }
    let split = data.len() / 2;
    let source = polygon(&data[..split]);
    let target = polygon(&data[split..]);
    let samples = 3 + usize::from(data.first().copied().unwrap_or_default() % 126);
    let tolerance = 0.0001 + f32::from(data.get(1).copied().unwrap_or_default()) / 2550.0;
    let options = MorphOptions {
        samples_per_contour: samples,
        flatten_tolerance: tolerance,
    };

    if let Ok(plan) = plan_morph(&source, &target, options) {
        assert!(plan.point_count() <= MAX_POINTS * samples);
        for progress in [f32::NAN, -1.0, 0.0, 0.25, 0.5, 1.0, 2.0] {
            let frame = plan.interpolate(progress);
            assert!(frame.contours.iter().all(|contour| contour.points.iter().all(|point| {
                point.x.is_finite() && point.y.is_finite()
            })));
        }
    }
});
