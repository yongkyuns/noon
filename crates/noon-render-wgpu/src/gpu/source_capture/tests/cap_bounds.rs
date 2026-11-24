use super::*;
#[test]
fn square_line_capture_bounds_contain_cap_corners_in_both_width_modes() {
    use noon_core::StrokeWidthMode;
    let mut profiles = 0;
    let mut old_omissions = 0;
    for mode in [
        StrokeWidthMode::ScaleWithObject,
        StrokeWidthMode::ScreenSpace,
    ] {
        for angle in [0.0_f32, std::f32::consts::FRAC_PI_4, 1.3] {
            for scale in [
                Vec2::new(1.0, 1.0),
                Vec2::new(0.2, 2.0),
                Vec2::new(-2.0, 0.3),
            ] {
                for rotation in [0.0, 0.3, 1.1] {
                    let (sin, cos) = angle.sin_cos();
                    let tangent = Vec2::new(cos, sin);
                    let a = tangent * -0.3;
                    let b = tangent * 0.3;
                    let transform = Transform2D {
                        translation: Vec2::new(1.2, -0.8),
                        scale,
                        rotation,
                    };
                    let style = Style {
                        fill: None,
                        stroke: Some(Color::WHITE),
                        stroke_width: 2.0,
                        stroke_width_mode: mode,
                        ..Style::default()
                    };
                    let mut packed: PackedStyle = style.into();
                    packed.stroke_enabled |= 2 << 2;
                    let pad = stroke_pad(packed, transform.into());
                    let bounds = |pad| {
                        transformed(
                            Rect::new(
                                Vec2::new(a.x.min(b.x) - pad, a.y.min(b.y) - pad),
                                Vec2::new(a.x.max(b.x) + pad, a.y.max(b.y) + pad),
                            ),
                            transform.into(),
                        )
                        .unwrap()
                    };
                    let actual = bounds(pad);
                    let old = bounds(if mode == StrokeWidthMode::ScreenSpace {
                        1.0 / scale.x.abs().min(scale.y.abs())
                    } else {
                        1.0
                    });
                    let (start, end, unit) = if mode == StrokeWidthMode::ScreenSpace {
                        let a = transform.transform_point(a);
                        let b = transform.transform_point(b);
                        let delta = b - a;
                        let length = delta.x.hypot(delta.y);
                        (a, b, delta * (1.0 / length))
                    } else {
                        (a, b, tangent)
                    };
                    let normal = Vec2::new(-unit.y, unit.x);
                    for (endpoint, sign) in [(start, -1.0), (end, 1.0)] {
                        for side in [-1.0, 1.0] {
                            // Independent geometric square-cap corners: one half
                            // stroke in both tangent and normal directions.
                            let corner = endpoint + unit * sign + normal * side;
                            let p = if mode == StrokeWidthMode::ScreenSpace {
                                corner
                            } else {
                                transform.transform_point(corner)
                            };
                            assert!(
                                p.x >= actual.min.x - 1e-5
                                    && p.x <= actual.max.x + 1e-5
                                    && p.y >= actual.min.y - 1e-5
                                    && p.y <= actual.max.y + 1e-5,
                                "profile {profiles}: {p:?} outside {actual:?}"
                            );
                            if p.x < old.min.x - 1e-5
                                || p.x > old.max.x + 1e-5
                                || p.y < old.min.y - 1e-5
                                || p.y > old.max.y + 1e-5
                            {
                                old_omissions += 1;
                            }
                        }
                    }
                    profiles += 1;
                }
            }
        }
    }
    assert_eq!(profiles, 54);
    assert!(
        old_omissions > 0,
        "old capture bounds must omit real square-cap corners"
    );
}
