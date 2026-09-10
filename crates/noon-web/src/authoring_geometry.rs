//! Inert typed geometry inputs for optional language wrappers.
#![cfg(target_arch = "wasm32")]

use noon_core::{Vec2, VectorPath};
use wasm_bindgen::prelude::*;

use crate::authoring_error::js_error;

/// Constructor values own no semantic store, identity, or execution state.
#[wasm_bindgen]
pub struct WasmManimGeometryOptions {
    pub(crate) options: noon::ManimGeometryOptions,
}

#[wasm_bindgen]
impl WasmManimGeometryOptions {
    #[wasm_bindgen(js_name = setZIndex)]
    pub fn set_z_index(&mut self, value: f64) -> Result<(), JsValue> {
        self.options.set_z_index(value).map_err(js_error)
    }

    #[wasm_bindgen(js_name = setTranslation)]
    pub fn set_translation(&mut self, x: f64, y: f64) -> Result<(), JsValue> {
        self.options.set_translation(x, y).map_err(js_error)
    }

    #[wasm_bindgen(js_name = setScale)]
    pub fn set_scale(&mut self, x: f64, y: f64) -> Result<(), JsValue> {
        self.options.set_scale(x, y).map_err(js_error)
    }

    #[wasm_bindgen(js_name = scaleBy)]
    pub fn scale_by(&mut self, x: f64, y: f64) -> Result<(), JsValue> {
        self.options.scale_by(x, y).map_err(js_error)
    }

    #[wasm_bindgen(js_name = setRotation)]
    pub fn set_rotation(&mut self, angle: f64) -> Result<(), JsValue> {
        self.options.set_rotation(angle).map_err(js_error)
    }

    #[wasm_bindgen(js_name = setColor)]
    pub fn set_color(
        &mut self,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
    ) -> Result<(), JsValue> {
        self.options
            .set_color(red, green, blue, alpha)
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = disableFill)]
    pub fn disable_fill(&mut self) {
        self.options.disable_fill();
    }

    #[wasm_bindgen(js_name = setFill)]
    pub fn set_fill(
        &mut self,
        red: f64,
        green: f64,
        blue: f64,
        opacity: f64,
    ) -> Result<(), JsValue> {
        self.options
            .set_fill(red, green, blue, opacity)
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = setFillColor)]
    pub fn set_fill_color(
        &mut self,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
    ) -> Result<(), JsValue> {
        self.options
            .set_fill_color(red, green, blue, alpha)
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = setFillOpacity)]
    pub fn set_fill_opacity(&mut self, opacity: f64) -> Result<(), JsValue> {
        self.options.set_fill_opacity(opacity).map_err(js_error)
    }

    #[wasm_bindgen(js_name = disableStroke)]
    pub fn disable_stroke(&mut self) {
        self.options.disable_stroke();
    }

    #[wasm_bindgen(js_name = setStroke)]
    pub fn set_stroke(
        &mut self,
        red: f64,
        green: f64,
        blue: f64,
        opacity: f64,
    ) -> Result<(), JsValue> {
        self.options
            .set_stroke(red, green, blue, opacity)
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = setStrokeColor)]
    pub fn set_stroke_color(
        &mut self,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
    ) -> Result<(), JsValue> {
        self.options
            .set_stroke_color(red, green, blue, alpha)
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = setStrokeOpacity)]
    pub fn set_stroke_opacity(&mut self, opacity: f64) -> Result<(), JsValue> {
        self.options.set_stroke_opacity(opacity).map_err(js_error)
    }

    #[wasm_bindgen(js_name = setStrokeWidth)]
    pub fn set_stroke_width(&mut self, width: f64) -> Result<(), JsValue> {
        self.options.set_stroke_width(width).map_err(js_error)
    }

    #[wasm_bindgen(js_name = setStrokeWidthMode)]
    pub fn set_stroke_width_mode(&mut self, mode: &str) -> Result<(), JsValue> {
        self.options.set_stroke_width_mode(mode).map_err(js_error)
    }

    #[wasm_bindgen(js_name = setStrokeJoin)]
    pub fn set_stroke_join(&mut self, join: &str) -> Result<(), JsValue> {
        self.options.set_stroke_join(join).map_err(js_error)
    }

    #[wasm_bindgen(js_name = setStrokeCap)]
    pub fn set_stroke_cap(&mut self, cap: &str) -> Result<(), JsValue> {
        self.options.set_stroke_cap(cap).map_err(js_error)
    }

    #[wasm_bindgen(js_name = setObjectOpacity)]
    pub fn set_object_opacity(&mut self, opacity: f64) -> Result<(), JsValue> {
        self.options.set_object_opacity(opacity).map_err(js_error)
    }
}

impl WasmManimGeometryOptions {
    pub(crate) fn from_options(options: noon::ManimGeometryOptions) -> Self {
        Self { options }
    }
}

#[wasm_bindgen]
impl WasmManimGeometryOptions {
    pub fn circle(radius: f64) -> Result<Self, JsValue> {
        noon::ManimGeometryOptions::circle(radius)
            .map(Self::from_options)
            .map_err(js_error)
    }

    pub fn ellipse(width: f64, height: f64) -> Result<Self, JsValue> {
        noon::ManimGeometryOptions::ellipse(width, height)
            .map(Self::from_options)
            .map_err(js_error)
    }

    pub fn square(side: f64) -> Result<Self, JsValue> {
        noon::ManimGeometryOptions::square(side)
            .map(Self::from_options)
            .map_err(js_error)
    }

    pub fn rectangle(width: f64, height: f64) -> Result<Self, JsValue> {
        noon::ManimGeometryOptions::rectangle(width, height)
            .map(Self::from_options)
            .map_err(js_error)
    }

    pub fn line(start_x: f64, start_y: f64, end_x: f64, end_y: f64) -> Result<Self, JsValue> {
        noon::ManimGeometryOptions::line(start_x, start_y, end_x, end_y)
            .map(Self::from_options)
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = dot)]
    pub fn dot(x: f64, y: f64, radius: f64) -> Result<Self, JsValue> {
        noon::ManimGeometryOptions::dot(x, y, radius)
            .map(Self::from_options)
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = triangle)]
    pub fn triangle() -> Result<Self, JsValue> {
        noon::ManimGeometryOptions::triangle()
            .map(Self::from_options)
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = elbow)]
    pub fn elbow(width: f64, angle: f64) -> Result<Self, JsValue> {
        noon::ManimGeometryOptions::elbow(width, angle)
            .map(Self::from_options)
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = roundedRectangle)]
    pub fn rounded_rectangle(width: f64, height: f64, corner_radius: f64) -> Result<Self, JsValue> {
        noon::ManimGeometryOptions::rounded_rectangle(width, height, corner_radius)
            .map(Self::from_options)
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = annularSector)]
    pub fn annular_sector(
        inner_radius: f64,
        outer_radius: f64,
        angle: f64,
        start_angle: f64,
        num_components: u32,
        center_x: f64,
        center_y: f64,
    ) -> Result<Self, JsValue> {
        noon::ManimGeometryOptions::annular_sector(
            inner_radius,
            outer_radius,
            angle,
            start_angle,
            num_components,
            center_x,
            center_y,
        )
        .map(Self::from_options)
        .map_err(js_error)
    }

    #[wasm_bindgen(js_name = sector)]
    pub fn sector(
        radius: f64,
        angle: f64,
        start_angle: f64,
        num_components: u32,
        center_x: f64,
        center_y: f64,
    ) -> Result<Self, JsValue> {
        noon::ManimGeometryOptions::sector(
            radius,
            angle,
            start_angle,
            num_components,
            center_x,
            center_y,
        )
        .map(Self::from_options)
        .map_err(js_error)
    }

    #[wasm_bindgen(js_name = annulus)]
    pub fn annulus(
        inner_radius: f64,
        outer_radius: f64,
        num_components: u32,
        center_x: f64,
        center_y: f64,
    ) -> Result<Self, JsValue> {
        noon::ManimGeometryOptions::annulus(
            inner_radius,
            outer_radius,
            num_components,
            center_x,
            center_y,
        )
        .map(Self::from_options)
        .map_err(js_error)
    }

    #[wasm_bindgen(js_name = dashedLine)]
    pub fn dashed_line(
        start_x: f64,
        start_y: f64,
        end_x: f64,
        end_y: f64,
        dash_length: f64,
        dashed_ratio: f64,
    ) -> Result<Self, JsValue> {
        noon::ManimGeometryOptions::dashed_line(
            start_x,
            start_y,
            end_x,
            end_y,
            dash_length,
            dashed_ratio,
        )
        .map(Self::from_options)
        .map_err(js_error)
    }

    #[wasm_bindgen(js_name = emptyPath)]
    pub fn empty_path() -> Result<Self, JsValue> {
        noon::ManimGeometryOptions::path(VectorPath::new())
            .map(Self::from_options)
            .map_err(js_error)
    }

    pub fn path(path: WasmAuthoringVectorPath) -> Result<Self, JsValue> {
        noon::ManimGeometryOptions::path(path.path)
            .map(Self::from_options)
            .map_err(js_error)
    }
}

/// Typed path commands are accumulated without importing a resource or allocating identity.
#[wasm_bindgen]
#[derive(Default)]
pub struct WasmAuthoringVectorPath {
    path: VectorPath,
}

pub(crate) fn point(x: f64, y: f64) -> Result<Vec2, JsValue> {
    let value = noon::integration::authoring_xy_f64(x, y).map_err(js_error)?;
    Ok(Vec2::new(value.x as f32, value.y as f32))
}

pub(crate) fn points(values: &[f64]) -> Result<Vec<Vec2>, JsValue> {
    if values.len() % 2 != 0 {
        return Err(js_error(noon::AuthoringError::InvalidPointCoordinates(
            values.len(),
        )));
    }
    values
        .chunks_exact(2)
        .map(|pair| point(pair[0], pair[1]))
        .collect()
}

#[wasm_bindgen]
impl WasmAuthoringVectorPath {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self::default()
    }

    #[wasm_bindgen(js_name = moveTo)]
    pub fn move_to(&mut self, x: f64, y: f64) -> Result<(), JsValue> {
        let to = point(x, y)?;
        self.path = std::mem::take(&mut self.path).move_to(to);
        Ok(())
    }

    #[wasm_bindgen(js_name = lineTo)]
    pub fn line_to(&mut self, x: f64, y: f64) -> Result<(), JsValue> {
        let to = point(x, y)?;
        self.path = std::mem::take(&mut self.path).line_to(to);
        Ok(())
    }

    #[wasm_bindgen(js_name = quadraticTo)]
    pub fn quadratic_to(
        &mut self,
        control_x: f64,
        control_y: f64,
        x: f64,
        y: f64,
    ) -> Result<(), JsValue> {
        let control = point(control_x, control_y)?;
        let to = point(x, y)?;
        self.path = std::mem::take(&mut self.path).quadratic_to(control, to);
        Ok(())
    }

    #[wasm_bindgen(js_name = cubicTo)]
    pub fn cubic_to(
        &mut self,
        first_x: f64,
        first_y: f64,
        second_x: f64,
        second_y: f64,
        x: f64,
        y: f64,
    ) -> Result<(), JsValue> {
        let first = point(first_x, first_y)?;
        let second = point(second_x, second_y)?;
        let to = point(x, y)?;
        self.path = std::mem::take(&mut self.path).cubic_to(first, second, to);
        Ok(())
    }

    pub fn close(&mut self) {
        self.path = std::mem::take(&mut self.path).close();
    }
}

/// Inert operand references. No geometry is captured until the constructor runs,
/// so all operands observe one runtime publication.
#[wasm_bindgen]
pub struct WasmBooleanOperands {
    pub(crate) objects: Vec<noon::Mobject>,
}
#[wasm_bindgen]
impl WasmBooleanOperands {
    pub fn push(&mut self, object: &crate::WasmAuthoringMobjectHandle) {
        self.objects.push(object.semantic_mobject().clone());
    }
}
pub(crate) fn boolean_operation(name: &str) -> Result<noon::BooleanOperation, JsValue> {
    match name {
        "Union" => Ok(noon::BooleanOperation::Union),
        "Intersection" => Ok(noon::BooleanOperation::Intersection),
        "Difference" => Ok(noon::BooleanOperation::Difference),
        "Exclusion" => Ok(noon::BooleanOperation::Exclusion),
        _ => Err(js_error("unknown boolean operation")),
    }
}
#[wasm_bindgen]
impl WasmManimGeometryOptions {
    #[wasm_bindgen(js_name = booleanOperands)]
    pub fn boolean_operands() -> WasmBooleanOperands {
        WasmBooleanOperands {
            objects: Vec::new(),
        }
    }
    #[wasm_bindgen(js_name = booleanGeometry)]
    pub fn boolean_geometry(
        name: &str,
        operands: &WasmBooleanOperands,
    ) -> Result<WasmManimGeometryOptions, JsValue> {
        noon::ManimGeometryOptions::boolean_geometry(boolean_operation(name)?, &operands.objects)
            .map(Self::from_options)
            .map_err(js_error)
    }
}
