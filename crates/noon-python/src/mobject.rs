//! Store-scoped native handles; all geometry/state behavior is shared Rust.
use crate::engine_error;
use pyo3::prelude::*;

#[pyclass(unsendable, skip_from_py_object, module = "_noon_native")]
#[derive(Clone)]
pub struct MobjectHandle {
    pub(crate) handle: noon::Mobject,
}
impl MobjectHandle {
    pub(crate) fn from_semantic_mobject(handle: noon::Mobject) -> Self {
        Self { handle }
    }
}
#[pymethods]
impl MobjectHandle {
    #[getter(semanticSlot)]
    pub fn semantic_slot(&self) -> u32 {
        self.handle.node_id().slot()
    }
    #[getter(semanticGeneration)]
    pub fn semantic_generation(&self) -> u32 {
        self.handle.node_id().generation()
    }
    #[pyo3(name = "cloneHandle")]
    pub fn clone_handle(&self) -> Result<MobjectHandle, PyErr> {
        self.handle
            .copy_handle()
            .map(|handle| Self { handle })
            .map_err(engine_error)
    }
    #[pyo3(name = "targetEditor")]
    pub fn target_editor(&self) -> Result<MobjectHandle, PyErr> {
        self.clone_handle()
    }
    #[pyo3(name = "centerCoordinates")]
    pub fn center_coordinates(&self) -> Result<(f64, f64), PyErr> {
        self.handle.center().map_err(engine_error)
    }
    #[getter(centerX)]
    pub fn center_x(&self) -> Result<f64, PyErr> {
        Ok(self.handle.center().map_err(engine_error)?.0)
    }
    #[getter(centerY)]
    pub fn center_y(&self) -> Result<f64, PyErr> {
        Ok(self.handle.center().map_err(engine_error)?.1)
    }
    #[getter]
    pub fn width(&self) -> Result<f64, PyErr> {
        self.handle.width().map_err(engine_error)
    }
    #[getter]
    pub fn height(&self) -> Result<f64, PyErr> {
        self.handle.height().map_err(engine_error)
    }
    #[pyo3(name = "criticalX")]
    pub fn critical_x(&self, direction_x: f64, direction_y: f64) -> Result<f64, PyErr> {
        Ok(self
            .handle
            .critical_point(direction_x, direction_y)
            .map_err(engine_error)?
            .0)
    }
    #[pyo3(name = "criticalY")]
    pub fn critical_y(&self, direction_x: f64, direction_y: f64) -> Result<f64, PyErr> {
        Ok(self
            .handle
            .critical_point(direction_x, direction_y)
            .map_err(engine_error)?
            .1)
    }
    pub fn shift(&mut self, x: f64, y: f64) -> Result<(), PyErr> {
        self.handle.shift(x, y).map_err(engine_error)
    }
    #[pyo3(name = "moveTo")]
    pub fn move_to(&mut self, x: f64, y: f64) -> Result<(), PyErr> {
        self.handle.move_to(x, y).map_err(engine_error)
    }
    #[pyo3(name = "setTranslation")]
    pub fn set_translation(&mut self, x: f64, y: f64) -> Result<(), PyErr> {
        self.handle.set_translation(x, y).map_err(engine_error)
    }
    #[pyo3(name = "setScale")]
    pub fn set_scale(&mut self, x: f64, y: f64) -> Result<(), PyErr> {
        self.handle.set_scale(x, y).map_err(engine_error)
    }
    #[pyo3(name = "setRotation")]
    pub fn set_rotation(&mut self, angle: f64) -> Result<(), PyErr> {
        self.handle.set_rotation(angle).map_err(engine_error)
    }
    #[pyo3(name = "setStrokeWidthMode")]
    pub fn set_stroke_width_mode(&mut self, mode: &str) -> Result<(), PyErr> {
        self.handle
            .set_stroke_width_mode(mode)
            .map_err(engine_error)
    }
    #[pyo3(name = "setStrokeJoin")]
    pub fn set_stroke_join(&mut self, join: &str) -> Result<(), PyErr> {
        self.handle.set_stroke_join(join).map_err(engine_error)
    }
    #[pyo3(name = "setStrokeCap")]
    pub fn set_stroke_cap(&mut self, cap: &str) -> Result<(), PyErr> {
        self.handle.set_stroke_cap(cap).map_err(engine_error)
    }
    #[pyo3(name = "setObjectOpacity")]
    pub fn set_object_opacity(&mut self, opacity: f64) -> Result<(), PyErr> {
        self.handle
            .set_object_opacity(opacity)
            .map_err(engine_error)
    }
    #[pyo3(name = "manimMoveToPoint")]
    pub fn manim_move_to_point(
        &mut self,
        point_x: f64,
        point_y: f64,
        aligned_edge_x: f64,
        aligned_edge_y: f64,
        mask_x: f64,
        mask_y: f64,
    ) -> Result<(), PyErr> {
        self.handle
            .manim_move_to_point(
                point_x,
                point_y,
                aligned_edge_x,
                aligned_edge_y,
                mask_x,
                mask_y,
            )
            .map_err(engine_error)
    }
    pub fn scale(&mut self, x: f64, y: f64) -> Result<(), PyErr> {
        self.handle.manim_scale(x, y).map_err(engine_error)
    }
    pub fn rotate(&mut self, angle: f64) -> Result<(), PyErr> {
        self.handle.rotate(angle).map_err(engine_error)
    }
    #[pyo3(name = "rotateAboutPoint")]
    pub fn rotate_about_point(
        &mut self,
        angle: f64,
        point_x: f64,
        point_y: f64,
    ) -> Result<(), PyErr> {
        self.handle
            .rotate_about_point(angle, point_x, point_y)
            .map_err(engine_error)
    }
    #[pyo3(name = "setColor")]
    pub fn set_color(&mut self, red: f64, green: f64, blue: f64, alpha: f64) -> Result<(), PyErr> {
        self.handle
            .set_color(red, green, blue, alpha)
            .map_err(engine_error)
    }
    #[pyo3(name = "disableFill")]
    pub fn disable_fill(&mut self) -> Result<(), PyErr> {
        self.handle.disable_fill().map_err(engine_error)
    }
    #[pyo3(name = "setFillColor")]
    pub fn set_fill_color(
        &mut self,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
    ) -> Result<(), PyErr> {
        self.handle
            .set_fill_color(red, green, blue, alpha)
            .map_err(engine_error)
    }
    #[pyo3(name = "setFillOpacity")]
    pub fn set_fill_opacity(&mut self, opacity: f64) -> Result<(), PyErr> {
        self.handle.set_fill_opacity(opacity).map_err(engine_error)
    }
    #[pyo3(name = "setFill")]
    pub fn set_fill(&mut self, red: f64, green: f64, blue: f64, opacity: f64) -> Result<(), PyErr> {
        self.handle
            .set_fill(red, green, blue, opacity)
            .map_err(engine_error)
    }
    #[getter(fillOpacity)]
    pub fn fill_opacity(&self) -> Result<f64, PyErr> {
        self.handle.fill_opacity().map_err(engine_error)
    }
    #[pyo3(name = "disableStroke")]
    pub fn disable_stroke(&mut self) -> Result<(), PyErr> {
        self.handle.disable_stroke().map_err(engine_error)
    }
    #[pyo3(name = "setStrokeColor")]
    pub fn set_stroke_color(
        &mut self,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
    ) -> Result<(), PyErr> {
        self.handle
            .set_stroke_color(red, green, blue, alpha)
            .map_err(engine_error)
    }
    #[pyo3(name = "setStrokeWidth")]
    pub fn set_stroke_width(&mut self, width: f64) -> Result<(), PyErr> {
        self.handle.set_stroke_width(width).map_err(engine_error)
    }
    #[pyo3(name = "setStrokeOpacity")]
    pub fn set_stroke_opacity(&mut self, opacity: f64) -> Result<(), PyErr> {
        self.handle
            .set_stroke_opacity(opacity)
            .map_err(engine_error)
    }
    #[getter(strokeOpacity)]
    pub fn stroke_opacity(&self) -> Result<f64, PyErr> {
        self.handle.stroke_opacity().map_err(engine_error)
    }
    #[pyo3(name = "setOpacity")]
    pub fn set_opacity(&mut self, opacity: f64) -> Result<(), PyErr> {
        self.handle.set_opacity(opacity).map_err(engine_error)
    }
    #[getter]
    fn rotation(&self) -> PyResult<f64> {
        self.handle
            .state()
            .map_err(engine_error)?
            .transform
            .planar_rotation()
            .ok_or_else(|| engine_error("rotation getter is unsupported for spatial orientation"))
    }
    #[pyo3(name = "layoutAnchor")]
    fn layout_anchor(&self, index: Option<isize>) -> LayoutAnchor {
        let anchor = noon::LayoutAnchor::from(&self.handle);
        LayoutAnchor {
            anchor: match index {
                Some(i) => anchor.member(i),
                None => anchor,
            },
        }
    }
}
#[pyclass(unsendable, skip_from_py_object, module = "_noon_native")]
#[derive(Clone)]
pub struct LayoutAnchor {
    pub(crate) anchor: noon::LayoutAnchor,
}
#[pymethods]
impl LayoutAnchor {
    fn scale(&self, sx: f64, sy: f64, x: f64, y: f64, point: bool) -> PyResult<()> {
        self.anchor
            .scale(sx, sy, pivot(x, y, point))
            .map_err(engine_error)
    }
    fn rotate(&self, angle: f64, x: f64, y: f64, point: bool) -> PyResult<()> {
        self.anchor
            .rotate(angle, pivot(x, y, point))
            .map_err(engine_error)
    }
    #[pyo3(name = "zIndex")]
    fn z_index(&self) -> PyResult<f64> {
        self.anchor.z_index().map_err(engine_error)
    }
    #[pyo3(name = "setZIndex")]
    fn set_z_index(&self, value: f64, family: bool) -> PyResult<()> {
        self.anchor.set_z_index(value, family).map_err(engine_error)
    }
}
pub(crate) fn pivot(x: f64, y: f64, point: bool) -> noon::ManimRotationPivot {
    if point {
        noon::ManimRotationPivot::Point(x, y)
    } else {
        noon::ManimRotationPivot::Edge(x, y)
    }
}
#[pyclass(module = "_noon_native")]
pub struct LayoutObservation {
    pub(crate) center: (f64, f64),
    pub(crate) width: f64,
    pub(crate) height: f64,
}
#[pymethods]
impl LayoutObservation {
    #[getter(centerX)]
    fn center_x(&self) -> f64 {
        self.center.0
    }
    #[getter(centerY)]
    fn center_y(&self) -> f64 {
        self.center.1
    }
    #[getter]
    fn width(&self) -> f64 {
        self.width
    }
    #[getter]
    fn height(&self) -> f64 {
        self.height
    }
}

impl LayoutObservation {
    pub(crate) fn from_effective(value: noon::EffectiveMobjectLayout) -> Self {
        Self {
            center: value.center,
            width: value.width,
            height: value.height,
        }
    }
}
