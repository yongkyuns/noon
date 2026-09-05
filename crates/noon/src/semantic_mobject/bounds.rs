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
    let x = f64::from(point.x) * transform.scale.x;
    let y = f64::from(point.y) * transform.scale.y;
    let sine = transform.rotation_z.sin();
    let cosine = transform.rotation_z.cos();
    (
        x * cosine - y * sine + transform.translation.x,
        x * sine + y * cosine + transform.translation.y,
    )
}
fn quadratic_layout_point(p0: (f64, f64), p1: (f64, f64), p2: (f64, f64), t: f64) -> (f64, f64) {
    let u = 1.0 - t;
    (
        u * u * p0.0 + 2.0 * u * t * p1.0 + t * t * p2.0,
        u * u * p0.1 + 2.0 * u * t * p1.1 + t * t * p2.1,
    )
}
fn cubic_layout_point(
    p0: (f64, f64),
    p1: (f64, f64),
    p2: (f64, f64),
    p3: (f64, f64),
    t: f64,
) -> (f64, f64) {
    let u = 1.0 - t;
    (
        u * u * u * p0.0 + 3.0 * u * u * t * p1.0 + 3.0 * u * t * t * p2.0 + t * t * t * p3.0,
        u * u * u * p0.1 + 3.0 * u * u * t * p1.1 + 3.0 * u * t * t * p2.1 + t * t * t * p3.1,
    )
}
fn cubic_layout_derivative_roots(p0: f64, p1: f64, p2: f64, p3: f64) -> Vec<f64> {
    let a = -p0 + 3.0 * p1 - 3.0 * p2 + p3;
    let b = 2.0 * (p0 - 2.0 * p1 + p2);
    let c = p1 - p0;
    let epsilon = 1.0e-14;
    if a.abs() <= epsilon {
        if b.abs() <= epsilon {
            return Vec::new();
        }
        return vec![-c / b];
    }
    let discriminant = b * b - 4.0 * a * c;
    if discriminant < 0.0 {
        return Vec::new();
    }
    let root = discriminant.sqrt();
    let mut roots = vec![(-b + root) / (2.0 * a)];
    if root > epsilon {
        roots.push((-b - root) / (2.0 * a));
    }
    roots
}
fn transformed_path_layout_bounds(
    path: &VectorPath,
    transform: SemanticTransform2_5D,
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
                for axis in 0..2 {
                    let (p0, p1, p2) = if axis == 0 {
                        (start.0, control.0, end.0)
                    } else {
                        (start.1, control.1, end.1)
                    };
                    let denominator = p0 - 2.0 * p1 + p2;
                    if denominator.abs() <= 1.0e-14 {
                        continue;
                    }
                    let t = (p0 - p1) / denominator;
                    if (0.0..1.0).contains(&t) {
                        include_layout_point(
                            &mut bounds,
                            quadratic_layout_point(start, control, end, t),
                        );
                    }
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
                let mut roots =
                    cubic_layout_derivative_roots(start.0, control1.0, control2.0, end.0);
                roots.extend(cubic_layout_derivative_roots(
                    start.1, control1.1, control2.1, end.1,
                ));
                for t in roots {
                    if (0.0..1.0).contains(&t) {
                        include_layout_point(
                            &mut bounds,
                            cubic_layout_point(start, control1, control2, end, t),
                        );
                    }
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
) -> Option<Bounds2D64> {
    match geometry {
        GeometryRef::Circle { radius } => {
            let radius = f64::from(*radius);
            let sine = transform.rotation_z.sin();
            let cosine = transform.rotation_z.cos();
            let half_width = radius * (transform.scale.x * cosine).hypot(transform.scale.y * sine);
            let half_height = radius * (transform.scale.x * sine).hypot(transform.scale.y * cosine);
            Some(Bounds2D64 {
                min_x: transform.translation.x - half_width,
                min_y: transform.translation.y - half_height,
                max_x: transform.translation.x + half_width,
                max_y: transform.translation.y + half_height,
            })
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
        GeometryRef::VectorPath(path) => transformed_path_layout_bounds(path, transform),
        GeometryRef::External(_) => None,
    }
}
pub(super) fn layout_for_content(
    store: &SemanticStore,
    geometry: StoredGeometry,
    transform: SemanticTransform2_5D,
) -> Result<Option<Bounds2D64>, String> {
    Ok(match geometry {
        StoredGeometry::Circle { radius } => {
            geometry_layout_bounds(&GeometryRef::circle(radius), transform)
        }
        StoredGeometry::Rectangle { size } => {
            geometry_layout_bounds(&GeometryRef::Rectangle { size }, transform)
        }
        StoredGeometry::Line { start, end } => {
            geometry_layout_bounds(&GeometryRef::Line { start, end }, transform)
        }
        StoredGeometry::Resource(handle) => match store
            .geometry_resources()
            .get(handle)
            .ok_or("unknown or stale geometry resource")?
        {
            GeometryResource::VectorPath(path) => transformed_path_layout_bounds(path, transform),
        },
    })
}
