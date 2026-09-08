//! Closed retained contours used by shared annular-sector, sector and annulus handles.
use crate::arc_authoring::{authored_f32, circular_arc_path, ArcAuthoringError};
use noon_core::{GeometryRef, PathCommand, Vec2, VectorPath, TAU};

fn append_command(path: VectorPath, command: PathCommand) -> VectorPath {
    match command {
        PathCommand::MoveTo { to } => path.move_to(to),
        PathCommand::LineTo { to } => path.line_to(to),
        PathCommand::QuadraticTo { control, to } => path.quadratic_to(control, to),
        PathCommand::CubicTo {
            control1,
            control2,
            to,
        } => path.cubic_to(control1, control2, to),
        PathCommand::Close => path.close(),
    }
}

fn append_path(
    mut destination: VectorPath,
    source: &VectorPath,
    skip_first_move: bool,
) -> VectorPath {
    for (index, command) in source.commands().iter().copied().enumerate() {
        if skip_first_move && index == 0 && matches!(command, PathCommand::MoveTo { .. }) {
            continue;
        }
        destination = append_command(destination, command);
    }
    destination
}

fn first_point(path: &VectorPath) -> Vec2 {
    match path.commands().first().copied() {
        Some(PathCommand::MoveTo { to }) => to,
        other => unreachable!("arc path must start with MoveTo, got {other:?}"),
    }
}

fn annular_sector_path(
    inner_radius: f32,
    outer_radius: f32,
    angle: f32,
    start_angle: f32,
    num_components: usize,
    arc_center: Vec2,
) -> Result<VectorPath, ArcAuthoringError> {
    let inner = circular_arc_path(inner_radius, start_angle, angle, num_components, arc_center)?;
    // Manim reverses an independently constructed outer arc before joining it
    // to the inner arc. Building it directly with the opposite signed angle is
    // equivalent and preserves the cubic control-point geometry.
    let outer = circular_arc_path(
        outer_radius,
        start_angle + angle,
        -angle,
        num_components,
        arc_center,
    )?;

    let inner_start = first_point(&inner);
    let outer_end = first_point(&outer);
    let mut path = append_path(VectorPath::new(), &inner, false);
    path = path.line_to(outer_end);
    path = append_path(path, &outer, true);
    Ok(path.line_to(inner_start).close())
}

fn annulus_path(
    inner_radius: f32,
    outer_radius: f32,
    num_components: usize,
    arc_center: Vec2,
) -> Result<VectorPath, ArcAuthoringError> {
    let outer = circular_arc_path(outer_radius, 0.0, TAU, num_components, arc_center)?;
    let inner = circular_arc_path(inner_radius, TAU, -TAU, num_components, arc_center)?;

    // Separate, oppositely wound closed contours give the retained path the
    // same annular fill semantics as Manim's outer-circle + reversed-inner-circle.
    let mut path = append_path(VectorPath::new(), &outer, false).close();
    path = append_path(path, &inner, false).close();
    Ok(path)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn annular_sector_geometry(
    inner_radius: f64,
    outer_radius: f64,
    angle: f64,
    start_angle: f64,
    num_components: u32,
    center_x: f64,
    center_y: f64,
) -> Result<GeometryRef, String> {
    let path = annular_sector_path(
        authored_f32(inner_radius, "annular sector inner radius")?,
        authored_f32(outer_radius, "annular sector outer radius")?,
        authored_f32(angle, "annular sector angle")?,
        authored_f32(start_angle, "annular sector start angle")?,
        num_components as usize,
        Vec2::new(
            authored_f32(center_x, "annular sector center x")?,
            authored_f32(center_y, "annular sector center y")?,
        ),
    )
    .map_err(|error| error.to_string())?;
    Ok(GeometryRef::VectorPath(path))
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn sector_geometry(
    radius: f64,
    angle: f64,
    start_angle: f64,
    num_components: u32,
    center_x: f64,
    center_y: f64,
) -> Result<GeometryRef, String> {
    annular_sector_geometry(
        0.0,
        radius,
        angle,
        start_angle,
        num_components,
        center_x,
        center_y,
    )
}

pub(crate) fn annulus_geometry(
    inner_radius: f64,
    outer_radius: f64,
    num_components: u32,
    center_x: f64,
    center_y: f64,
) -> Result<GeometryRef, String> {
    let path = annulus_path(
        authored_f32(inner_radius, "annulus inner radius")?,
        authored_f32(outer_radius, "annulus outer radius")?,
        num_components as usize,
        Vec2::new(
            authored_f32(center_x, "annulus center x")?,
            authored_f32(center_y, "annulus center y")?,
        ),
    )
    .map_err(|error| error.to_string())?;
    Ok(GeometryRef::VectorPath(path))
}
