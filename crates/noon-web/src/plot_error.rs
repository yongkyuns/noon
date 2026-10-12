//! Shared typed error projection for native host tests and WASM plotting.
use crate::authoring_error::AuthoringFailure;

pub(crate) fn plot_failure(error: noon::PlotAuthoringError) -> AuthoringFailure {
    match error {
        noon::PlotAuthoringError::Authoring(error) => error.into(),
        noon::PlotAuthoringError::Preparation(error) => sampling_failure(error),
    }
}

pub(crate) fn sampling_failure(error: noon::PlotPreparationError) -> AuthoringFailure {
    use noon::PlotPreparationError::*;
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

pub(crate) fn coordinate_failure(error: noon::CoordinateAuthoringError) -> AuthoringFailure {
    use noon::CoordinateAuthoringError::*;
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

pub(crate) fn coordinate_math_failure(error: noon::CoordinateError) -> AuthoringFailure {
    use noon::CoordinateError::*;
    let category = match &error {
        AllocationFailed | TickLimitExceeded => "resource_limit",
        InvalidRange | InvalidLength | InvalidPoint | DegenerateAxis => "invalid_input",
    };
    AuthoringFailure::new(category, "coordinate.query", error)
}
