//! Inert constructor values forwarding directly to shared Rust validation.
use crate::engine_error;
use pyo3::prelude::*;
#[pyclass(unsendable, skip_from_py_object, module = "_noon_native")]
#[derive(Clone)]
pub struct GeometryOptions {
    pub(crate) options: noon::ManimGeometryOptions,
}
impl GeometryOptions {
    pub(crate) fn from_options(options: noon::ManimGeometryOptions) -> Self {
        Self { options }
    }
}
#[pymethods]
impl GeometryOptions {
    #[pyo3(name = "setZIndex")]
    pub fn set_z_index(&mut self, value: f64) -> Result<(), PyErr> {
        self.options.set_z_index(value).map_err(engine_error)
    }

    #[pyo3(name = "setTranslation")]
    pub fn set_translation(&mut self, x: f64, y: f64) -> Result<(), PyErr> {
        self.options.set_translation(x, y).map_err(engine_error)
    }

    #[pyo3(name = "setScale")]
    pub fn set_scale(&mut self, x: f64, y: f64) -> Result<(), PyErr> {
        self.options.set_scale(x, y).map_err(engine_error)
    }

    #[pyo3(name = "scaleBy")]
    pub fn scale_by(&mut self, x: f64, y: f64) -> Result<(), PyErr> {
        self.options.scale_by(x, y).map_err(engine_error)
    }

    #[pyo3(name = "setRotation")]
    pub fn set_rotation(&mut self, angle: f64) -> Result<(), PyErr> {
        self.options.set_rotation(angle).map_err(engine_error)
    }

    #[pyo3(name = "setColor")]
    pub fn set_color(&mut self, red: f64, green: f64, blue: f64, alpha: f64) -> Result<(), PyErr> {
        self.options
            .set_color(red, green, blue, alpha)
            .map_err(engine_error)
    }

    #[pyo3(name = "disableFill")]
    pub fn disable_fill(&mut self) {
        self.options.disable_fill();
    }

    #[pyo3(name = "setFill")]
    pub fn set_fill(&mut self, red: f64, green: f64, blue: f64, opacity: f64) -> Result<(), PyErr> {
        self.options
            .set_fill(red, green, blue, opacity)
            .map_err(engine_error)
    }

    #[pyo3(name = "setFillColor")]
    pub fn set_fill_color(
        &mut self,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
    ) -> Result<(), PyErr> {
        self.options
            .set_fill_color(red, green, blue, alpha)
            .map_err(engine_error)
    }

    #[pyo3(name = "setFillOpacity")]
    pub fn set_fill_opacity(&mut self, opacity: f64) -> Result<(), PyErr> {
        self.options.set_fill_opacity(opacity).map_err(engine_error)
    }

    #[pyo3(name = "disableStroke")]
    pub fn disable_stroke(&mut self) {
        self.options.disable_stroke();
    }

    #[pyo3(name = "setStroke")]
    pub fn set_stroke(
        &mut self,
        red: f64,
        green: f64,
        blue: f64,
        opacity: f64,
    ) -> Result<(), PyErr> {
        self.options
            .set_stroke(red, green, blue, opacity)
            .map_err(engine_error)
    }

    #[pyo3(name = "setStrokeColor")]
    pub fn set_stroke_color(
        &mut self,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
    ) -> Result<(), PyErr> {
        self.options
            .set_stroke_color(red, green, blue, alpha)
            .map_err(engine_error)
    }

    #[pyo3(name = "setStrokeOpacity")]
    pub fn set_stroke_opacity(&mut self, opacity: f64) -> Result<(), PyErr> {
        self.options
            .set_stroke_opacity(opacity)
            .map_err(engine_error)
    }

    #[pyo3(name = "setStrokeWidth")]
    pub fn set_stroke_width(&mut self, width: f64) -> Result<(), PyErr> {
        self.options.set_stroke_width(width).map_err(engine_error)
    }

    #[pyo3(name = "setStrokeWidthMode")]
    pub fn set_stroke_width_mode(&mut self, mode: &str) -> Result<(), PyErr> {
        self.options
            .set_stroke_width_mode(mode)
            .map_err(engine_error)
    }

    #[pyo3(name = "setStrokeJoin")]
    pub fn set_stroke_join(&mut self, join: &str) -> Result<(), PyErr> {
        self.options.set_stroke_join(join).map_err(engine_error)
    }

    #[pyo3(name = "setStrokeCap")]
    pub fn set_stroke_cap(&mut self, cap: &str) -> Result<(), PyErr> {
        self.options.set_stroke_cap(cap).map_err(engine_error)
    }

    #[pyo3(name = "setObjectOpacity")]
    pub fn set_object_opacity(&mut self, opacity: f64) -> Result<(), PyErr> {
        self.options
            .set_object_opacity(opacity)
            .map_err(engine_error)
    }

    #[staticmethod]
    pub fn circle(radius: f64) -> Result<Self, PyErr> {
        noon::ManimGeometryOptions::circle(radius)
            .map(Self::from_options)
            .map_err(engine_error)
    }
    #[staticmethod]
    pub fn ellipse(width: f64, height: f64) -> Result<Self, PyErr> {
        noon::ManimGeometryOptions::ellipse(width, height)
            .map(Self::from_options)
            .map_err(engine_error)
    }
    #[staticmethod]
    pub fn square(side: f64) -> Result<Self, PyErr> {
        noon::ManimGeometryOptions::square(side)
            .map(Self::from_options)
            .map_err(engine_error)
    }
    #[staticmethod]
    pub fn rectangle(width: f64, height: f64) -> Result<Self, PyErr> {
        noon::ManimGeometryOptions::rectangle(width, height)
            .map(Self::from_options)
            .map_err(engine_error)
    }
    #[staticmethod]
    pub fn line(start_x: f64, start_y: f64, end_x: f64, end_y: f64) -> Result<Self, PyErr> {
        noon::ManimGeometryOptions::line(start_x, start_y, end_x, end_y)
            .map(Self::from_options)
            .map_err(engine_error)
    }
    #[staticmethod]
    #[pyo3(name = "dot")]
    pub fn dot(x: f64, y: f64, radius: f64) -> Result<Self, PyErr> {
        noon::ManimGeometryOptions::dot(x, y, radius)
            .map(Self::from_options)
            .map_err(engine_error)
    }
    #[staticmethod]
    #[pyo3(name = "triangle")]
    pub fn triangle() -> Result<Self, PyErr> {
        noon::ManimGeometryOptions::triangle()
            .map(Self::from_options)
            .map_err(engine_error)
    }
}
