//! Geometry layout bounds read directly from immutable semantic resources.
use super::*;
fn include_layout_point(bounds: &mut Option<Bounds2D64>, point: (f64, f64)) {
    if let Some(bounds) = bounds {
        bounds.include(point.0, point.1);
    } else {
        *bounds = Some(Bounds2D64::point(point.0, point.1));
    }
}
fn transform_layout_point(transform: SemanticTransform2_5D, point: Vec2) -> (f64, f64) {
    transform_layout_xy(transform, f64::from(point.x), f64::from(point.y))
}
pub(super) fn transform_layout_xy(transform: SemanticTransform2_5D, x: f64, y: f64) -> (f64, f64) {
    let x = x * transform.scale.x;
    let y = y * transform.scale.y;
    let sine = transform.rotation_z.sin();
    let cosine = transform.rotation_z.cos();
    (
        x * cosine - y * sine + transform.translation.x,
        x * sine + y * cosine + transform.translation.y,
    )
}

fn canonical_circle_layout_bounds(
    radius: f32,
    transform: SemanticTransform2_5D,
    include_handles: bool,
) -> Option<Bounds2D64> {
    let radius = f64::from(radius);
    let factor = (4.0 / 3.0) * (std::f64::consts::PI / 16.0).tan();
    let mut bounds = None;
    for index in 0..8 {
        let start_angle = f64::from(index) * std::f64::consts::PI / 4.0;
        let end_angle = f64::from(index + 1) * std::f64::consts::PI / 4.0;
        let (start_sine, start_cosine) = start_angle.sin_cos();
        let (end_sine, end_cosine) = end_angle.sin_cos();
        for (point_index, (x, y)) in [
            (start_cosine, start_sine),
            (
                start_cosine - factor * start_sine,
                start_sine + factor * start_cosine,
            ),
            (
                end_cosine + factor * end_sine,
                end_sine - factor * end_cosine,
            ),
            (end_cosine, end_sine),
        ]
        .into_iter()
        .enumerate()
        {
            if !include_handles && (point_index == 1 || point_index == 2) {
                continue;
            }
            include_layout_point(
                &mut bounds,
                transform_layout_xy(transform, radius * x, radius * y),
            );
        }
    }
    bounds
}
// Manim dimensions bound cubic control points; centers and edges bound anchors.
// These are layout bounds; runtime visibility continues using geometric bounds.
fn transformed_path_layout_bounds(
    path: &VectorPath,
    transform: SemanticTransform2_5D,
    include_handles: bool,
) -> Option<Bounds2D64> {
    let mut bounds = None;
    let mut current = None;
    let mut subpath_start = None;

    for command in path.commands() {
        match *command {
            PathCommand::MoveTo { to } => {
                let point = transform_layout_point(transform, to);
                include_layout_point(&mut bounds, point);
                current = Some(point);
                subpath_start = Some(point);
            }
            PathCommand::LineTo { to } => {
                let end = transform_layout_point(transform, to);
                if let Some(start) = current {
                    include_layout_point(&mut bounds, start);
                }
                include_layout_point(&mut bounds, end);
                current = Some(end);
            }
            PathCommand::QuadraticTo { control, to } => {
                let end = transform_layout_point(transform, to);
                let Some(start) = current else {
                    include_layout_point(&mut bounds, end);
                    current = Some(end);
                    continue;
                };
                let control = transform_layout_point(transform, control);
                include_layout_point(&mut bounds, start);
                include_layout_point(&mut bounds, end);
                // A quadratic is the equivalent cubic with handles at 2/3.
                for anchor in [start, end].into_iter().filter(|_| include_handles) {
                    include_layout_point(
                        &mut bounds,
                        (
                            anchor.0 + (control.0 - anchor.0) * (2.0 / 3.0),
                            anchor.1 + (control.1 - anchor.1) * (2.0 / 3.0),
                        ),
                    );
                }
                current = Some(end);
            }
            PathCommand::CubicTo {
                control1,
                control2,
                to,
            } => {
                let end = transform_layout_point(transform, to);
                let Some(start) = current else {
                    include_layout_point(&mut bounds, end);
                    current = Some(end);
                    continue;
                };
                let control1 = transform_layout_point(transform, control1);
                let control2 = transform_layout_point(transform, control2);
                include_layout_point(&mut bounds, start);
                include_layout_point(&mut bounds, end);
                if include_handles {
                    include_layout_point(&mut bounds, control1);
                    include_layout_point(&mut bounds, control2);
                }
                current = Some(end);
            }
            PathCommand::Close => {
                if let Some(end) = current {
                    include_layout_point(&mut bounds, end);
                }
                if let Some(start) = subpath_start {
                    include_layout_point(&mut bounds, start);
                    current = Some(start);
                }
            }
        }
    }
    bounds
}
fn geometry_layout_bounds(
    geometry: &GeometryRef,
    transform: SemanticTransform2_5D,
    include_handles: bool,
) -> Option<Bounds2D64> {
    match geometry {
        GeometryRef::Circle { radius } => {
            canonical_circle_layout_bounds(*radius, transform, include_handles)
        }
        GeometryRef::Rectangle { size } => {
            let half_x = f64::from(size.x) * 0.5;
            let half_y = f64::from(size.y) * 0.5;
            let mut bounds = None;
            for (x, y) in [
                (-half_x, -half_y),
                (-half_x, half_y),
                (half_x, -half_y),
                (half_x, half_y),
            ] {
                let sine = transform.rotation_z.sin();
                let cosine = transform.rotation_z.cos();
                let x = x * transform.scale.x;
                let y = y * transform.scale.y;
                include_layout_point(
                    &mut bounds,
                    (
                        x * cosine - y * sine + transform.translation.x,
                        x * sine + y * cosine + transform.translation.y,
                    ),
                );
            }
            bounds
        }
        GeometryRef::Line { start, end } => {
            let mut bounds = None;
            include_layout_point(&mut bounds, transform_layout_point(transform, *start));
            include_layout_point(&mut bounds, transform_layout_point(transform, *end));
            bounds
        }
        GeometryRef::VectorPath(path) => {
            transformed_path_layout_bounds(path, transform, include_handles)
        }
        GeometryRef::External(_) => None,
    }
}
pub(crate) fn layout_for_content(
    store: &SemanticStore,
    content: SemanticObjectContent,
    transform: SemanticTransform2_5D,
) -> Result<Option<Bounds2D64>, AuthoringError> {
    measure_content(store, content, transform, true)
}

pub(crate) fn boundary_for_content(
    store: &SemanticStore,
    content: SemanticObjectContent,
    transform: SemanticTransform2_5D,
) -> Result<Option<Bounds2D64>, AuthoringError> {
    measure_content(store, content, transform, false)
}

fn measure_content(
    store: &SemanticStore,
    content: SemanticObjectContent,
    transform: SemanticTransform2_5D,
    include_handles: bool,
) -> Result<Option<Bounds2D64>, AuthoringError> {
    let geometry = match content {
        SemanticObjectContent::Geometry(geometry) => geometry,
        SemanticObjectContent::Text(handle) => {
            let local = store
                .text_resources()
                .get(handle)
                .ok_or(AuthoringError::MissingTextResource(handle))?
                .bounds;
            let mut bounds = None;
            for point in [
                local.min,
                Vec2::new(local.min.x, local.max.y),
                local.max,
                Vec2::new(local.max.x, local.min.y),
            ] {
                include_layout_point(&mut bounds, transform_layout_point(transform, point));
            }
            return Ok(bounds);
        }
    };
    Ok(match geometry {
        StoredGeometry::Circle { radius } => {
            geometry_layout_bounds(&GeometryRef::circle(radius), transform, include_handles)
        }
        StoredGeometry::Rectangle { size } => {
            geometry_layout_bounds(&GeometryRef::Rectangle { size }, transform, include_handles)
        }
        StoredGeometry::Line { start, end } => geometry_layout_bounds(
            &GeometryRef::Line { start, end },
            transform,
            include_handles,
        ),
        StoredGeometry::Resource(handle) => match store
            .geometry_resources()
            .get(handle)
            .ok_or(AuthoringError::MissingGeometryResource(handle))?
        {
            GeometryResource::VectorPath(path) => {
                transformed_path_layout_bounds(path, transform, include_handles)
            }
        },
    })
}
