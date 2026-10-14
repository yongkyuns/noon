//! Native family handles project canonical Rust ownership and metadata only.
use crate::{context::MembershipBatch, engine_error, mobject::MobjectHandle};
use pyo3::prelude::*;

#[pyclass(frozen, skip_from_py_object, module = "_noon_native")]
pub struct ColorObservation {
    #[pyo3(get)]
    red: f64,
    #[pyo3(get)]
    green: f64,
    #[pyo3(get)]
    blue: f64,
    #[pyo3(get)]
    alpha: f64,
}
impl From<noon::Color> for ColorObservation {
    fn from(value: noon::Color) -> Self {
        Self {
            red: value.red.into(),
            green: value.green.into(),
            blue: value.blue.into(),
            alpha: value.alpha.into(),
        }
    }
}

#[pyclass(unsendable, skip_from_py_object, module = "_noon_native")]
#[derive(Clone)]
pub struct FamilyHandle {
    pub(crate) family: noon::MobjectFamily,
}
#[pymethods]
impl FamilyHandle {
    fn layout(&self) -> PyResult<FamilyLayoutObservation> {
        self.family
            .layout()
            .map(|layout| FamilyLayoutObservation { layout })
            .map_err(engine_error)
    }
    #[getter(semanticSlot)]
    fn semantic_slot(&self) -> u32 {
        self.family.node_id().slot()
    }
    #[getter(semanticGeneration)]
    fn semantic_generation(&self) -> u32 {
        self.family.node_id().generation()
    }
    #[pyo3(name = "memberKeys")]
    fn member_keys(&self) -> PyResult<Vec<String>> {
        let store = self.family.integration_store().borrow();
        let node = store
            .semantic_family_checked(self.family.node_id())
            .map_err(engine_error)?;
        Ok(node
            .members_iter()
            .map(|id| format!("{}:{}", id.slot(), id.generation()))
            .collect())
    }
    #[pyo3(name = "manimColor")]
    fn color(&self) -> PyResult<ColorObservation> {
        self.family
            .own_color()
            .map(ColorObservation::from)
            .map_err(engine_error)
    }
    #[pyo3(name = "setColor")]
    fn set_color(&self, red: f64, green: f64, blue: f64, alpha: f64) -> PyResult<()> {
        self.family
            .set_color(red, green, blue, alpha)
            .map_err(engine_error)
    }
    #[pyo3(name = "setOwnColor")]
    fn set_own_color(&self, red: f64, green: f64, blue: f64, alpha: f64) -> PyResult<()> {
        self.family
            .set_own_color(red, green, blue, alpha)
            .map_err(engine_error)
    }
    #[pyo3(name = "editMembership")]
    fn edit_membership(&self, batch: &MembershipBatch) -> PyResult<Vec<bool>> {
        let members = batch.targets();
        match batch.kind.as_str() {
            "add" => self.family.add_many(&members),
            "remove" => self.family.remove_many(&members),
            _ => return Err(engine_error("unsupported family membership operation")),
        }
        .map_err(engine_error)
    }
    #[pyo3(name = "copyFamily")]
    fn copy_family(&self, references: &MembershipBatch) -> PyResult<FamilyCopy> {
        if references.kind != "add" {
            return Err(engine_error("family copy references require an add batch"));
        }
        self.family
            .copy_with_references(&references.targets())
            .map(|copied| FamilyCopy { copied })
            .map_err(engine_error)
    }
}

/// Immutable authored observation; all aggregation and critical points stay in Rust.
#[pyclass(unsendable, skip_from_py_object, module = "_noon_native")]
pub struct FamilyLayoutObservation {
    layout: noon::FamilyLayout,
}
#[pymethods]
impl FamilyLayoutObservation {
    #[getter(centerX)]
    fn center_x(&self) -> f64 {
        self.layout.center().0
    }
    #[getter(centerY)]
    fn center_y(&self) -> f64 {
        self.layout.center().1
    }
    #[getter]
    fn width(&self) -> f64 {
        self.layout.width()
    }
    #[getter]
    fn height(&self) -> f64 {
        self.layout.height()
    }
    #[pyo3(name = "criticalX")]
    fn critical_x(&self, x: f64, y: f64) -> f64 {
        self.layout.critical_point(x, y).0
    }
    #[pyo3(name = "criticalY")]
    fn critical_y(&self, x: f64, y: f64) -> f64 {
        self.layout.critical_point(x, y).1
    }
}

#[pyclass(unsendable, skip_from_py_object, module = "_noon_native")]
pub struct FamilyCopy {
    pub(crate) copied: noon::FamilyCopy,
}
#[pymethods]
impl FamilyCopy {
    #[pyo3(name = "familyFor")]
    fn family_for(&self, source: &FamilyHandle) -> PyResult<FamilyHandle> {
        self.copied
            .family(&source.family)
            .map(|family| FamilyHandle { family })
            .map_err(engine_error)
    }
    #[pyo3(name = "mobjectFor")]
    fn mobject_for(&self, source: &MobjectHandle) -> PyResult<MobjectHandle> {
        self.copied
            .mobject(&source.handle)
            .map(MobjectHandle::from_semantic_mobject)
            .map_err(engine_error)
    }
}
