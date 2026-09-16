//! Coordinate-family wrappers. Geometry and numerical rules remain in `noon`.

use std::rc::Rc;

use noon::{
    AxesFrame, CoordinateTicks, ManimAxes, ManimAxesOptions, ManimNumberLine,
    ManimNumberLineOptions, NumberLineFrame, PlotSamplingOptions,
};
use wasm_bindgen::prelude::*;

use crate::authoring_error::{js_error, AuthoringFailure};
use crate::authoring_plotting::{coordinate_failure, coordinate_math_failure, data_points};
use crate::{
    CanonicalAuthoringSceneContext, WasmAuthoringFamilyHandle, WasmAuthoringMobjectHandle,
    WasmAuthoringStore, WasmManimGeometryOptions, WasmPlotSamplingPlan,
};

/// Inert input only. No family or shaft identity exists until `createCoordinates`.
#[wasm_bindgen]
pub struct WasmCoordinateOptions {
    request: CoordinateRequest,
}

enum CoordinateRequest {
    NumberLine(ManimNumberLineOptions),
    Axes(ManimAxesOptions),
}

fn range3(values: &[f64]) -> Result<[f64; 3], JsValue> {
    let [start, end, step] = values else {
        return Err(js_error(AuthoringFailure::new(
            "invalid_input",
            "coordinate.range_shape",
            "coordinate range requires exactly three values",
        )));
    };
    let range = [*start, *end, *step];
    noon_geometry::validate_coordinate_range(range)
        .map_err(coordinate_math_failure)
        .map_err(js_error)?;
    Ok(range)
}

impl WasmCoordinateOptions {
    fn style_mut(&mut self) -> &mut noon::SemanticStyle {
        match &mut self.request {
            CoordinateRequest::NumberLine(options) => &mut options.style,
            CoordinateRequest::Axes(options) => &mut options.style,
        }
    }
}

#[wasm_bindgen]
impl WasmCoordinateOptions {
    #[wasm_bindgen(js_name = numberLine)]
    pub fn number_line(
        range: &[f64],
        length: Option<f64>,
        unit_size: f64,
        rotation: f64,
    ) -> Result<Self, JsValue> {
        let mut options = ManimNumberLineOptions::new(range3(range)?);
        options.length = length;
        options.unit_size = unit_size;
        options.rotation = rotation;
        Ok(Self {
            request: CoordinateRequest::NumberLine(options),
        })
    }

    pub fn axes(
        x_range: &[f64],
        y_range: &[f64],
        x_length: f64,
        y_length: f64,
    ) -> Result<Self, JsValue> {
        Ok(Self {
            request: CoordinateRequest::Axes(ManimAxesOptions::new(
                range3(x_range)?,
                range3(y_range)?,
                x_length,
                y_length,
            )),
        })
    }

    #[wasm_bindgen(js_name = setTicks)]
    pub fn set_ticks(&mut self, enabled: bool, half_length: f64, exclude_origin: bool) {
        let ticks = match &mut self.request {
            CoordinateRequest::NumberLine(options) => &mut options.ticks,
            CoordinateRequest::Axes(options) => &mut options.ticks,
        };
        *ticks = CoordinateTicks {
            enabled,
            half_length,
            exclude_origin,
            ..*ticks
        };
    }

    #[wasm_bindgen(js_name = setColor)]
    pub fn set_color(&mut self, red: f64, green: f64, blue: f64, alpha: f64) -> Result<(), JsValue> {
        let color = crate::authoring_mobject::family_color(true, red, green, blue, alpha)
            .map_err(|error| js_error(AuthoringFailure::new("invalid_input", "coordinate.color", error)))?
            .expect("enabled color");
        self.style_mut().stroke = Some(noon::SemanticPaint::Solid(color));
        Ok(())
    }

    #[wasm_bindgen(js_name = setStrokeWidth)]
    pub fn set_stroke_width(&mut self, width: f64) {
        self.style_mut().stroke_width = width;
    }

    #[wasm_bindgen(js_name = setStrokeOpacity)]
    pub fn set_stroke_opacity(&mut self, opacity: f64) {
        self.style_mut().stroke_opacity = opacity;
    }

    #[wasm_bindgen(js_name = setObjectOpacity)]
    pub fn set_object_opacity(&mut self, opacity: f64) {
        self.style_mut().object_opacity = opacity;
    }
}

#[wasm_bindgen]
impl WasmAuthoringStore {
    /// Cold authoring only, matching the store's other direct constructors.
    /// The Python facade refuses this route once its continuation has a player.
    #[wasm_bindgen(js_name = createCoordinates)]
    pub fn create_coordinates(
        &self,
        candidate: WasmCoordinateOptions,
    ) -> Result<WasmAuthoringFamilyHandle, JsValue> {
        let family = match candidate.request {
            CoordinateRequest::NumberLine(options) => {
                ManimNumberLine::create(Rc::clone(&self.semantics), &options)
                    .map(|line| line.family().clone())
            }
            CoordinateRequest::Axes(options) => {
                ManimAxes::create(Rc::clone(&self.semantics), &options)
                    .map(|axes| axes.family().clone())
            }
        };
        family
            .map(WasmAuthoringFamilyHandle::from_semantic_family)
            .map_err(coordinate_failure)
            .map_err(js_error)
    }
}

#[wasm_bindgen]
pub struct WasmNumberLineFrame {
    frame: NumberLineFrame,
}

#[wasm_bindgen]
impl WasmNumberLineFrame {
    pub fn range(&self) -> Vec<f64> {
        self.frame.range().to_vec()
    }

    #[wasm_bindgen(js_name = unitSize)]
    pub fn unit_size(&self) -> f64 {
        self.frame.unit_size()
    }

    #[wasm_bindgen(js_name = numberToPoint)]
    pub fn number_to_point(&self, value: f64) -> Result<Vec<f64>, JsValue> {
        self.frame
            .number_to_point(value)
            .map(|point| point.to_vec())
            .map_err(coordinate_math_failure)
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = pointToNumber)]
    pub fn point_to_number(&self, x: f64, y: f64) -> Result<f64, JsValue> {
        self.frame
            .point_to_number([x, y])
            .map_err(coordinate_math_failure)
            .map_err(js_error)
    }
}

#[wasm_bindgen]
pub struct WasmAxesFrame {
    frame: AxesFrame,
}

#[wasm_bindgen]
impl WasmAxesFrame {
    #[wasm_bindgen(js_name = coordsToPoint)]
    pub fn coords_to_point(&self, x: f64, y: f64) -> Result<Vec<f64>, JsValue> {
        self.frame
            .coords_to_point(x, y)
            .map(|point| point.to_vec())
            .map_err(coordinate_math_failure)
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = pointToCoords)]
    pub fn point_to_coords(&self, x: f64, y: f64) -> Result<Vec<f64>, JsValue> {
        self.frame
            .point_to_coords([x, y])
            .map(|point| point.to_vec())
            .map_err(coordinate_math_failure)
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = plotPlan)]
    pub fn plot_plan(
        &self,
        range: &[f64],
        discontinuities: Vec<f64>,
        dt: Option<f64>,
        max_samples: Option<u32>,
    ) -> Result<WasmPlotSamplingPlan, JsValue> {
        let options = PlotSamplingOptions::axes(
            self.frame.x().range(),
            if range.is_empty() { None } else { Some(range) },
        )
        .map_err(noon::PlotAuthoringError::from)
        .map_err(crate::authoring_plotting::plot_failure)
        .map_err(js_error)?;
        WasmPlotSamplingPlan::prepare(options, discontinuities, dt, max_samples, Some(self.frame))
    }

    #[wasm_bindgen(js_name = sampledPlot)]
    pub fn sampled_plot(&self, values: &[f64]) -> Result<WasmManimGeometryOptions, JsValue> {
        noon::ManimGeometryOptions::axes_sampled_plot(self.frame, &data_points(values)?)
            .map(WasmManimGeometryOptions::from_options)
            .map_err(coordinate_failure)
            .map_err(js_error)
    }
}

#[wasm_bindgen]
impl WasmAuthoringFamilyHandle {
    /// Only wrapper identities are reconstructed; ranges remain on the shafts.
    #[wasm_bindgen(js_name = coordinateAxis)]
    pub fn coordinate_axis(&self, index: u32) -> Result<WasmAuthoringFamilyHandle, JsValue> {
        let axes = ManimAxes::from_family(self.semantic_family()?)
            .map_err(coordinate_failure)
            .map_err(js_error)?;
        let axis = match index {
            0 => axes.x_axis(),
            1 => axes.y_axis(),
            _ => return Err(js_error("coordinate axis index must be zero or one")),
        }
        .map_err(coordinate_failure)
        .map_err(js_error)?;
        Ok(Self::from_semantic_family(axis.family().clone()))
    }

    #[wasm_bindgen(js_name = coordinateShaft)]
    pub fn coordinate_shaft(&self) -> Result<WasmAuthoringMobjectHandle, JsValue> {
        ManimNumberLine::from_family(self.semantic_family()?)
            .and_then(|line| line.shaft())
            .map(WasmAuthoringMobjectHandle::from_semantic_mobject)
            .map_err(coordinate_failure)
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = coordinateTicks)]
    pub fn coordinate_ticks(&self) -> Result<WasmAuthoringFamilyHandle, JsValue> {
        ManimNumberLine::from_family(self.semantic_family()?)
            .and_then(|line| line.ticks())
            .map(Self::from_semantic_family)
            .map_err(coordinate_failure)
            .map_err(js_error)
    }

    /// Constructor-only wrapper materialization, bounded by this line's ticks.
    #[wasm_bindgen(js_name = coordinateTickObjects)]
    pub fn coordinate_tick_objects(&self) -> Result<js_sys::Array, JsValue> {
        let ticks = ManimNumberLine::from_family(self.semantic_family()?)
            .and_then(|line| line.ticks())
            .map_err(coordinate_failure)
            .map_err(js_error)?;
        let members = ticks
            .integration_store()
            .borrow()
            .semantic_family_members_checked(ticks.node_id())
            .map_err(js_error)?;
        let result = js_sys::Array::new();
        for member in members {
            let object = noon::Mobject::from_node(Rc::clone(ticks.integration_store()), member)
                .map_err(js_error)?;
            result.push(&WasmAuthoringMobjectHandle::from_semantic_mobject(object).into());
        }
        Ok(result)
    }

    #[wasm_bindgen(js_name = numberLineFrame)]
    pub fn number_line_frame(&self) -> Result<WasmNumberLineFrame, JsValue> {
        ManimNumberLine::from_family(self.semantic_family()?)
            .and_then(|line| line.authored_frame())
            .map(|frame| WasmNumberLineFrame { frame })
            .map_err(coordinate_failure)
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = axesFrame)]
    pub fn axes_frame(&self) -> Result<WasmAxesFrame, JsValue> {
        ManimAxes::from_family(self.semantic_family()?)
            .and_then(|axes| axes.authored_frame())
            .map(|frame| WasmAxesFrame { frame })
            .map_err(coordinate_failure)
            .map_err(js_error)
    }
}

impl CanonicalAuthoringSceneContext {
    fn coordinate_line_frame(&mut self, line: &ManimNumberLine) -> Result<NumberLineFrame, JsValue> {
        let range = line.range().map_err(coordinate_failure).map_err(js_error)?;
        let shaft = line.shaft().map_err(coordinate_failure).map_err(js_error)?;
        let path = self.query_mobject_path(&WasmAuthoringMobjectHandle::from_semantic_mobject(shaft))?;
        let start = path.start()?;
        let end = path.end()?;
        NumberLineFrame::new(range, [start[0], start[1]], [end[0], end[1]])
            .map_err(coordinate_math_failure)
            .map_err(js_error)
    }
}

#[wasm_bindgen]
impl CanonicalAuthoringSceneContext {
    #[wasm_bindgen(js_name = queryNumberLineFrame)]
    pub fn query_number_line_frame(
        &mut self,
        handle: &WasmAuthoringFamilyHandle,
    ) -> Result<WasmNumberLineFrame, JsValue> {
        let line = ManimNumberLine::from_family(handle.semantic_family()?)
            .map_err(coordinate_failure)
            .map_err(js_error)?;
        Ok(WasmNumberLineFrame {
            frame: self.coordinate_line_frame(&line)?,
        })
    }

    /// Acquire both operands inside one Rust call. No JS/Python callback, yield,
    /// frame advancement or publication can interleave the X and Y observations.
    #[wasm_bindgen(js_name = queryAxesFrame)]
    pub fn query_axes_frame(
        &mut self,
        handle: &WasmAuthoringFamilyHandle,
    ) -> Result<WasmAxesFrame, JsValue> {
        let axes = ManimAxes::from_family(handle.semantic_family()?)
            .map_err(coordinate_failure)
            .map_err(js_error)?;
        let x = axes.x_axis().map_err(coordinate_failure).map_err(js_error)?;
        let y = axes.y_axis().map_err(coordinate_failure).map_err(js_error)?;
        Ok(WasmAxesFrame {
            frame: AxesFrame::new(self.coordinate_line_frame(&x)?, self.coordinate_line_frame(&y)?),
        })
    }
}
