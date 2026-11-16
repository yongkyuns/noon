//! Native retained BraceLabel handle, backed by Scene publication.
use crate::{engine_error, family::FamilyHandle, mobject::MobjectHandle};
use pyo3::prelude::*;

#[pyclass(unsendable, skip_from_py_object, module = "_noon_native")]
pub(crate) struct BraceLabelHandle(pub(crate) noon::BraceLabel);

#[pymethods]
impl BraceLabelHandle {
    fn family(&self) -> FamilyHandle {
        FamilyHandle::from_semantic_family(self.0.family().clone())
    }

    fn brace(&self) -> MobjectHandle {
        MobjectHandle::from_semantic_mobject(self.0.brace().object().clone())
    }
}

pub(crate) fn options(
    x: f64,
    y: f64,
    buff: f64,
    sharpness: f64,
    label_buff: f64,
) -> noon::BraceOptions {
    noon::BraceOptions {
        direction: (x, y),
        buff,
        sharpness,
        label_buff,
    }
}

pub(crate) fn failure(code: &'static str, error: &noon::AuthoringError) -> PyErr {
    engine_error(noon::integration::AuthoringFailure::unclassified(
        code, error,
    ))
}
