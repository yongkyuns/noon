//! Thin static-plot preparation at the optional Python/WASM language boundary.
//! Sample order, subpaths, coordinate mapping and smoothing remain shared Rust.

use noon::{AxesFrame, ManimGeometryOptions, PlotSamplingOptions, PlotSamplingPlan};
use wasm_bindgen::prelude::*;

use crate::authoring_error::{js_error, AuthoringFailure};
use crate::WasmManimGeometryOptions;

use crate::plot_error::sampling_failure;
pub(crate) use crate::plot_error::{coordinate_failure, coordinate_math_failure, plot_failure};

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
        let mut options = ManimGeometryOptions::plot_samples(&self.plan, &points, smooth)
            .map_err(plot_failure)
            .map_err(js_error)?;
        if self.frame.is_some() {
            let range = self.plan.range();
            options.set_semantic_role(noon_core::SemanticObjectRole::FunctionPlot(
                noon_core::SemanticFunctionPlotRole::new([range[0], range[1]]),
            ));
        }
        Ok(WasmManimGeometryOptions::from_options(options))
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
