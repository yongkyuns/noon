//! Thin static-plot preparation at the optional Python/WASM language boundary.
//! Sample order, subpaths, coordinate mapping and smoothing remain shared Rust.

use noon::{AxesFrame, ManimGeometryOptions, PlotSamplingOptions, PlotSamplingPlan};
use wasm_bindgen::prelude::*;

use crate::authoring_error::{js_error, AuthoringFailure};
use crate::WasmManimGeometryOptions;

pub(crate) fn plot_failure(error: noon::PlotAuthoringError) -> AuthoringFailure {
    match error {
        noon::PlotAuthoringError::Authoring(error) => error.into(),
        noon::PlotAuthoringError::Preparation(error) => sampling_failure(error),
    }
}

fn sampling_failure(error: noon::PlotPreparationError) -> AuthoringFailure {
    use noon::PlotPreparationError::*;
    let category = match &error {
        AllocationFailed | SampleLimitExceeded => "resource_limit",
        SmoothingFailed => "unsupported_operation",
        InvalidRange | InvalidDiscontinuity | InvalidPoint { .. } | SampleCountMismatch { .. } => {
            "invalid_input"
        }
    };
    AuthoringFailure::new(category, "plot.preparation", error)
}

pub(crate) fn coordinate_failure(error: noon::CoordinateAuthoringError) -> AuthoringFailure {
    use noon::CoordinateAuthoringError::*;
    match error {
        Authoring(error) => error.into(),
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

/// Disposable preparation input. It retains no callable, semantic identity or
/// execution owner. A coordinate snapshot, when supplied, is fixed for the whole
/// evaluation, including host functions that perform other authoring work.
#[wasm_bindgen]
pub struct WasmPlotSamplingPlan {
    plan: PlotSamplingPlan,
    frame: Option<AxesFrame>,
}

impl WasmPlotSamplingPlan {
    pub(crate) fn prepare(
        mut options: PlotSamplingOptions,
        discontinuities: Vec<f64>,
        dt: Option<f64>,
        max_samples: Option<u32>,
        frame: Option<AxesFrame>,
    ) -> Result<Self, JsValue> {
        options.discontinuities = discontinuities;
        if let Some(dt) = dt {
            options.dt = dt;
        }
        if let Some(limit) = max_samples {
            options.max_samples = limit as usize;
        }
        Ok(Self {
            plan: options.plan().map_err(sampling_failure).map_err(js_error)?,
            frame,
        })
    }

    fn geometry(
        &self,
        mut points: Vec<[f64; 2]>,
        smooth: bool,
    ) -> Result<WasmManimGeometryOptions, JsValue> {
        if let Some(frame) = self.frame {
            for point in &mut points {
                *point = frame
                    .coords_to_point(point[0], point[1])
                    .map_err(coordinate_math_failure)
                    .map_err(js_error)?;
            }
        }
        ManimGeometryOptions::plot_samples(&self.plan, &points, smooth)
            .map(WasmManimGeometryOptions::from_options)
            .map_err(plot_failure)
            .map_err(js_error)
    }

    fn require_count(&self, actual: usize) -> Result<(), JsValue> {
        let expected = self.plan.parameters().len();
        if actual != expected {
            return Err(js_error(sampling_failure(
                noon::PlotPreparationError::SampleCountMismatch { expected, actual },
            )));
        }
        Ok(())
    }
}

#[wasm_bindgen]
impl WasmPlotSamplingPlan {
    pub fn parametric(
        range: &[f64],
        discontinuities: Vec<f64>,
        dt: Option<f64>,
        max_samples: Option<u32>,
    ) -> Result<Self, JsValue> {
        Self::prepare(
            PlotSamplingOptions::parametric(range)
                .map_err(sampling_failure)
                .map_err(js_error)?,
            discontinuities,
            dt,
            max_samples,
            None,
        )
    }

    /// One language-boundary copy of the already planned evaluation order.
    pub fn parameters(&self) -> Vec<f64> {
        self.plan.parameters().to_vec()
    }

    /// Python evaluates only y values; Rust supplies the corresponding x values.
    #[wasm_bindgen(js_name = functionSamples)]
    pub fn function_samples(
        &self,
        values: &[f64],
        smooth: bool,
    ) -> Result<WasmManimGeometryOptions, JsValue> {
        self.require_count(values.len())?;
        let mut points = Vec::new();
        points.try_reserve_exact(values.len()).map_err(|_| {
            js_error(sampling_failure(
                noon::PlotPreparationError::AllocationFailed,
            ))
        })?;
        points.extend(
            self.plan
                .parameters()
                .iter()
                .zip(values)
                .map(|(&x, &y)| [x, y]),
        );
        self.geometry(points, smooth)
    }

    #[wasm_bindgen(js_name = parametricSamples)]
    pub fn parametric_samples(
        &self,
        values: &[f64],
        smooth: bool,
    ) -> Result<WasmManimGeometryOptions, JsValue> {
        if !values.len().is_multiple_of(2) {
            return Err(js_error(AuthoringFailure::new(
                "invalid_input",
                "plot.point_components",
                "plot samples require x/y component pairs",
            )));
        }
        self.require_count(values.len() / 2)?;
        let mut points = Vec::new();
        points.try_reserve_exact(values.len() / 2).map_err(|_| {
            js_error(sampling_failure(
                noon::PlotPreparationError::AllocationFailed,
            ))
        })?;
        points.extend_from_slice(values.as_chunks::<2>().0);
        self.geometry(points, smooth)
    }
}

pub(crate) fn data_points(values: &[f64]) -> Result<Vec<[f64; 2]>, JsValue> {
    if !values.len().is_multiple_of(2) {
        return Err(js_error(AuthoringFailure::new(
            "invalid_input",
            "plot.point_components",
            "plot samples require x/y component pairs",
        )));
    }
    if values.len() / 2 > noon_geometry::DEFAULT_PLOT_SAMPLE_LIMIT {
        return Err(js_error(sampling_failure(
            noon::PlotPreparationError::SampleLimitExceeded,
        )));
    }
    let mut points = Vec::new();
    points.try_reserve_exact(values.len() / 2).map_err(|_| {
        js_error(sampling_failure(
            noon::PlotPreparationError::AllocationFailed,
        ))
    })?;
    points.extend_from_slice(values.as_chunks::<2>().0);
    Ok(points)
}
