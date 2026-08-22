use noon_core::{StrokeCap, StrokeJoin, Vec2, VectorPath};
use noon_geometry::{plan_morph, tessellate_styled_with_fill, MorphOptions};

#[derive(Clone, Copy)]
struct Lcg(u64);

impl Lcg {
    fn new(seed: u64) -> Self {
        Self(seed)
    }

    fn next_u32(&mut self) -> u32 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 32) as u32
    }

    fn unit(&mut self) -> f32 {
        self.next_u32() as f32 / u32::MAX as f32
    }

    fn signed(&mut self) -> f32 {
        self.unit() * 2.0 - 1.0
    }
}

fn generated_path(seed: u64, point_count: usize, closed: bool) -> VectorPath {
    assert!(point_count >= 4);
    let mut rng = Lcg::new(seed);
    let mut points = Vec::with_capacity(point_count);
    for index in 0..point_count {
        let angle = std::f32::consts::TAU * index as f32 / point_count as f32;
        let radius = 0.75 + rng.unit() * 1.75;
        points.push(Vec2::new(
            angle.cos() * radius + rng.signed() * 0.05,
            angle.sin() * radius + rng.signed() * 0.05,
        ));
    }

    let mut path = VectorPath::new().move_to(points[0]);
    for index in 1..points.len() {
        let previous = points[index - 1];
        let next = points[index];
        path = match index % 3 {
            0 => {
                let delta = next - previous;
                path.cubic_to(
                    previous + delta * (0.33 + rng.signed() * 0.08),
                    previous + delta * (0.66 + rng.signed() * 0.08),
                    next,
                )
            }
            1 => path.quadratic_to(
                (previous + next) * 0.5 + Vec2::new(rng.signed(), rng.signed()) * 0.15,
                next,
            ),
            _ => path.line_to(next),
        };
    }
    if closed {
        path.close()
    } else {
        path
    }
}

fn assert_finite(point: Vec2) {
    assert!(point.x.is_finite(), "non-finite x coordinate: {}", point.x);
    assert!(point.y.is_finite(), "non-finite y coordinate: {}", point.y);
}

#[test]
fn generated_paths_tessellate_deterministically_and_remain_index_safe() {
    for seed in 0..128_u64 {
        let point_count = 8 + (seed as usize % 41);
        let path = generated_path(seed ^ 0x5eed_1234, point_count, seed % 2 == 0);
        let first = tessellate_styled_with_fill(
            &path,
            0.08,
            StrokeJoin::Round,
            StrokeCap::Round,
            false,
        )
        .expect("generated finite path should tessellate");
        let second = tessellate_styled_with_fill(
            &path,
            0.08,
            StrokeJoin::Round,
            StrokeCap::Round,
            false,
        )
        .expect("same generated path should tessellate twice");

        assert_eq!(first, second, "tessellation changed for seed {seed}");
        assert!(!first.vertices.is_empty(), "seed {seed} produced no vertices");
        assert!(!first.indices.is_empty(), "seed {seed} produced no indices");
        for vertex in &first.vertices {
            assert_finite(vertex.position);
            assert_finite(vertex.target_position);
            assert!(vertex.path_distance.is_finite());
            assert!((0.0..=1.0).contains(&vertex.path_progress));
        }
        for &index in &first.indices {
            assert!(
                (index as usize) < first.vertices.len(),
                "seed {seed} produced out-of-range index {index} for {} vertices",
                first.vertices.len()
            );
        }
        if let Some(bounds) = first.bounds {
            assert_finite(bounds.min);
            assert_finite(bounds.max);
            assert!(bounds.min.x <= bounds.max.x);
            assert!(bounds.min.y <= bounds.max.y);
        }
    }
}

#[test]
fn generated_morph_plans_keep_exact_endpoints_and_finite_intermediates() {
    let options = MorphOptions {
        samples_per_contour: 96,
        flatten_tolerance: 0.01,
    };

    for seed in 0..64_u64 {
        let point_count = 7 + (seed as usize % 19);
        let closed = seed % 2 == 0;
        let source = generated_path(seed ^ 0xa11c_e001, point_count, closed);
        let target = generated_path(seed ^ 0xb22d_f002, point_count + 3, closed);
        let plan = plan_morph(&source, &target, options).expect("compatible paths should plan");

        assert_eq!(plan.contours.len(), 1);
        assert_eq!(plan.point_count(), options.samples_per_contour);
        let contour = &plan.contours[0];
        assert_eq!(contour.source_points.len(), contour.target_points.len());

        let start = plan.interpolate(0.0);
        let end = plan.interpolate(1.0);
        assert_eq!(start.contours[0].points, contour.source_points);
        assert_eq!(end.contours[0].points, contour.target_points);

        for step in 0..=32 {
            let progress = step as f32 / 32.0;
            let frame = plan.interpolate(progress);
            assert_eq!(frame.contours.len(), 1);
            assert_eq!(frame.contours[0].points.len(), options.samples_per_contour);
            for ((point, source), target) in frame.contours[0]
                .points
                .iter()
                .zip(&contour.source_points)
                .zip(&contour.target_points)
            {
                assert_finite(*point);
                let expected = *source * (1.0 - progress) + *target * progress;
                assert!((point.x - expected.x).abs() <= 1.0e-6);
                assert!((point.y - expected.y).abs() <= 1.0e-6);
            }
        }
    }
}

#[test]
fn reveal_length_is_monotonic_and_clamped_across_generated_paths() {
    for seed in 0..32_u64 {
        let path = generated_path(seed ^ 0xc33e_7003, 32, false);
        let mesh = tessellate_styled_with_fill(
            &path,
            0.1,
            StrokeJoin::Round,
            StrokeCap::Round,
            false,
        )
        .expect("generated path should tessellate");
        assert!(mesh.stroke_length > 0.0);

        let mut previous = 0.0;
        for step in 0..=256 {
            let reveal = -0.25 + 1.5 * step as f32 / 256.0;
            let length = mesh.revealed_stroke_length(reveal);
            assert!(length + 1.0e-6 >= previous);
            assert!(length >= 0.0);
            assert!(length <= mesh.stroke_length + 1.0e-6);
            previous = length;
        }
        assert_eq!(mesh.revealed_stroke_length(-10.0), 0.0);
        assert_eq!(mesh.revealed_stroke_length(10.0), mesh.stroke_length);
        assert_eq!(mesh.revealed_stroke_length(f32::NAN), 0.0);
    }
}

#[test]
fn malformed_paths_fail_without_emitting_partial_nonfinite_geometry() {
    let malformed = [
        VectorPath::new().line_to(Vec2::new(1.0, 0.0)),
        VectorPath::new().quadratic_to(Vec2::ZERO, Vec2::new(1.0, 0.0)),
        VectorPath::new().cubic_to(Vec2::ZERO, Vec2::ONE, Vec2::new(2.0, 0.0)),
        VectorPath::new().close(),
        VectorPath::new()
            .move_to(Vec2::ZERO)
            .line_to(Vec2::new(f32::NAN, 1.0)),
        VectorPath::new()
            .move_to(Vec2::ZERO)
            .quadratic_to(Vec2::new(f32::INFINITY, 0.0), Vec2::ONE),
    ];

    for path in malformed {
        assert!(
            tessellate_styled_with_fill(
                &path,
                0.1,
                StrokeJoin::Round,
                StrokeCap::Round,
                false,
            )
            .is_err(),
            "malformed path unexpectedly tessellated: {path:?}"
        );
    }
}
