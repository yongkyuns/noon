//! Immutable CPython projections of shared Rust text-source observations.
use crate::engine_error;
use pyo3::prelude::*;

#[pyclass(unsendable, module = "_noon_native")]
pub struct TextPart {
    value: Option<noon::TextPart>,
}
impl TextPart {
    fn value(&self) -> PyResult<&noon::TextPart> {
        self.value
            .as_ref()
            .ok_or_else(|| engine_error("text part has been freed"))
    }
}
#[pymethods]
impl TextPart {
    fn free(&mut self) {
        self.value.take();
    }
    #[getter(sourceStart)]
    fn source_start(&self) -> PyResult<u32> {
        Ok(self.value()?.source_span.start)
    }
    #[getter(sourceEnd)]
    fn source_end(&self) -> PyResult<u32> {
        Ok(self.value()?.source_span.end)
    }
    #[getter(firstCluster)]
    fn first_cluster(&self) -> PyResult<u32> {
        Ok(self.value()?.first_cluster)
    }
    #[getter(clusterCount)]
    fn cluster_count(&self) -> PyResult<u32> {
        Ok(self.value()?.cluster_count)
    }
    #[getter(firstVector)]
    fn first_vector(&self) -> PyResult<u32> {
        Ok(self.value()?.first_vector)
    }
    #[getter(vectorCount)]
    fn vector_count(&self) -> PyResult<u32> {
        Ok(self.value()?.vector_count)
    }
    #[getter(semanticKey)]
    fn semantic_key(&self) -> PyResult<Option<String>> {
        Ok(self.value()?.semantic_key.as_deref().map(ToOwned::to_owned))
    }
}

#[pyclass(unsendable, module = "_noon_native")]
pub struct TextPartList {
    values: Option<Vec<noon::TextPart>>,
}
impl TextPartList {
    pub(crate) fn new(values: Vec<noon::TextPart>) -> Self {
        Self {
            values: Some(values),
        }
    }
    fn values(&self) -> PyResult<&[noon::TextPart]> {
        self.values
            .as_deref()
            .ok_or_else(|| engine_error("text part list has been freed"))
    }
}
#[pymethods]
impl TextPartList {
    fn free(&mut self) {
        self.values.take();
    }
    #[getter]
    fn length(&self) -> PyResult<usize> {
        Ok(self.values()?.len())
    }
    fn item(&self, index: usize) -> PyResult<TextPart> {
        self.values()?
            .get(index)
            .cloned()
            .map(|value| TextPart { value: Some(value) })
            .ok_or_else(|| engine_error("text part index is out of bounds"))
    }
}
