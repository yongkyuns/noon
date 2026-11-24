//! Thin CPython command projection; the inert path accumulator lives in shared Rust.
use crate::engine_error;
use pyo3::prelude::*;

#[pyclass(unsendable, module = "_noon_native")]
pub struct AuthoringVectorPath {
    value: Option<noon::integration::AuthoringVectorPath>,
}
impl AuthoringVectorPath {
    fn value(&mut self) -> PyResult<&mut noon::integration::AuthoringVectorPath> {
        self.value
            .as_mut()
            .ok_or_else(|| engine_error("path input has been consumed"))
    }
    pub(crate) fn take(&mut self) -> PyResult<noon_core::VectorPath> {
        self.value
            .take()
            .map(|p| p.into_path())
            .ok_or_else(|| engine_error("path input has been consumed"))
    }
}
#[pymethods]
impl AuthoringVectorPath {
    #[new]
    fn new() -> Self {
        Self {
            value: Some(noon::integration::AuthoringVectorPath::default()),
        }
    }
    #[pyo3(name = "moveTo")]
    fn move_to(&mut self, x: f64, y: f64) -> PyResult<()> {
        self.value()?.move_to(x, y).map_err(engine_error)
    }
    #[pyo3(name = "lineTo")]
    fn line_to(&mut self, x: f64, y: f64) -> PyResult<()> {
        self.value()?.line_to(x, y).map_err(engine_error)
    }
    #[pyo3(name = "quadraticTo")]
    fn quadratic_to(&mut self, cx: f64, cy: f64, x: f64, y: f64) -> PyResult<()> {
        self.value()?
            .quadratic_to(cx, cy, x, y)
            .map_err(engine_error)
    }
    #[pyo3(name = "cubicTo")]
    fn cubic_to(&mut self, ax: f64, ay: f64, bx: f64, by: f64, x: f64, y: f64) -> PyResult<()> {
        self.value()?
            .cubic_to(ax, ay, bx, by, x, y)
            .map_err(engine_error)
    }
    fn close(&mut self) -> PyResult<()> {
        self.value()?.close();
        Ok(())
    }
}
