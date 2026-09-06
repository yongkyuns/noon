use noon_core::{
    GeometryRef, Property, StrokeWidthMode, Style, TrackDefinition, TrackValues, Transform2D,
    VectorPath,
};
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq)]
pub enum TransformGeometryPlan {
    Static,
    PointwiseRotation,
    Circle {
        from_radius: f32,
        to_radius: f32,
    },
    Rectangle {
        from_size: noon_core::Vec2,
        to_size: noon_core::Vec2,
    },
    Line {
        from_start: noon_core::Vec2,
        from_end: noon_core::Vec2,
        to_start: noon_core::Vec2,
        to_end: noon_core::Vec2,
    },
    PathPair {
        geometry: Arc<GeometryRef>,
        /// Fixed coordinate frame for renderer-only endpoints; semantic TRS stays separate.
        render_transform: Option<Transform2D>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TransformCompileFailure {
    UnsupportedGeometry,
    RequiresRetessellation,
    UnsafeFilledPath,
}

pub(crate) fn compile_transform_geometry_plan(
    track: &TrackDefinition,
) -> Result<Option<TransformGeometryPlan>, TransformCompileFailure> {
    if track.property == Property::Morph {
        return match &track.values {
            TrackValues::PreparedMorph {
                geometry,
                render_transform,
                ..
            } => {
                let GeometryRef::VectorPath(source) = geometry else {
                    return Err(TransformCompileFailure::UnsupportedGeometry);
                };
                let Some(target) = source.morph_target() else {
                    return Err(TransformCompileFailure::UnsupportedGeometry);
                };
                noon_geometry::plan_morph(source, target, noon_geometry::MorphOptions::DEFAULT)
                    .map_err(|_| TransformCompileFailure::UnsupportedGeometry)?;
                noon_geometry::plan_filled_morph(
                    source,
                    target,
                    noon_geometry::MorphOptions::DEFAULT,
                )
                .map_err(|_| TransformCompileFailure::UnsafeFilledPath)?;
                Ok(Some(TransformGeometryPlan::PathPair {
                    geometry: Arc::new(geometry.clone()),
                    render_transform: *render_transform,
                }))
            }
            _ => Ok(None),
        };
    }
    if track.property != Property::Transform {
        return Ok(None);
    }
    let TrackValues::Object { from, to } = &track.values else {
        unreachable!("validated Transform track must contain object snapshots");
    };

    if let (GeometryRef::VectorPath(_), GeometryRef::VectorPath(_)) = (&from.geometry, &to.geometry)
    {
        if path_style_requires_retessellation(from.style, to.style) {
            return Err(TransformCompileFailure::RequiresRetessellation);
        }
    }

    if from.geometry == to.geometry {
        if from.transform.scale == to.transform.scale
            && from.transform.rotation.to_bits() != to.transform.rotation.to_bits()
        {
            return Ok(Some(TransformGeometryPlan::PointwiseRotation));
        }
        return Ok(Some(TransformGeometryPlan::Static));
    }

    let plan = match (&from.geometry, &to.geometry) {
        (
            GeometryRef::Circle {
                radius: from_radius,
            },
            GeometryRef::Circle { radius: to_radius },
        ) => TransformGeometryPlan::Circle {
            from_radius: *from_radius,
            to_radius: *to_radius,
        },
        (GeometryRef::Rectangle { size: from_size }, GeometryRef::Rectangle { size: to_size }) => {
            TransformGeometryPlan::Rectangle {
                from_size: *from_size,
                to_size: *to_size,
            }
        }
        (
            GeometryRef::Line {
                start: from_start,
                end: from_end,
            },
            GeometryRef::Line {
                start: to_start,
                end: to_end,
            },
        ) => TransformGeometryPlan::Line {
            from_start: *from_start,
            from_end: *from_end,
            to_start: *to_start,
            to_end: *to_end,
        },
        (GeometryRef::VectorPath(source), GeometryRef::VectorPath(target)) => compile_path_pair(
            from.style,
            to.style,
            from.transform,
            to.transform,
            source.clone(),
            target.clone(),
        )?,
        (GeometryRef::Circle { .. }, GeometryRef::Rectangle { .. })
        | (GeometryRef::Rectangle { .. }, GeometryRef::Circle { .. }) => {
            let source = noon_geometry::canonical_outline_path(&from.geometry)
                .expect("closed analytic source geometry must convert to a path");
            let target = noon_geometry::canonical_outline_path(&to.geometry)
                .expect("closed analytic target geometry must convert to a path");
            compile_path_pair(
                from.style,
                to.style,
                from.transform,
                to.transform,
                source,
                target,
            )?
        }
        _ => return Err(TransformCompileFailure::UnsupportedGeometry),
    };
    Ok(Some(plan))
}

/// Compile a typed analytic content morph without constructing an authored
/// object endpoint. The returned resource is execution-owned and may use a
/// fixed render frame while semantic affine channels remain independently visible.
pub(crate) fn compile_analytic_content_morph(
    from_geometry: &GeometryRef,
    to_geometry: &GeometryRef,
    from_style: Style,
    to_style: Style,
    from_transform: Transform2D,
    to_transform: Transform2D,
) -> Result<(GeometryRef, Option<Transform2D>), TransformCompileFailure> {
    let supported = matches!(
        (from_geometry, to_geometry),
        (GeometryRef::Circle { .. }, GeometryRef::Rectangle { .. })
            | (GeometryRef::Rectangle { .. }, GeometryRef::Circle { .. })
    );
    if !supported {
        return Err(TransformCompileFailure::UnsupportedGeometry);
    }
    let source = noon_geometry::canonical_outline_path(from_geometry)
        .expect("closed analytic source geometry must convert to a path");
    let target = noon_geometry::canonical_outline_path(to_geometry)
        .expect("closed analytic target geometry must convert to a path");
    let TransformGeometryPlan::PathPair {
        geometry,
        render_transform,
    } = compile_path_pair(
        from_style,
        to_style,
        from_transform,
        to_transform,
        source,
        target,
    )?
    else {
        unreachable!("analytic cross-content morph compiles to a path pair")
    };
    Ok((geometry.as_ref().clone(), render_transform))
}

fn path_style_requires_retessellation(from: Style, to: Style) -> bool {
    from.stroke_width.to_bits() != to.stroke_width.to_bits()
        || from.stroke_join != to.stroke_join
        || from.stroke_cap != to.stroke_cap
        || from.fill.is_some() != to.fill.is_some()
}

fn compile_path_pair(
    from_style: Style,
    to_style: Style,
    from_transform: Transform2D,
    to_transform: Transform2D,
    source: VectorPath,
    target: VectorPath,
) -> Result<TransformGeometryPlan, TransformCompileFailure> {
    if path_style_requires_retessellation(from_style, to_style) {
        return Err(TransformCompileFailure::RequiresRetessellation);
    }
    if from_style.fill.is_some()
        && noon_geometry::plan_filled_morph(&source, &target, noon_geometry::MorphOptions::DEFAULT)
            .is_err()
    {
        return Err(TransformCompileFailure::UnsafeFilledPath);
    }
    // A fixed world frame keeps both stroke tessellation and path resource identity
    // independent of animation progress. Preserve the current-relative lane when
    // interpolation can become singular: later independent TRS drivers may need
    // to invert the semantic transform to take ownership of this render geometry.
    let same_nonzero_sign = |a: f32, b: f32| {
        a.abs() > 1.0e-7 && b.abs() > 1.0e-7 && a.is_sign_positive() == b.is_sign_positive()
    };
    if from_style.stroke_width_mode == StrokeWidthMode::ScreenSpace
        && to_style.stroke_width_mode == StrokeWidthMode::ScreenSpace
        && same_nonzero_sign(from_transform.scale.x, to_transform.scale.x)
        && same_nonzero_sign(from_transform.scale.y, to_transform.scale.y)
    {
        let world_source = source.transformed(from_transform);
        let world_target = target.transformed(to_transform);
        // Overflowed derived points and unsafe world-space filled topology retain
        // the established local plan rather than installing an invalid resource.
        if world_source.is_finite()
            && world_target.is_finite()
            && fixed_frame_inverse_is_finite(
                &world_source,
                &world_target,
                from_transform,
                to_transform,
            )
            && (from_style.fill.is_none()
                || noon_geometry::plan_filled_morph(
                    &world_source,
                    &world_target,
                    noon_geometry::MorphOptions::DEFAULT,
                )
                .is_ok())
        {
            return Ok(TransformGeometryPlan::PathPair {
                geometry: Arc::new(GeometryRef::path(
                    world_source.with_morph_target(world_target),
                )),
                render_transform: Some(Transform2D::IDENTITY),
            });
        }
    }
    Ok(TransformGeometryPlan::PathPair {
        geometry: Arc::new(GeometryRef::path(source.with_morph_target(target))),
        render_transform: None,
    })
}

// Independent drivers can take over a fixed frame only if its conversion back
// to any interpolated semantic TRS is finite. Bound the inverse in f64 so valid
// but extreme authored coordinates retain the original local-plan behavior.
fn fixed_frame_inverse_is_finite(
    source: &VectorPath,
    target: &VectorPath,
    from: Transform2D,
    to: Transform2D,
) -> bool {
    if !(to.rotation - from.rotation).is_finite()
        || !(to.translation.x - from.translation.x).is_finite()
        || !(to.translation.y - from.translation.y).is_finite()
    {
        return false;
    }
    let max_world = [source, target]
        .into_iter()
        .filter_map(VectorPath::conservative_bounds)
        .flat_map(|bounds| [bounds.min.x, bounds.min.y, bounds.max.x, bounds.max.y])
        .map(|value| f64::from(value).abs())
        .fold(0.0_f64, f64::max);
    let max_translation = [
        from.translation.x,
        from.translation.y,
        to.translation.x,
        to.translation.y,
    ]
    .into_iter()
    .map(|value| f64::from(value).abs())
    .fold(0.0_f64, f64::max);
    let min_scale = [from.scale.x, from.scale.y, to.scale.x, to.scale.y]
        .into_iter()
        .map(|value| f64::from(value).abs())
        .fold(f64::INFINITY, f64::min);
    let relative_bound = 4.0 * (max_world + max_translation);
    relative_bound < f64::from(f32::MAX) && relative_bound / min_scale < f64::from(f32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use noon_core::{Color, Vec2};

    #[test]
    fn fixed_world_pair_requires_finite_invertible_driver_takeover() {
        let source = VectorPath::new()
            .move_to(Vec2::ZERO)
            .line_to(Vec2::new(1.0, 0.0));
        let target = VectorPath::new()
            .move_to(Vec2::ZERO)
            .line_to(Vec2::new(0.0, 1.0));
        let style = Style {
            fill: None,
            stroke: Some(Color::WHITE),
            stroke_width_mode: StrokeWidthMode::ScreenSpace,
            ..Style::default()
        };
        for (scale, translation, fixed) in [
            (Vec2::new(2.0, 0.5), Vec2::ZERO, true),
            (Vec2::new(-2.0, 0.5), Vec2::ZERO, false),
            (Vec2::new(0.0, 1.0), Vec2::ZERO, false),
            (Vec2::new(1.0e-8, 1.0), Vec2::ZERO, false),
            (Vec2::ONE, Vec2::new(f32::MAX, 0.0), false),
        ] {
            let plan = compile_path_pair(
                style,
                style,
                Transform2D::IDENTITY,
                Transform2D {
                    scale,
                    translation,
                    ..Transform2D::IDENTITY
                },
                source.clone(),
                target.clone(),
            )
            .unwrap();
            let TransformGeometryPlan::PathPair {
                geometry,
                render_transform,
            } = plan
            else {
                panic!("pair");
            };
            assert_eq!(render_transform.is_some(), fixed);
            assert!(geometry.is_finite());
        }
    }

    #[test]
    fn analytic_morph_prepares_rotated_screen_space_endpoints_in_world_frame() {
        let style = Style {
            stroke: Some(Color::WHITE),
            stroke_width_mode: StrokeWidthMode::ScreenSpace,
            ..Style::default()
        };
        let source_transform = Transform2D {
            rotation: std::f32::consts::FRAC_PI_4,
            ..Transform2D::IDENTITY
        };
        let (geometry, render_transform) = compile_analytic_content_morph(
            &GeometryRef::rectangle(2.0, 2.0),
            &GeometryRef::circle(1.0),
            style,
            style,
            source_transform,
            Transform2D::IDENTITY,
        )
        .unwrap();

        let GeometryRef::VectorPath(path) = geometry else {
            panic!("analytic cross-content morph must compile to a path pair")
        };
        assert!(path.morph_target().is_some());
        assert_eq!(render_transform, Some(Transform2D::IDENTITY));
        assert!(path
            .conservative_bounds()
            .is_some_and(|bounds| bounds.width() > 2.5 && bounds.height() > 2.5));
    }
}
