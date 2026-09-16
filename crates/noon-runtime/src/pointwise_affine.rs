//! Evaluate correlated affine channels through the ordinary runtime scheduler.
use noon_core::{PointwiseAffineEndpoints, Vec2};

pub(super) fn components(endpoints: PointwiseAffineEndpoints, progress: f32) -> (f32, Vec2) {
    let from = endpoints.from();
    let to = endpoints.to();
    if progress <= 0.0 {
        return (from.rotation, from.scale);
    }
    if progress >= 1.0 {
        return (to.rotation, to.scale);
    }
    let alpha = f64::from(progress);
    let mix = |a: f64, b: f64| a * (1.0 - alpha) + b * alpha;
    let (s0, c0) = f64::from(from.rotation).sin_cos();
    let (s1, c1) = f64::from(to.rotation).sin_cos();
    // Interpolate the transformed basis vectors, not their TRS parameters.
    let x = [
        mix(c0 * f64::from(from.scale.x), c1 * f64::from(to.scale.x)),
        mix(s0 * f64::from(from.scale.x), s1 * f64::from(to.scale.x)),
    ];
    let y = [
        mix(-s0 * f64::from(from.scale.y), -s1 * f64::from(to.scale.y)),
        mix(c0 * f64::from(from.scale.y), c1 * f64::from(to.scale.y)),
    ];
    let x_len = x[0].hypot(x[1]);
    let y_len = y[0].hypot(y[1]);
    let determinant = x[0] * y[1] - x[1] * y[0];
    // The checked payload guarantees orthogonal columns. Use the longer column
    // when decomposing to avoid division by a collapsing or zero-length axis.
    if x_len >= y_len && x_len > 0.0 {
        (
            x[1].atan2(x[0]) as f32,
            Vec2::new(x_len as f32, (determinant / x_len) as f32),
        )
    } else if y_len > 0.0 {
        (
            (-y[0]).atan2(y[1]) as f32,
            Vec2::new((determinant / y_len) as f32, y_len as f32),
        )
    } else {
        (from.rotation, Vec2::ZERO)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use noon_core::Transform2D;

    #[test]
    fn basis_interpolation_preserves_corners_including_collapsed_and_reflected_endpoints() {
        for (source_scale, target_scale) in [
            (Vec2::ONE, Vec2::new(0.75, 0.75)),
            (Vec2::new(2.0, 0.5), Vec2::new(1.5, 0.375)),
            (Vec2::new(-2.0, 0.5), Vec2::new(-1.5, 0.375)),
            (Vec2::ZERO, Vec2::new(2.0, 1.0)),
            (Vec2::new(0.0, 2.0), Vec2::new(0.0, 1.0)),
        ] {
            for angle in [-3.0, -0.8, 0.0, 0.8, 3.0] {
                let from = Transform2D {
                    scale: source_scale,
                    ..Transform2D::IDENTITY
                };
                let to = Transform2D {
                    rotation: angle,
                    scale: target_scale,
                    ..from
                };
                let endpoints = PointwiseAffineEndpoints::new(from, to).unwrap();
                assert_eq!(components(endpoints, 0.0), (from.rotation, from.scale));
                assert_eq!(components(endpoints, 1.0), (to.rotation, to.scale));
                for alpha in [0.125, 0.25, 0.5, 0.75, 0.875] {
                    let (rotation, scale) = components(endpoints, alpha);
                    let transform = Transform2D {
                        rotation,
                        scale,
                        ..from
                    };
                    for point in [
                        Vec2::new(-1.0, -1.0),
                        Vec2::new(-1.0, 1.0),
                        Vec2::new(1.0, -1.0),
                        Vec2::new(1.0, 1.0),
                    ] {
                        let a = from.transform_point(point);
                        let b = to.transform_point(point);
                        let expected = a + (b - a) * alpha;
                        let actual = transform.transform_point(point);
                        assert!(
                            (actual - expected).length() < 2e-6,
                            "from={from:?}, to={to:?}, alpha={alpha}: {actual:?} != {expected:?}"
                        );
                    }
                }
            }
        }
    }
}
