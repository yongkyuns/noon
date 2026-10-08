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
        /// Shape frame with independently sampled translation; semantic TRS stays separate.
        render_frame: Option<noon_core::MorphRenderFrame>,
        /// Prepared once in source semantic coordinates for earlier playback.
        /// Only fixed-frame PreparedMorph plans need this additional geometry.
        prestart_geometry: Option<Arc<GeometryRef>>,
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
    compile_transform_geometry_values(track.property, &track.values)
}

pub(crate) fn compile_transform_geometry_values(
    property: Property,
    values: &TrackValues,
) -> Result<Option<TransformGeometryPlan>, TransformCompileFailure> {
    if property == Property::Morph {
        return match values {
            TrackValues::PreparedMorph {
                geometry,
                render_frame,
                source_transform,
                ..
            } => {
                let GeometryRef::VectorPath(source) = geometry else {
                    return Err(TransformCompileFailure::UnsupportedGeometry);
                };
                let Some(target) = source.morph_target() else {
                    return Err(TransformCompileFailure::UnsupportedGeometry);
                };
                noon_geometry::plan_morph_preserving_order(
                    source,
                    target,
                    noon_geometry::MorphOptions::DEFAULT,
                )
                .map_err(|_| TransformCompileFailure::UnsupportedGeometry)?;
                // Filled-path support is style-dependent and was validated before
                // PreparedMorph construction. This second pass has no style context, so
                // it deliberately revalidates only renderer-independent correspondence.
                Ok(Some(TransformGeometryPlan::PathPair {
                    geometry: Arc::new(geometry.clone()),
                    render_frame: *render_frame,
                    prestart_geometry: render_frame
                        .filter(|render| render.from != *source_transform)
                        .map(|render| {
                            prepared_pair_in_source_frame(source, render.from, *source_transform)
                        })
                        .transpose()?
                        .map(Arc::new),
                }))
            }
            _ => Ok(None),
        };
    }
    if property != Property::Transform {
        return Ok(None);
    }
    let TrackValues::Object { from, to } = values else {
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
                .expect("supported source geometry must convert to a path");
            let target = noon_geometry::canonical_outline_path(&to.geometry)
                .expect("supported target geometry must convert to a path");
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

/// Compile a typed analytic or resource-path point-correspondence transform without constructing
/// an authored object endpoint. The returned resource is execution-owned and may
/// use a fixed render frame while semantic affine channels remain independently visible.
pub(crate) fn compile_content_morph(
    from_geometry: &GeometryRef,
    to_geometry: &GeometryRef,
    from_style: Style,
    to_style: Style,
    from_transform: Transform2D,
    to_transform: Transform2D,
) -> Result<(GeometryRef, Option<noon_core::MorphRenderFrame>), TransformCompileFailure> {
    let supported = matches!(
        (from_geometry, to_geometry),
        (GeometryRef::Circle { .. }, GeometryRef::Rectangle { .. })
            | (GeometryRef::Rectangle { .. }, GeometryRef::Circle { .. })
            | (GeometryRef::Circle { .. }, GeometryRef::Circle { .. })
            | (GeometryRef::Rectangle { .. }, GeometryRef::Rectangle { .. })
            | (GeometryRef::VectorPath(_), GeometryRef::VectorPath(_))
            | (GeometryRef::Line { .. }, GeometryRef::Line { .. })
            | (
                GeometryRef::VectorPath(_),
                GeometryRef::Circle { .. }
                    | GeometryRef::Rectangle { .. }
                    | GeometryRef::Line { .. }
            )
            | (
                GeometryRef::Circle { .. }
                    | GeometryRef::Rectangle { .. }
                    | GeometryRef::Line { .. },
                GeometryRef::VectorPath(_)
            )
    );
    if !supported {
        return Err(TransformCompileFailure::UnsupportedGeometry);
    }
    let source = noon_geometry::canonical_outline_path(from_geometry)
        .expect("supported source geometry must convert to a path");
    let target = noon_geometry::canonical_outline_path(to_geometry)
        .expect("supported target geometry must convert to a path");
    let screen_line_width_morph = matches!(
        (from_geometry, to_geometry),
        (GeometryRef::Line { .. }, GeometryRef::Line { .. })
    ) && screen_line_width_only_change(from_style, to_style);
    // Stroke width is a runtime style channel for screen-space lines: it does
    // not change the prepared centerline resource or its path topology.
    let pair_from_style = if screen_line_width_morph {
        Style {
            stroke_width: to_style.stroke_width,
            ..from_style
        }
    } else {
        from_style
    };
    let TransformGeometryPlan::PathPair {
        geometry,
        render_frame,
        ..
    } = compile_path_pair(
        pair_from_style,
        to_style,
        from_transform,
        to_transform,
        source,
        target,
    )?
    else {
        unreachable!("point transform compiles to a path pair")
    };
    if screen_space_stroke_requires_fixed_frame(from_style, to_style) && render_frame.is_none() {
        return Err(TransformCompileFailure::RequiresRetessellation);
    }
    Ok((geometry.as_ref().clone(), render_frame))
}

fn screen_space_stroke_requires_fixed_frame(from: Style, to: Style) -> bool {
    let screen_space = from.stroke_width_mode == StrokeWidthMode::ScreenSpace
        || to.stroke_width_mode == StrokeWidthMode::ScreenSpace;
    let stroked = (from.stroke.is_some() && from.stroke_width > 0.0)
        || (to.stroke.is_some() && to.stroke_width > 0.0);
    screen_space && stroked
}

pub(crate) fn morph_requires_filled_topology(from: Style, to: Style) -> bool {
    from.fill.is_some_and(|color| color.alpha != 0.0)
        || to.fill.is_some_and(|color| color.alpha != 0.0)
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
    let fill_topology_required = morph_requires_filled_topology(from_style, to_style);
    if fill_topology_required && !filled_morph_is_supported(&source, &target) {
        return Err(TransformCompileFailure::UnsafeFilledPath);
    }
    // A shape frame keeps stroke tessellation and path resource identity independent
    // of animation progress and endpoint translations. Interpolate translation in
    // the runtime frame: pointwise interpolation is linear in both endpoint points
    // and translation, so unrelated moves can share the same endpoint geometry. Prepared
    // morph evaluation owns this frame through an interior singular scale, so only the
    // endpoint transforms need to be invertible when a later independent TRS driver
    // takes ownership.
    if from_style.stroke_width_mode == StrokeWidthMode::ScreenSpace
        && to_style.stroke_width_mode == StrokeWidthMode::ScreenSpace
    {
        let world_source = source.transformed(from_transform);
        let world_target = target.transformed(to_transform);
        let render_frame = noon_core::MorphRenderFrame {
            from: Transform2D {
                translation: from_transform.translation,
                ..Transform2D::IDENTITY
            },
            to_translation: to_transform.translation,
        };
        let intermediate_translation = render_frame.sample(0.5).translation;
        let frame_source = source.transformed(Transform2D {
            translation: noon_core::Vec2::ZERO,
            rotation: from_transform.rotation,
            scale: from_transform.scale,
        });
        let frame_target = target.transformed(Transform2D {
            translation: noon_core::Vec2::ZERO,
            rotation: to_transform.rotation,
            scale: to_transform.scale,
        });
        // Overflowed derived points and unsupported world-space correspondence retain
        // the established local plan rather than installing an invalid resource.
        if world_source.is_finite()
            && world_target.is_finite()
            && frame_source.is_finite()
            && frame_target.is_finite()
            && intermediate_translation.x.is_finite()
            && intermediate_translation.y.is_finite()
            && fixed_frame_inverse_is_finite(
                &world_source,
                &world_target,
                from_transform,
                to_transform,
            )
            && (!fill_topology_required || filled_morph_is_supported(&frame_source, &frame_target))
        {
            return Ok(TransformGeometryPlan::PathPair {
                geometry: Arc::new(GeometryRef::path(
                    frame_source.with_morph_target(frame_target),
                )),
                render_frame: Some(render_frame),
                prestart_geometry: None,
            });
        }
    }
    Ok(TransformGeometryPlan::PathPair {
        geometry: Arc::new(GeometryRef::path(source.with_morph_target(target))),
        render_frame: None,
        prestart_geometry: None,
    })
}

fn prepared_pair_in_source_frame(
    path: &VectorPath,
    render: Transform2D,
    source: Transform2D,
) -> Result<GeometryRef, TransformCompileFailure> {
    if source.scale.x.abs() <= 1.0e-7 || source.scale.y.abs() <= 1.0e-7 {
        return Err(TransformCompileFailure::RequiresRetessellation);
    }
    // Inverse nonuniform TRS requires rotation before reciprocal scale. This
    // cold preparation keeps pre-activation frames on the ordinary affine lane.
    let path = path
        .transformed(render)
        .transformed(Transform2D {
            translation: (-source.translation).rotate(-source.rotation),
            rotation: -source.rotation,
            ..Transform2D::IDENTITY
        })
        .transformed(Transform2D {
            scale: noon_core::Vec2::new(1.0 / source.scale.x, 1.0 / source.scale.y),
            ..Transform2D::IDENTITY
        });
    if !path.is_finite() {
        return Err(TransformCompileFailure::RequiresRetessellation);
    }
    Ok(GeometryRef::path(path))
}

fn screen_line_width_only_change(from: Style, to: Style) -> bool {
    from.stroke_width_mode == StrokeWidthMode::ScreenSpace
        && to.stroke_width_mode == StrokeWidthMode::ScreenSpace
        && from.stroke.is_some()
        && to.stroke.is_some()
        && from.fill == to.fill
        && from.stroke == to.stroke
        && from.stroke_join == to.stroke_join
        && from.stroke_cap == to.stroke_cap
        && from.opacity == to.opacity
}

fn filled_morph_is_supported(source: &VectorPath, target: &VectorPath) -> bool {
    // Prefer exact ordered affine reflection when winding changes. The general
    // planner remains the fallback for the established non-inverting class.
    noon_geometry::plan_filled_affine_winding_flip_preserving_order(
        source,
        target,
        noon_geometry::MorphOptions::DEFAULT,
    )
    .is_ok()
        || noon_geometry::plan_filled_morph_preserving_order(
            source,
            target,
            noon_geometry::MorphOptions::DEFAULT,
        )
        .is_ok()
        // Complex/concave and changing-contour fills have no valid retained fan.
        // They retain the SAME ordered endpoint resource and progress channel;
        // the renderer samples/tessellates only that affected path locally.
        || noon_geometry::PreparedPathInterpolation::new(source, target).is_ok()
}

// Independent drivers can take over a fixed frame only if its conversion back
// to either nonsingular endpoint semantic TRS is finite. The prepared morph owns
// the render frame at interior singular instants, so those instants need no inverse.
fn fixed_frame_inverse_is_finite(
    source: &VectorPath,
    target: &VectorPath,
    from: Transform2D,
    to: Transform2D,
) -> bool {
    const MIN_ENDPOINT_SCALE: f32 = 1.0e-7;
    if [from.scale.x, from.scale.y, to.scale.x, to.scale.y]
        .into_iter()
        .any(|scale| !scale.is_finite() || scale.abs() <= MIN_ENDPOINT_SCALE)
    {
        return false;
    }
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
    use noon_core::{Color, PathCommand, Vec2};

    #[test]
    fn identity_free_value_plan_matches_stable_track_plan() {
        let values = TrackValues::Object {
            from: noon_core::TransformTrackEndpoint {
                geometry: GeometryRef::circle(1.0),
                transform: Transform2D::IDENTITY,
                style: Style::default(),
            },
            to: noon_core::TransformTrackEndpoint {
                geometry: GeometryRef::rectangle(2.0, 1.0),
                transform: Transform2D::IDENTITY,
                style: Style::default(),
            },
        };
        let direct = compile_transform_geometry_values(Property::Transform, &values).unwrap();
        let track = TrackDefinition {
            id: noon_core::TrackId::new(7),
            object: noon_core::ObjectId::new(11),
            property: Property::Transform,
            values,
            timing: noon_core::TrackTiming::new(0.0, 1.0, noon_core::RateFunction::Linear),
            time_map: noon_core::CompositionTimeMap::identity(),
        };
        assert_eq!(direct, compile_transform_geometry_plan(&track).unwrap());
        assert!(matches!(
            direct,
            Some(TransformGeometryPlan::PathPair { .. })
        ));
    }

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
            (Vec2::new(-2.0, 0.5), Vec2::ZERO, true),
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
                render_frame,
                ..
            } = plan
            else {
                panic!("pair");
            };
            assert_eq!(render_frame.is_some(), fixed);
            assert!(geometry.is_finite());
        }
    }

    #[test]
    fn fixed_screen_space_frame_reuses_geometry_across_translated_morphs() {
        let source = VectorPath::new()
            .move_to(Vec2::new(-1.0, 0.5))
            .line_to(Vec2::new(0.5, 1.5));
        let target = VectorPath::new()
            .move_to(Vec2::new(1.25, -0.75))
            .line_to(Vec2::new(2.0, 0.25));
        let style = Style {
            fill: None,
            stroke: Some(Color::WHITE),
            stroke_width: 0.08,
            stroke_width_mode: StrokeWidthMode::ScreenSpace,
            ..Style::default()
        };
        let from = Transform2D {
            translation: Vec2::new(32.0, -24.0),
            rotation: std::f32::consts::FRAC_PI_4,
            scale: Vec2::new(0.5, 2.0),
        };
        let to = Transform2D {
            translation: Vec2::new(41.0, -39.0),
            rotation: -std::f32::consts::FRAC_PI_6,
            scale: Vec2::new(1.2, 0.75),
        };
        let offset = Vec2::new(4096.0, -8192.0);
        let translated_from = Transform2D {
            translation: from.translation + offset,
            ..from
        };
        let translated_to = Transform2D {
            translation: to.translation + offset,
            ..to
        };

        let TransformGeometryPlan::PathPair {
            geometry,
            render_frame: Some(render_frame),
            ..
        } = compile_path_pair(style, style, from, to, source.clone(), target.clone()).unwrap()
        else {
            panic!("screen-space path pair must use a fixed frame")
        };
        let TransformGeometryPlan::PathPair {
            geometry: translated_geometry,
            render_frame: Some(translated_render_frame),
            ..
        } = compile_path_pair(
            style,
            style,
            translated_from,
            translated_to,
            source.clone(),
            target.clone(),
        )
        .unwrap()
        else {
            panic!("translated screen-space path pair must use a fixed frame")
        };

        assert_eq!(geometry, translated_geometry);
        assert_eq!(render_frame.from.translation, from.translation);
        assert_eq!(
            translated_render_frame.from.translation,
            translated_from.translation
        );

        let GeometryRef::VectorPath(frame_source) = geometry.as_ref() else {
            panic!("path pair geometry")
        };
        let frame_target = frame_source.morph_target().expect("path pair target");
        let PathCommand::MoveTo {
            to: source_in_frame,
        } = frame_source.commands()[0]
        else {
            panic!("source move")
        };
        let PathCommand::MoveTo {
            to: target_in_frame,
        } = frame_target.commands()[0]
        else {
            panic!("target move")
        };
        let PathCommand::MoveTo { to: source_point } = source.commands()[0] else {
            panic!("source point")
        };
        let PathCommand::MoveTo { to: target_point } = target.commands()[0] else {
            panic!("target point")
        };

        for progress in [0.0, 0.5, 1.0] {
            let frame_point = source_in_frame + (target_in_frame - source_in_frame) * progress;
            let expected = from.transform_point(source_point)
                + (to.transform_point(target_point) - from.transform_point(source_point))
                    * progress;
            let translated_expected = translated_from.transform_point(source_point)
                + (translated_to.transform_point(target_point)
                    - translated_from.transform_point(source_point))
                    * progress;
            let actual = render_frame.sample(progress).transform_point(frame_point);
            let translated_actual = translated_render_frame
                .sample(progress)
                .transform_point(frame_point);
            assert!(
                (actual - expected).length() <= 1.0e-4,
                "progress {progress}: expected {expected:?}, got {actual:?}"
            );
            assert!(
                (translated_actual - translated_expected).length() <= 1.0e-4,
                "translated progress {progress}: expected {translated_expected:?}, got {translated_actual:?}"
            );
        }
    }

    #[test]
    fn screen_space_morphs_share_geometry_across_600_independent_moves() {
        let geometry = GeometryRef::rectangle(0.14, 0.14);
        let style = Style {
            stroke_width_mode: StrokeWidthMode::ScreenSpace,
            ..Style::default()
        };
        let mut shared = None;
        for index in 0..600 {
            let from = Transform2D {
                translation: Vec2::new((index % 30) as f32 * 0.3, (index / 30) as f32 * 0.3),
                rotation: std::f32::consts::FRAC_PI_2,
                ..Transform2D::IDENTITY
            };
            let angle = index as f32 * 0.07;
            let to = Transform2D {
                translation: Vec2::new(angle.cos(), angle.sin()) * (index as f32 / 100.0),
                rotation: from.rotation + std::f32::consts::FRAC_PI_3,
                ..Transform2D::IDENTITY
            };
            let (pair, Some(frame)) =
                compile_content_morph(&geometry, &geometry, style, style, from, to).unwrap()
            else {
                panic!("screen-space morph must retain a sampled shape frame")
            };
            if let Some(expected) = &shared {
                assert_eq!(
                    &pair, expected,
                    "move {index} must not create distinct path coordinates"
                );
            } else {
                shared = Some(pair.clone());
            }
            let GeometryRef::VectorPath(path) = pair else {
                panic!("point morph has a path pair")
            };
            let PathCommand::MoveTo { to: a } = path.commands()[0] else {
                panic!("source corner")
            };
            let PathCommand::MoveTo { to: b } = path.morph_target().unwrap().commands()[0] else {
                panic!("target corner")
            };
            let point = Vec2::new(0.07, 0.07);
            // Compare to the independent world-space point definition, including
            // the shrunken rotation midpoint rather than rigid rotation.
            for alpha in [0.0, 0.25, 0.5, 0.75, 1.0] {
                let expected = from.transform_point(point)
                    + (to.transform_point(point) - from.transform_point(point)) * alpha;
                let actual = frame.sample(alpha).transform_point(a + (b - a) * alpha);
                assert!(
                    (actual - expected).length() <= 1.0e-5,
                    "move {index}@{alpha}: {actual:?} != {expected:?}"
                );
            }
        }
    }

    #[test]
    fn screen_space_point_correspondence_accepts_affine_reflection_through_singular_midpoint() {
        let style = Style {
            fill: Some(Color::WHITE),
            stroke: Some(Color::BLACK),
            stroke_width: 0.08,
            stroke_width_mode: StrokeWidthMode::ScreenSpace,
            ..Style::default()
        };
        let reflection = Transform2D {
            scale: Vec2::new(-1.0, 1.0),
            ..Transform2D::IDENTITY
        };
        let (geometry, render_frame) = compile_content_morph(
            &GeometryRef::rectangle(2.0, 1.0),
            &GeometryRef::rectangle(2.0, 1.0),
            style,
            style,
            Transform2D::IDENTITY,
            reflection,
        )
        .expect("nonsingular reflected endpoints keep one fixed render frame");
        assert_eq!(
            render_frame,
            Some(noon_core::MorphRenderFrame::fixed(Transform2D::IDENTITY))
        );
        let GeometryRef::VectorPath(path) = geometry else {
            panic!("point correspondence must compile to a path pair")
        };
        let target = path.morph_target().expect("prepared reflected target");
        let fill = noon_geometry::plan_filled_affine_winding_flip_preserving_order(
            &path,
            target,
            noon_geometry::MorphOptions::DEFAULT,
        )
        .expect("reflected fill must carry the affine winding proof");
        assert!(fill
            .interpolate_vertices(0.5)
            .iter()
            .all(|point| point.x.abs() < 1.0e-5));
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
        let (geometry, render_frame) = compile_content_morph(
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
        assert_eq!(
            render_frame,
            Some(noon_core::MorphRenderFrame::fixed(Transform2D::IDENTITY))
        );
        assert!(path
            .conservative_bounds()
            .is_some_and(|bounds| bounds.width() > 2.5 && bounds.height() > 2.5));
    }

    #[test]
    fn screen_space_line_content_morph_keeps_rotated_endpoints_in_fixed_frame() {
        let style = Style {
            stroke: Some(Color::WHITE),
            stroke_width: 0.0375,
            stroke_width_mode: StrokeWidthMode::ScreenSpace,
            ..Style::default()
        };
        let (geometry, render_frame) = compile_content_morph(
            &GeometryRef::line(Vec2::ZERO, Vec2::new(0.75, 0.0)),
            &GeometryRef::line(Vec2::ZERO, Vec2::new(0.0, 0.75)),
            style,
            style,
            Transform2D::IDENTITY,
            Transform2D::IDENTITY,
        )
        .expect("line endpoints can rotate through the prepared path morph");

        let GeometryRef::VectorPath(path) = geometry else {
            panic!("line content morph must use the retained path pair")
        };
        let target = path.morph_target().expect("rotated line endpoint path");
        assert_eq!(
            render_frame,
            Some(noon_core::MorphRenderFrame::fixed(Transform2D::IDENTITY))
        );
        assert_eq!(path.commands().len(), 2);
        assert_eq!(target.commands().len(), 2);
        assert_eq!(
            path.commands()[1],
            PathCommand::LineTo {
                to: Vec2::new(0.75, 0.0)
            }
        );
        assert_eq!(
            target.commands()[1],
            PathCommand::LineTo {
                to: Vec2::new(0.0, 0.75)
            }
        );
    }

    #[test]
    fn screen_space_line_content_morph_allows_only_width_change_for_shared_topology() {
        let source_style = Style {
            fill: None,
            stroke: Some(Color::WHITE),
            stroke_width: 0.025,
            stroke_width_mode: StrokeWidthMode::ScreenSpace,
            ..Style::default()
        };
        let target_style = Style {
            stroke_width: 0.075,
            ..source_style
        };
        let source = GeometryRef::line(Vec2::ZERO, Vec2::new(0.75, 0.0));
        let target = GeometryRef::line(Vec2::ZERO, Vec2::new(0.0, 0.75));
        let (geometry, render_frame) = compile_content_morph(
            &source,
            &target,
            source_style,
            target_style,
            Transform2D::IDENTITY,
            Transform2D::IDENTITY,
        )
        .expect("screen-space line width is a runtime style channel");
        assert!(matches!(geometry, GeometryRef::VectorPath(_)));
        assert_eq!(
            render_frame,
            Some(noon_core::MorphRenderFrame::fixed(Transform2D::IDENTITY))
        );

        let scale_with_object = Style {
            stroke_width_mode: StrokeWidthMode::ScaleWithObject,
            ..source_style
        };
        assert_eq!(
            compile_content_morph(
                &source,
                &target,
                scale_with_object,
                Style {
                    stroke_width: 0.075,
                    ..scale_with_object
                },
                Transform2D::IDENTITY,
                Transform2D::IDENTITY,
            ),
            Err(TransformCompileFailure::RequiresRetessellation)
        );

        assert_eq!(
            compile_content_morph(
                &source,
                &target,
                source_style,
                Style {
                    stroke_cap: noon_core::StrokeCap::Butt,
                    ..target_style
                },
                Transform2D::IDENTITY,
                Transform2D::IDENTITY,
            ),
            Err(TransformCompileFailure::RequiresRetessellation)
        );
    }

    #[test]
    fn analytic_screen_space_morph_rejects_singular_fixed_frame() {
        let style = Style {
            stroke: Some(Color::WHITE),
            stroke_width_mode: StrokeWidthMode::ScreenSpace,
            ..Style::default()
        };
        let singular = Transform2D {
            scale: Vec2::new(0.0, 1.0),
            ..Transform2D::IDENTITY
        };

        assert_eq!(
            compile_content_morph(
                &GeometryRef::rectangle(2.0, 2.0),
                &GeometryRef::circle(1.0),
                style,
                style,
                singular,
                Transform2D::IDENTITY,
            ),
            Err(TransformCompileFailure::RequiresRetessellation)
        );
    }

    #[test]
    fn fill_only_screen_space_morph_allows_local_plan_when_fixed_frame_is_singular() {
        let style = Style {
            fill: Some(Color::WHITE),
            stroke: None,
            stroke_width: 0.0,
            stroke_width_mode: StrokeWidthMode::ScreenSpace,
            ..Style::default()
        };
        let singular = Transform2D {
            scale: Vec2::new(0.0, 1.0),
            ..Transform2D::IDENTITY
        };

        let (geometry, render_frame) = compile_content_morph(
            &GeometryRef::rectangle(2.0, 2.0),
            &GeometryRef::circle(1.0),
            style,
            style,
            singular,
            Transform2D::IDENTITY,
        )
        .expect("fill-only morphs do not need a fixed frame for absent screen-space strokes");
        let GeometryRef::VectorPath(path) = geometry else {
            panic!("content morph must compile to a path pair")
        };
        assert!(path.morph_target().is_some());
        assert_eq!(render_frame, None);
    }

    #[test]
    fn transparent_fill_open_path_morph_uses_stroke_correspondence() {
        let source = VectorPath::new().move_to(Vec2::new(-1.0, 0.0)).cubic_to(
            Vec2::new(-0.5, 1.0),
            Vec2::new(0.5, 1.0),
            Vec2::new(1.0, 0.0),
        );
        let target = VectorPath::new().move_to(Vec2::new(-1.25, 0.25)).cubic_to(
            Vec2::new(-0.25, 1.25),
            Vec2::new(0.75, 0.75),
            Vec2::new(1.25, -0.25),
        );
        let transparent_fill = Color {
            alpha: 0.0,
            ..Color::WHITE
        };
        let style = Style {
            fill: Some(transparent_fill),
            stroke: Some(Color::WHITE),
            stroke_width: 0.08,
            stroke_width_mode: StrokeWidthMode::ScreenSpace,
            ..Style::default()
        };

        let plan = compile_path_pair(
            style,
            style,
            Transform2D::IDENTITY,
            Transform2D::IDENTITY,
            source.clone(),
            target.clone(),
        )
        .expect("transparent fill must not request filled topology for an open path");
        assert!(matches!(
            plan,
            TransformGeometryPlan::PathPair {
                render_frame: Some(frame),
                ..
            } if frame == noon_core::MorphRenderFrame::fixed(Transform2D::IDENTITY)
        ));

        let visible_fill = Style {
            fill: Some(Color::WHITE),
            ..style
        };
        assert!(
            compile_path_pair(
                visible_fill,
                visible_fill,
                Transform2D::IDENTITY,
                Transform2D::IDENTITY,
                source,
                target,
            )
            .is_ok(),
            "open fills use the same implicit closure as ordinary tessellation"
        );
    }
}
