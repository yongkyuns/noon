//! Authored primary-click actions, independent of platform collectors.

use crate::Color;

/// A self-targeting action on one semantic object. The declaration owns options,
/// never an operation token, elapsed time, captured geometry, or selection state.
/// Supported actions use the normal shared semantic compiler and runtime.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SemanticPointerClickAction {
    /// Temporarily indicate this object and release the operation at its source.
    Indicate { scale_factor: f64, color: Color, run_time: f64 },
}

impl SemanticPointerClickAction {
    pub const fn indicate(scale_factor: f64, color: Color, run_time: f64) -> Self {
        Self::Indicate { scale_factor, color, run_time }
    }

    /// Check every supplied declaration before publication, including no-ops.
    pub fn is_valid(self) -> bool {
        match self {
            Self::Indicate { scale_factor, color, run_time } => {
                scale_factor.is_finite() && scale_factor >= 0.0
                    && scale_factor <= f64::from(f32::MAX)
                    && [color.red, color.green, color.blue, color.alpha]
                        .into_iter().all(f32::is_finite)
                    && run_time.is_finite() && run_time > 0.0
            }
        }
    }
}

impl Default for SemanticPointerClickAction {
    fn default() -> Self {
        Self::indicate(1.2, crate::YELLOW, 1.0)
    }
}
