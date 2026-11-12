//! Shared typed error projection for native host tests and WASM plotting.
use crate::integration::AuthoringFailure;

pub fn plot_failure(error: crate::PlotAuthoringError) -> AuthoringFailure {
    match error {
        crate::PlotAuthoringError::Authoring(error) => error.into(),
        crate::PlotAuthoringError::Preparation(error) => sampling_failure(error),
    }
}

pub fn sampling_failure(error: crate::PlotPreparationError) -> AuthoringFailure {
    use crate::PlotPreparationError::*;
    let category = match &error {
        AllocationFailed | SampleLimitExceeded | ImplicitLeafLimitExceeded => "resource_limit",
        SmoothingFailed => "unsupported_operation",
        InvalidRange
        | InvalidDiscontinuity
        | InvalidPoint { .. }
        | SampleCountMismatch { .. }
        | InvalidImplicitOptions
        | InvalidImplicitPoint { .. } => "invalid_input",
    };
    AuthoringFailure::new(category, "plot.preparation", error)
}

pub fn coordinate_failure(error: crate::CoordinateAuthoringError) -> AuthoringFailure {
    use crate::CoordinateAuthoringError::*;
    match error {
        Authoring(error) => error.into(),
        Live(error) => error.into(),
        Plot(error) => plot_failure(error),
        Coordinate(error) => coordinate_math_failure(error),
        InvalidOptions(reason) => {
            AuthoringFailure::new("invalid_input", "coordinate.options", reason)
        }
        InvalidTopology => AuthoringFailure::new(
            "invalid_input",
            "coordinate.topology",
            "coordinate family topology is invalid",
        ),
    }
}

/// Keep the diagnostics of live coordinate authoring at the host boundary.
pub fn live_coordinate_failure(error: crate::CoordinateAuthoringError) -> AuthoringFailure {
    match error {
        crate::CoordinateAuthoringError::Authoring(error) => {
            crate::LiveSessionError::from(error).into()
        }
        other => coordinate_failure(other),
    }
}

/// PolarPlane publishes its prepared transaction directly.
pub fn live_polar_plane_creation_failure(
    error: crate::CoordinateAuthoringError,
) -> AuthoringFailure {
    match error {
        crate::CoordinateAuthoringError::Authoring(
            crate::AuthoringError::ExecutionPublication(error),
        ) => crate::LiveSessionError::Publication(error).into(),
        other => coordinate_failure(other),
    }
}

pub fn coordinate_math_failure(error: crate::CoordinateError) -> AuthoringFailure {
    use crate::CoordinateError::*;
    let category = match &error {
        AllocationFailed | TickLimitExceeded => "resource_limit",
        InvalidRange | InvalidLength | InvalidPoint | DegenerateAxis => "invalid_input",
    };
    AuthoringFailure::new(category, "coordinate.query", error)
}
