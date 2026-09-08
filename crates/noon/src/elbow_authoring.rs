//! Pure retained path construction used by shared Elbow handles.
use noon_core::{Vec2, VectorPath};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ElbowAuthoringError {
    NonFiniteWidth(f32),
    NonFiniteAngle(f32),
}

impl std::fmt::Display for ElbowAuthoringError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NonFiniteWidth(width) => {
                write!(formatter, "elbow width must be finite, got {width}")
            }
            Self::NonFiniteAngle(angle) => {
                write!(formatter, "elbow angle must be finite, got {angle}")
            }
        }
    }
}

impl std::error::Error for ElbowAuthoringError {}

pub(crate) fn manim_elbow_path(width: f32, angle: f32) -> Result<VectorPath, ElbowAuthoringError> {
    if !width.is_finite() {
        return Err(ElbowAuthoringError::NonFiniteWidth(width));
    }
    if !angle.is_finite() {
        return Err(ElbowAuthoringError::NonFiniteAngle(angle));
    }

    let (sin, cos) = angle.sin_cos();
    let rotate =
        |point: Vec2| Vec2::new(point.x * cos - point.y * sin, point.x * sin + point.y * cos);
    let path = VectorPath::new()
        .move_to(rotate(Vec2::new(0.0, width)))
        .line_to(rotate(Vec2::new(width, width)))
        .line_to(rotate(Vec2::new(width, 0.0)));
    Ok(path)
}
