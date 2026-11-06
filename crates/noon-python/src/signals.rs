//! Store-scoped scalar identities; values and evaluation remain in shared Rust.
use crate::engine_error;
use pyo3::prelude::*;
#[pyclass(unsendable, module = "_noon_native")]
pub struct ValueTrackerHandle {
    pub(crate) value: noon::ValueTracker,
}
#[pymethods]
impl ValueTrackerHandle {
    #[getter(semanticSlot)]
    fn semantic_slot(&self) -> u32 {
        self.value.node_id().slot()
    }
    #[getter(semanticGeneration)]
    fn semantic_generation(&self) -> u32 {
        self.value.node_id().generation()
    }
    #[pyo3(name = "detachedValue")]
    fn detached_value(&self) -> PyResult<f64> {
        self.value.detached_value().map_err(engine_error)
    }
    #[pyo3(name = "setDetachedValue")]
    fn set_detached_value(&self, value: f64) -> PyResult<()> {
        self.value.set_detached_value(value).map_err(engine_error)
    }
}
#[pyclass(unsendable, module = "_noon_native")]
pub struct TrackerPositionHandle {
    pub(crate) value: noon::TrackerPosition,
}
