//! Thin PolarPlane input and query handles over shared retained Rust semantics.
use noon::{
    ManimPolarPlane, ManimPolarPlaneOptions, PolarAzimuthDirection, PolarFrame, SemanticPaint,
};
use wasm_bindgen::prelude::*;

use crate::authoring_coordinates::{CoordinateRequest, WasmAxesFrame};
use crate::authoring_error::js_error;
use crate::authoring_plotting::{coordinate_failure, coordinate_math_failure};
use crate::{CanonicalAuthoringSceneContext, WasmAuthoringFamilyHandle, WasmCoordinateOptions};

#[wasm_bindgen]
impl WasmCoordinateOptions {
    #[wasm_bindgen(js_name = polarPlane)]
    pub fn polar_plane(
        radius_max: f64,
        size: Option<f64>,
        radius_step: f64,
        azimuth_step: Option<f64>,
        azimuth_offset: f64,
        clockwise: bool,
        faded_line_ratio: f64,
    ) -> Result<Self, JsValue> {
        if !faded_line_ratio.is_finite()
            || faded_line_ratio.fract() != 0.0
            || !(0.0..=u32::MAX as f64).contains(&faded_line_ratio)
        {
            return Err(js_error("faded_line_ratio must be a nonnegative integer"));
        }
        Ok(Self {
            request: CoordinateRequest::PolarPlane(ManimPolarPlaneOptions {
                radius_max,
                size,
                radius_step,
                azimuth_step,
                azimuth_offset,
                azimuth_direction: if clockwise {
                    PolarAzimuthDirection::Clockwise
                } else {
                    PolarAzimuthDirection::Counterclockwise
                },
                faded_line_ratio: faded_line_ratio as u32,
                ..Default::default()
            }),
        })
    }

    #[wasm_bindgen(js_name = setPolarLineStyle)]
    pub fn set_polar_line_style(
        &mut self,
        faded: bool,
        color: &[f64],
        width: Option<f64>,
        opacity: Option<f64>,
    ) -> Result<(), JsValue> {
        let CoordinateRequest::PolarPlane(options) = &mut self.request else {
            return Err(js_error("grid style requires PolarPlane options"));
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
pub struct WasmPolarFrame {
    frame: PolarFrame,
}

#[wasm_bindgen]
impl WasmPolarFrame {
    #[wasm_bindgen(js_name = polarToPoint)]
    pub fn polar_to_point(&self, radius: f64, azimuth: f64) -> Result<Vec<f64>, JsValue> {
        self.frame
            .polar_to_point(radius, azimuth)
            .map(|point| point.to_vec())
            .map_err(coordinate_math_failure)
            .map_err(js_error)
    }
    #[wasm_bindgen(js_name = pointToPolar)]
    pub fn point_to_polar(&self, x: f64, y: f64) -> Result<Vec<f64>, JsValue> {
        self.frame
            .point_to_polar([x, y])
            .map(|point| point.to_vec())
            .map_err(coordinate_math_failure)
            .map_err(js_error)
    }
    #[wasm_bindgen(js_name = axesFrame)]
    pub fn axes_frame(&self) -> WasmAxesFrame {
        WasmAxesFrame {
            frame: self.frame.axes(),
        }
    }
}

#[wasm_bindgen]
impl WasmAuthoringFamilyHandle {
    #[wasm_bindgen(js_name = polarPlanePart)]
    pub fn polar_plane_part(&self, index: u32) -> Result<Self, JsValue> {
        let plane = ManimPolarPlane::from_family(self.semantic_family()?)
            .map_err(coordinate_failure)
            .map_err(js_error)?;
        let family = match index {
            0 => plane.faded_lines(),
            1 => plane.background_lines(),
            2 => plane.x_axis().map(|axis| axis.family().clone()),
            3 => plane.y_axis().map(|axis| axis.family().clone()),
            _ => {
                return Err(js_error(
                    "PolarPlane part index must be between zero and three",
                ))
            }
        }
        .map_err(coordinate_failure)
        .map_err(js_error)?;
        Ok(Self::from_semantic_family(family))
    }
    #[wasm_bindgen(js_name = polarPlaneFrame)]
    pub fn polar_plane_frame(&self) -> Result<WasmPolarFrame, JsValue> {
        ManimPolarPlane::from_family(self.semantic_family()?)
            .and_then(|plane| plane.authored_polar_frame())
            .map(|frame| WasmPolarFrame { frame })
            .map_err(coordinate_failure)
            .map_err(js_error)
    }
}

#[wasm_bindgen]
impl CanonicalAuthoringSceneContext {
    #[wasm_bindgen(js_name = queryPolarPlaneFrame)]
    pub fn query_polar_plane_frame(
        &mut self,
        handle: &WasmAuthoringFamilyHandle,
    ) -> Result<WasmPolarFrame, JsValue> {
        let plane = ManimPolarPlane::from_family(handle.semantic_family()?)
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
        let axes = noon::AxesFrame::new(
            self.coordinate_line_frame(&x)?,
            self.coordinate_line_frame(&y)?,
        );
        Ok(WasmPolarFrame {
            frame: PolarFrame::new(axes),
        })
    }
}

/// Native and direct Rust/WASM render the same retained polar scene builder.
#[cfg(all(
    feature = "renderer",
    any(debug_assertions, feature = "renderer-smoke")
))]
#[wasm_bindgen(js_name = createPolarPlaneRenderer)]
pub async fn create_polar_plane_renderer(
    canvas: web_sys::OffscreenCanvas,
) -> Result<crate::WasmExecutionCanvasRenderer, JsValue> {
    let session = noon::example_scenes::polar_plane::session().map_err(js_error)?;
    crate::WasmExecutionCanvasRenderer::create_from_execution_session(canvas, session).await
}
