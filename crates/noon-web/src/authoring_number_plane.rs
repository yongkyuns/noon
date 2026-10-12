//! Thin NumberPlane options and handle observations over shared coordinate semantics.
use noon::{ManimNumberPlane, ManimNumberPlaneOptions, SemanticPaint};
use wasm_bindgen::prelude::*;

use crate::authoring_coordinates::{CoordinateRequest, WasmAxesFrame};
use crate::authoring_error::js_error;
use crate::authoring_plotting::coordinate_failure;
use crate::{CanonicalAuthoringSceneContext, WasmAuthoringFamilyHandle, WasmCoordinateOptions};

#[wasm_bindgen]
impl WasmCoordinateOptions {
    #[wasm_bindgen(js_name = numberPlane)]
    pub fn number_plane(
        x_range: &[f64],
        y_range: &[f64],
        x_length: Option<f64>,
        y_length: Option<f64>,
        faded_line_ratio: f64,
    ) -> Result<Self, JsValue> {
        let mut options = ManimNumberPlaneOptions::default();
        // Empty means omitted. Normalize two-value ranges in shared Rust input,
        // never in the Python facade's coordinate calculations.
        let range = |values: &[f64], default| match values {
            [] => Ok(default),
            [start, end] => Ok([*start, *end, 1.0]),
            [start, end, step] => Ok([*start, *end, *step]),
            _ => Err(js_error("NumberPlane range requires two or three values")),
        };
        options.x_range = range(x_range, options.x_range)?;
        options.y_range = range(y_range, options.y_range)?;
        options.x_length = x_length;
        options.y_length = y_length;
        if !faded_line_ratio.is_finite()
            || faded_line_ratio.fract() != 0.0
            || !(0.0..=u32::MAX as f64).contains(&faded_line_ratio)
        {
            return Err(js_error("faded_line_ratio must be a nonnegative integer"));
        }
        options.faded_line_ratio = faded_line_ratio as u32;
        Ok(Self {
            request: CoordinateRequest::NumberPlane(options),
        })
    }

    /// Partial style dictionary, supplied once before atomic construction.
    #[wasm_bindgen(js_name = setPlaneLineStyle)]
    pub fn set_plane_line_style(
        &mut self,
        faded: bool,
        color: &[f64],
        width: Option<f64>,
        opacity: Option<f64>,
    ) -> Result<(), JsValue> {
        let CoordinateRequest::NumberPlane(options) = &mut self.request else {
            return Err(js_error("grid style requires NumberPlane options"));
        };
        let style = if faded {
            options.faded_line_style_mut()
        } else {
            &mut options.background_line_style
        };
        if !color.is_empty() {
            let [r, g, b, a] = color else {
                return Err(js_error("stroke color requires RGBA"));
            };
            style.stroke = crate::authoring_mobject::family_color(true, *r, *g, *b, *a)
                .map_err(js_error)?
                .map(SemanticPaint::Solid);
        }
        if let Some(value) = width {
            style.stroke_width = value;
        }
        if let Some(value) = opacity {
            style.stroke_opacity = value;
        }
        Ok(())
    }
}

#[wasm_bindgen]
impl WasmAuthoringFamilyHandle {
    /// Return authoritative root members in their painter order, without new IDs.
    #[wasm_bindgen(js_name = numberPlanePart)]
    pub fn number_plane_part(&self, index: u32) -> Result<Self, JsValue> {
        let plane = ManimNumberPlane::from_family(self.semantic_family()?)
            .map_err(coordinate_failure)
            .map_err(js_error)?;
        let family = match index {
            0 => plane.faded_lines(),
            1 => plane.background_lines(),
            2 => plane.x_axis().map(|axis| axis.family().clone()),
            3 => plane.y_axis().map(|axis| axis.family().clone()),
            _ => {
                return Err(js_error(
                    "NumberPlane part index must be between zero and three",
                ))
            }
        }
        .map_err(coordinate_failure)
        .map_err(js_error)?;
        Ok(Self::from_semantic_family(family))
    }

    #[wasm_bindgen(js_name = numberPlaneFrame)]
    pub fn number_plane_frame(&self) -> Result<WasmAxesFrame, JsValue> {
        ManimNumberPlane::from_family(self.semantic_family()?)
            .and_then(|plane| plane.authored_frame())
            .map(|frame| WasmAxesFrame { frame })
            .map_err(coordinate_failure)
            .map_err(js_error)
    }
}

#[wasm_bindgen]
impl CanonicalAuthoringSceneContext {
    #[wasm_bindgen(js_name = queryNumberPlaneFrame)]
    pub fn query_number_plane_frame(
        &mut self,
        handle: &WasmAuthoringFamilyHandle,
    ) -> Result<WasmAxesFrame, JsValue> {
        let plane = ManimNumberPlane::from_family(handle.semantic_family()?)
            .map_err(coordinate_failure)
            .map_err(js_error)?;
        let x = plane
            .x_axis()
            .map_err(coordinate_failure)
            .map_err(js_error)?;
        let y = plane
            .y_axis()
            .map_err(coordinate_failure)
            .map_err(js_error)?;
        Ok(WasmAxesFrame {
            frame: noon::AxesFrame::new(
                self.coordinate_line_frame(&x)?,
                self.coordinate_line_frame(&y)?,
            ),
        })
    }
}

/// Native and single-context WASM run the same typed Rust scene builder.
#[cfg(all(
    feature = "renderer",
    any(debug_assertions, feature = "renderer-smoke")
))]
#[wasm_bindgen(js_name = createNumberPlaneRenderer)]
pub async fn create_number_plane_renderer(
    canvas: web_sys::OffscreenCanvas,
) -> Result<crate::WasmExecutionCanvasRenderer, JsValue> {
    let session = noon::example_scenes::number_plane::session().map_err(js_error)?;
    crate::WasmExecutionCanvasRenderer::create_from_execution_session(canvas, session).await
}
