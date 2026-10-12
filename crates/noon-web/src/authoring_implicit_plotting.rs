//! Authoring-only scalar callbacks over the shared adaptive contour planner.
use noon::{AxesFrame, ImplicitPlotOptions, ManimGeometryOptions};
use wasm_bindgen::prelude::*;

use crate::authoring_coordinates::WasmAxesFrame;
use crate::authoring_error::{js_error, AuthoringFailure};
use crate::plot_error::{coordinate_failure, plot_failure};
use crate::WasmManimGeometryOptions;

fn invalid(message: &str) -> JsValue {
    js_error(AuthoringFailure::new(
        "invalid_input",
        "plot.implicit",
        message,
    ))
}

fn integer(value: f64, name: &str) -> Result<u32, JsValue> {
    if !value.is_finite() || value.fract() != 0.0 || !(0.0..=u32::MAX as f64).contains(&value) {
        return Err(invalid(&format!("{name} must be a nonnegative integer")));
    }
    Ok(value as u32)
}

fn options(
    x_range: &[f64],
    y_range: &[f64],
    min_depth: f64,
    max_quads: f64,
    smooth: bool,
) -> Result<ImplicitPlotOptions, JsValue> {
    let mut result = ImplicitPlotOptions::default();
    let range = |values: &[f64], default| match values {
        [] => Ok(default),
        [start, end] => Ok([*start, *end]),
        _ => Err(invalid("implicit curve range requires two values")),
    };
    let x = range(x_range, [result.bounds.min.x, result.bounds.max.x])?;
    let y = range(y_range, [result.bounds.min.y, result.bounds.max.y])?;
    result.bounds.min = noon_geometry::IsolinePoint::new(x[0], y[0]);
    result.bounds.max = noon_geometry::IsolinePoint::new(x[1], y[1]);
    result.contour.min_depth = integer(min_depth, "min_depth")?;
    result.contour.max_quads = integer(max_quads, "max_quads")? as usize;
    result.use_smoothing = smooth;
    Ok(result)
}

fn prepare(
    function: &js_sys::Function,
    options: &ImplicitPlotOptions,
    frame: Option<AxesFrame>,
) -> Result<WasmManimGeometryOptions, JsValue> {
    let mut failure = None;
    let mut sample = |x: f64, y: f64| {
        // Preserve the first host exception. Remaining bounded preparation may
        // unwind through NaN cells, but never invokes a failed callable again.
        if failure.is_some() {
            return f64::NAN;
        }
        match function.call2(&JsValue::UNDEFINED, &x.into(), &y.into()) {
            Ok(value) => match value.as_f64() {
                Some(value) => value,
                None => {
                    failure = Some(invalid("implicit callback must return a scalar number"));
                    f64::NAN
                }
            },
            Err(error) => {
                failure = Some(error);
                f64::NAN
            }
        }
    };
    let geometry = if let Some(frame) = frame {
        ManimGeometryOptions::axes_implicit_plot(frame, options, &mut sample)
            .map_err(coordinate_failure)
    } else {
        ManimGeometryOptions::implicit_plot(options, &mut sample).map_err(plot_failure)
    };
    if let Some(error) = failure {
        return Err(error);
    }
    geometry
        .map(WasmManimGeometryOptions::from_options)
        .map_err(js_error)
}

#[wasm_bindgen]
impl WasmManimGeometryOptions {
    #[wasm_bindgen(js_name = implicitPlot)]
    pub fn implicit_plot(
        function: &js_sys::Function,
        x_range: &[f64],
        y_range: &[f64],
        min_depth: f64,
        max_quads: f64,
        use_smoothing: bool,
    ) -> Result<Self, JsValue> {
        prepare(
            function,
            &options(x_range, y_range, min_depth, max_quads, use_smoothing)?,
            None,
        )
    }
}

#[wasm_bindgen]
impl WasmAxesFrame {
    #[wasm_bindgen(js_name = implicitPlot)]
    pub fn implicit_plot(
        &self,
        function: &js_sys::Function,
        min_depth: f64,
        max_quads: f64,
        use_smoothing: bool,
    ) -> Result<WasmManimGeometryOptions, JsValue> {
        let x = self.frame.x().range();
        let y = self.frame.y().range();
        prepare(
            function,
            &options(&x[..2], &y[..2], min_depth, max_quads, use_smoothing)?,
            Some(self.frame),
        )
    }
}

#[cfg(all(
    feature = "renderer",
    any(debug_assertions, feature = "renderer-smoke")
))]
#[wasm_bindgen(js_name = createImplicitPlottingRenderer)]
pub async fn create_implicit_plotting_renderer(
    canvas: web_sys::OffscreenCanvas,
) -> Result<crate::WasmExecutionCanvasRenderer, JsValue> {
    let session = noon::example_scenes::implicit_plotting::session().map_err(js_error)?;
    crate::WasmExecutionCanvasRenderer::create_from_execution_session(canvas, session).await
}
