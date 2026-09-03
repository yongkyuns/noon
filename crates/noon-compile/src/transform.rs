use noon_core::{GeometryRef, ObjectSnapshot, Property, Style, TrackDefinition, TrackValues, VectorPath};

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
    PathPair(GeometryRef),
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
        (GeometryRef::VectorPath(source), GeometryRef::VectorPath(target)) => {
            compile_path_pair(from, to, source.clone(), target.clone())?
        }
        (GeometryRef::Circle { .. }, GeometryRef::Rectangle { .. })
        | (GeometryRef::Rectangle { .. }, GeometryRef::Circle { .. }) => {
            let source = noon_geometry::canonical_outline_path(&from.geometry)
                .expect("closed analytic source geometry must convert to a path");
            let target = noon_geometry::canonical_outline_path(&to.geometry)
                .expect("closed analytic target geometry must convert to a path");
            compile_path_pair(from, to, source, target)?
        }
        _ => return Err(TransformCompileFailure::UnsupportedGeometry),
    };
    Ok(Some(plan))
}

fn path_style_requires_retessellation(from: Style, to: Style) -> bool {
    from.stroke_width.to_bits() != to.stroke_width.to_bits()
        || from.stroke_join != to.stroke_join
        || from.stroke_cap != to.stroke_cap
        || from.fill.is_some() != to.fill.is_some()
}

fn compile_path_pair(
    from: &ObjectSnapshot,
    to: &ObjectSnapshot,
    source: VectorPath,
    target: VectorPath,
) -> Result<TransformGeometryPlan, TransformCompileFailure> {
    if path_style_requires_retessellation(from.style, to.style) {
        return Err(TransformCompileFailure::RequiresRetessellation);
    }
    if from.style.fill.is_some()
        && noon_geometry::plan_filled_morph(&source, &target, noon_geometry::MorphOptions::DEFAULT)
            .is_err()
    {
        return Err(TransformCompileFailure::UnsafeFilledPath);
    }
    Ok(TransformGeometryPlan::PathPair(GeometryRef::path(
        source.with_morph_target(target),
    )))
}
