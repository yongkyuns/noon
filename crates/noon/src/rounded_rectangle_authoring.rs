//! Pure rounded-rectangle path construction for shared semantic geometry.
use noon_core::{Vec2, VectorPath};

#[derive(Clone, Debug, PartialEq)]
pub enum RoundedRectangleAuthoringError {
    InvalidWidth(f32),
    InvalidHeight(f32),
    NonFiniteCornerRadius(f32),
}

impl std::fmt::Display for RoundedRectangleAuthoringError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidWidth(value) => write!(
                formatter,
                "rounded rectangle width must be finite and positive, got {value}"
            ),
            Self::InvalidHeight(value) => write!(
                formatter,
                "rounded rectangle height must be finite and positive, got {value}"
            ),
            Self::NonFiniteCornerRadius(value) => {
                write!(
                    formatter,
                    "rounded rectangle corner radius must be finite, got {value}"
                )
            }
        }
    }
}

impl std::error::Error for RoundedRectangleAuthoringError {}

fn validate_dimensions(width: f32, height: f32) -> Result<(), RoundedRectangleAuthoringError> {
    if !width.is_finite() || width <= 0.0 {
        return Err(RoundedRectangleAuthoringError::InvalidWidth(width));
    }
    if !height.is_finite() || height <= 0.0 {
        return Err(RoundedRectangleAuthoringError::InvalidHeight(height));
    }
    Ok(())
}

#[derive(Clone, Copy, Debug)]
struct CornerCurve {
    start: Vec2,
    control1: Vec2,
    control2: Vec2,
    end: Vec2,
    rounded: bool,
}

pub(crate) fn manim_rounded_rectangle_path(
    width: f32,
    height: f32,
    corner_radii: [f32; 4],
) -> Result<VectorPath, RoundedRectangleAuthoringError> {
    validate_dimensions(width, height)?;
    for radius in corner_radii {
        if !radius.is_finite() {
            return Err(RoundedRectangleAuthoringError::NonFiniteCornerRadius(
                radius,
            ));
        }
    }
    let half_width = width * 0.5;
    let half_height = height * 0.5;
    // Manim Rectangle vertex order after stretching UR/UL/DL/DR to width/height.
    let vertices = [
        Vec2::new(half_width, half_height),
        Vec2::new(-half_width, half_height),
        Vec2::new(-half_width, -half_height),
        Vec2::new(half_width, -half_height),
    ];

    let corners: [CornerCurve; 4] = std::array::from_fn(|index| {
        let previous = vertices[(index + vertices.len() - 1) % vertices.len()];
        let vertex = vertices[index];
        let next = vertices[(index + 1) % vertices.len()];
        rounded_corner(previous, vertex, next, corner_radii[index])
    });

    let mut path = VectorPath::new().move_to(corners[0].start);
    for index in 0..corners.len() {
        let corner = corners[index];
        if corner.rounded {
            path = path.cubic_to(corner.control1, corner.control2, corner.end);
        } else if corner.end != corner.start {
            path = path.line_to(corner.end);
        }
        let next_start = corners[(index + 1) % corners.len()].start;
        if next_start != corner.end {
            path = path.line_to(next_start);
        }
    }
    Ok(path)
}

fn rounded_corner(previous: Vec2, vertex: Vec2, next: Vec2, radius: f32) -> CornerCurve {
    let incoming = vertex - previous;
    let outgoing = next - vertex;
    let incoming_length = incoming.length();
    let outgoing_length = outgoing.length();
    let max_cutoff = incoming_length.min(outgoing_length) * 0.5;
    let cutoff = radius.abs().min(max_cutoff);

    if cutoff <= f32::EPSILON || radius == 0.0 {
        return CornerCurve {
            start: vertex,
            control1: vertex,
            control2: vertex,
            end: vertex,
            rounded: false,
        };
    }

    let incoming_unit = incoming / incoming_length;
    let outgoing_unit = outgoing / outgoing_length;
    let start = vertex - incoming_unit * cutoff;
    let end = vertex + outgoing_unit * cutoff;
    let sweep = std::f32::consts::FRAC_PI_2 * radius.signum();
    let (control1, control2) = cubic_controls_between(start, end, sweep);
    CornerCurve {
        start,
        control1,
        control2,
        end,
        rounded: true,
    }
}

/// Match Manim `ArcBetweenPoints(..., num_components=2)` for one corner.
fn cubic_controls_between(start: Vec2, end: Vec2, sweep: f32) -> (Vec2, Vec2) {
    let base_start = Vec2::new(1.0, 0.0);
    let (end_sin, end_cos) = sweep.sin_cos();
    let base_end = Vec2::new(end_cos, end_sin);
    let base_chord = base_end - base_start;
    let target_chord = end - start;
    let scale = target_chord.length() / base_chord.length();
    let rotation = target_chord.y.atan2(target_chord.x) - base_chord.y.atan2(base_chord.x);
    let transform = |point: Vec2| start + (point - base_start).rotate(rotation) * scale;

    let handle_factor = (4.0 / 3.0) * (sweep / 4.0).tan();
    let base_control1 = base_start + Vec2::new(0.0, 1.0) * handle_factor;
    let end_tangent = Vec2::new(-end_sin, end_cos);
    let base_control2 = base_end - end_tangent * handle_factor;
    (transform(base_control1), transform(base_control2))
}
