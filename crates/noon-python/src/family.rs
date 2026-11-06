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
    #[pyo3(name = "prependMembers")]
    fn prepend_members(&self, batch: &MembershipBatch) -> PyResult<()> {
        if batch.kind != "add" {
            return Err(engine_error("member prepend requires an add batch"));
        }
        self.family
            .prepend_many(&batch.targets())
            .map_err(engine_error)
    }

    #[pyo3(name="splitIterator",signature=(include=None))]
    fn split_iterator(&self, include: Option<bool>) -> PyResult<crate::family::FamilyIterator> {
        crate::family::split_iterator(
            noon::CompositeEntryHandle::Family(self.family.clone()),
            include,
        )
    }
    #[pyo3(name="splitCount",signature=(include=None))]
    fn split_count(&self, include: Option<bool>) -> PyResult<usize> {
        crate::family::split_count(
            noon::CompositeEntryHandle::Family(self.family.clone()),
            include,
        )
    }
    #[pyo3(name="splitKeys",signature=(include=None))]
    fn split_keys(&self, include: Option<bool>) -> PyResult<Vec<String>> {
        crate::family::split_keys(
            noon::CompositeEntryHandle::Family(self.family.clone()),
            include,
        )
    }
    #[pyo3(name="splitAt",signature=(index,include=None))]
    fn split_at(&self, py: Python<'_>, index: usize, include: Option<bool>) -> PyResult<Py<PyAny>> {
        crate::family::split_at(
            py,
            noon::CompositeEntryHandle::Family(self.family.clone()),
            index,
            include,
        )
    }
    #[pyo3(name="prepareSlice",signature=(include=None))]
    fn slice_plan(&self, include: Option<bool>) -> PyResult<crate::family::FamilySlicePlan> {
        crate::family::slice_plan(
            noon::CompositeEntryHandle::Family(self.family.clone()),
            include,
        )
    }
    #[pyo3(name = "vectorBoundaryPoints3D")]
    fn vector_boundary_points_3d(&self) -> PyResult<Vec<f64>> {
        noon::MobjectTarget::from(&self.family)
            .vector_boundary_points_3d()
            .map(crate::path_input::flat_points_3d)
            .map_err(engine_error)
    }

    #[pyo3(signature=(x,y,z=None))]
    fn shift(&self, x: f64, y: f64, z: Option<f64>) -> PyResult<()> {
        self.family
            .shift_3d(noon::SemanticVec3::new(x, y, z.unwrap_or(0.0)))
            .map_err(engine_error)
    }
    #[pyo3(name = "pointCount")]
    fn point_count(&self) -> PyResult<usize> {
        noon::MobjectTarget::from(&self.family)
            .get_num_points()
            .map_err(engine_error)
    }
    #[pyo3(name = "pointArray")]
    fn point_array(&self) -> PyResult<(Vec<f64>, Vec<usize>)> {
        noon::MobjectTarget::from(&self.family)
            .point_array()
            .map(crate::path_input::point_array)
            .map_err(engine_error)
    }

    #[pyo3(name = "setPoints")]
    fn set_points_f64(&self, values: Vec<f64>) -> PyResult<()> {
        self.family
            .set_points_f64(&crate::path_input::points_f64(&values)?)
            .map_err(engine_error)
    }
    #[pyo3(name = "setPoints3D")]
    fn set_points_3d(&self, values: Vec<f64>) -> PyResult<()> {
        self.family
            .set_points_3d(&crate::path_input::points_3d(&values)?)
            .map_err(engine_error)
    }
    #[pyo3(name = "appendPoints")]
    fn append_points_f64(&self, values: Vec<f64>) -> PyResult<()> {
        self.family
            .append_points_f64(&crate::path_input::points_f64(&values)?)
            .map_err(engine_error)
    }
    #[pyo3(name = "appendPoints3D")]
    fn append_points_3d(&self, values: Vec<f64>) -> PyResult<()> {
        self.family
            .append_points_3d(&crate::path_input::points_3d(&values)?)
            .map_err(engine_error)
    }
    #[pyo3(name = "setEmptyPointArray")]
    fn empty_array(&self, values: Vec<f64>) -> PyResult<()> {
        self.family
            .set_empty_point_array(crate::path_input::point_array_shape(&values)?)
            .map_err(engine_error)
    }
    #[pyo3(name = "clearPoints")]
    fn clear_points(&self) -> PyResult<()> {
        self.family.clear_points().map_err(engine_error)
    }
    #[pyo3(name = "pathQuery")]
    fn path_query(&self) -> PyResult<crate::path_query::PathQuery> {
        self.family
            .own_path_query()
            .map(crate::path_query::PathQuery::new)
            .map_err(engine_error)
    }
    #[pyo3(name = "makeSmooth")]
    fn make_smooth(&self) -> PyResult<()> {
        self.family.make_smooth().map_err(engine_error)
    }
    #[pyo3(name = "makeJagged")]
    fn make_jagged(&self) -> PyResult<()> {
        self.family.make_jagged().map_err(engine_error)
    }

    #[pyo3(name = "setGlow")]
    fn set_glow(&self, update: &crate::effects::EffectUpdate) -> PyResult<()> {
        self.family
            .clone()
            .set_glow(update.value.clone())
            .map_err(engine_error)
    }
    #[pyo3(name = "removeGlow")]
    fn remove_glow(&self) -> PyResult<()> {
        self.family.clone().remove_glow().map_err(engine_error)
    }
    #[pyo3(name = "addEffect")]
    fn add_effect(
        &self,
        definition: &crate::effects::EffectDefinition,
        name: &str,
    ) -> PyResult<()> {
        self.family
            .clone()
            .add_effect(definition.definition(), name)
            .map_err(engine_error)
    }
    #[pyo3(name = "getEffect")]
    fn get_effect(&self, name: &str) -> PyResult<crate::effects::EffectHandle> {
        self.family
            .get_effect(name)
            .map(|handle| crate::effects::EffectHandle { handle })
            .map_err(engine_error)
    }
    #[pyo3(name = "setEffect")]
    fn set_effect(&self, name: &str, update: &crate::effects::EffectUpdate) -> PyResult<()> {
        self.family
            .clone()
            .set_effect(name, update.value.clone())
            .map_err(engine_error)
    }
    #[pyo3(name = "setEffectHandle")]
    fn set_effect_handle(
        &self,
        handle: &crate::effects::EffectHandle,
        update: &crate::effects::EffectUpdate,
    ) -> PyResult<()> {
        self.family
            .clone()
            .set_effect(&handle.handle, update.value.clone())
            .map_err(engine_error)
    }
    #[pyo3(name = "removeEffect")]
    fn remove_effect(&self, name: &str) -> PyResult<()> {
        self.family
            .clone()
            .remove_effect(name)
            .map_err(engine_error)
    }
    #[pyo3(name = "removeEffectHandle")]
    fn remove_effect_handle(&self, handle: &crate::effects::EffectHandle) -> PyResult<()> {
        self.family
            .clone()
            .remove_effect(&handle.handle)
            .map_err(engine_error)
    }

    #[pyo3(name = "familyKeys")]
    fn family_keys(&self) -> PyResult<Vec<String>> {
        noon::MobjectTarget::from(&self.family)
            .family_nodes()
            .map(|nodes| {
                nodes
                    .into_iter()
                    .map(|id| format!("{}:{}", id.slot(), id.generation()))
                    .collect()
            })
            .map_err(engine_error)
    }

    #[pyo3(name = "requiresContentBoundary")]
    fn requires_content_boundary(&self) -> PyResult<bool> {
        noon::MobjectTarget::from(&self.family)
            .requires_content_boundary()
            .map_err(engine_error)
    }
    #[pyo3(name = "criticalPoint3D")]
    fn critical_point_3d(
        &self,
        x: f64,
        y: f64,
        z: f64,
        vector_boundary: bool,
    ) -> PyResult<(f64, f64, f64)> {
        let target = noon::MobjectTarget::from(&self.family);
        let direction = noon::SemanticVec3::new(x, y, z);
        let point = if vector_boundary {
            target.vector_critical_point_3d(direction)
        } else {
            target.critical_point_3d(direction)
        };
        point.map(|p| (p.x, p.y, p.z)).map_err(engine_error)
    }
    #[pyo3(name = "allPoints3D")]
    fn all_points_3d(&self) -> PyResult<Vec<f64>> {
        noon::MobjectTarget::from(&self.family)
            .all_points_3d()
            .map(|points| points.into_iter().flat_map(|p| [p.x, p.y, p.z]).collect())
            .map_err(engine_error)
    }

    #[getter(memberCount)]
    fn member_count(&self) -> PyResult<usize> {
        let store = self.family.integration_store().borrow();
        Ok(store
            .semantic_family_checked(self.family.node_id())
            .map_err(engine_error)?
            .members_iter()
            .count())
    }
    #[pyo3(name = "memberMobject")]
    fn member_mobject(&self, index: usize) -> PyResult<MobjectHandle> {
        let store = std::rc::Rc::clone(self.family.integration_store());
        let member = store
            .borrow()
            .semantic_family_members_checked(self.family.node_id())
            .map_err(engine_error)?
            .get(index)
            .copied()
            .ok_or_else(|| engine_error("family member index is out of bounds"))?;
        noon::Mobject::from_node(store, member)
            .map(MobjectHandle::from_semantic_mobject)
            .map_err(engine_error)
    }
    fn layout(&self) -> PyResult<FamilyLayoutObservation> {
        self.family
            .layout()
            .map(|layout| FamilyLayoutObservation { layout })
            .map_err(engine_error)
    }
    #[pyo3(name = "layoutAnchor")]
    fn layout_anchor(&self, index: Option<isize>) -> PyResult<crate::mobject::LayoutAnchor> {
        self.family.validate().map_err(engine_error)?;
        Ok(crate::mobject::LayoutAnchor::from_anchor(
            noon::LayoutAnchor::from(&self.family),
            index,
        ))
    }
    #[getter(semanticSlot)]
    fn semantic_slot(&self) -> u32 {
        self.family.node_id().slot()
    }
    #[getter(semanticGeneration)]
    fn semantic_generation(&self) -> u32 {
        self.family.node_id().generation()
    }
    fn scale(&self, x: f64, y: f64) -> PyResult<()> {
        self.family.scale(x, y).map_err(engine_error)
    }
    #[pyo3(name = "setOpacity")]
    fn set_opacity(&self, opacity: f64) -> PyResult<()> {
        self.family.set_opacity(opacity).map_err(engine_error)
    }
    #[pyo3(name = "setFill", signature = (has_color, red, green, blue, alpha, opacity=None))]
    fn set_fill(
        &self,
        has_color: bool,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
        opacity: Option<f64>,
    ) -> PyResult<()> {
        let color = noon::integration::family_color(has_color, red, green, blue, alpha)
            .map_err(engine_error)?;
        self.family.set_fill(color, opacity).map_err(engine_error)
    }
    #[pyo3(name = "setStroke", signature = (has_color, red, green, blue, alpha, width=None, opacity=None))]
    #[allow(clippy::too_many_arguments)]
    fn set_stroke(
        &self,
        has_color: bool,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
        width: Option<f64>,
        opacity: Option<f64>,
    ) -> PyResult<()> {
        let color = noon::integration::family_color(has_color, red, green, blue, alpha)
            .map_err(engine_error)?;
        self.family
            .set_stroke(color, width, opacity)
            .map_err(engine_error)
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

/// Immutable authored observation with shared atomic placement operations.
#[pyclass(unsendable, skip_from_py_object, module = "_noon_native")]
pub struct FamilyLayoutObservation {
    layout: noon::FamilyLayout,
}
#[pymethods]
impl FamilyLayoutObservation {
    #[pyo3(name = "shiftBy")]
    fn shift_by(&self, x: f64, y: f64) -> PyResult<()> {
        self.layout.shift(x, y).map_err(engine_error)
    }
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

/// Native projection of the existing shared occurrence cursor and slice plan.
#[pyclass(unsendable, module = "_noon_native")]
pub struct FamilyIterator {
    iterator: Option<noon::FamilyIterator>,
}
#[pymethods]
impl FamilyIterator {
    #[pyo3(name = "nextMember")]
    fn next_member(&mut self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let value = self
            .iterator
            .as_mut()
            .ok_or_else(|| engine_error("family iterator has been freed"))?
            .next()
            .transpose()
            .map_err(engine_error)?;
        entry_handle(py, value)
    }
    fn free(&mut self) {
        self.iterator.take();
    }
}
#[pyclass(unsendable, module = "_noon_native")]
pub struct FamilySlicePlan {
    plan: Option<noon::FamilySlicePlan>,
}
impl FamilySlicePlan {
    fn value(&self) -> PyResult<&noon::FamilySlicePlan> {
        self.plan
            .as_ref()
            .ok_or_else(|| engine_error("family slice plan has been freed"))
    }
}
#[pymethods]
impl FamilySlicePlan {
    #[getter(includesSelf)]
    fn includes_self(&self) -> PyResult<bool> {
        Ok(self.value()?.includes_self())
    }
    #[getter(normalizationCount)]
    fn normalization_count(&self) -> PyResult<usize> {
        self.value()?.normalization_count().map_err(engine_error)
    }
    #[pyo3(name = "selectNormalized")]
    fn select_normalized(&self, start: f64, stop: f64, step: f64) -> PyResult<Option<Vec<String>>> {
        if [start, stop, step]
            .iter()
            .any(|v| !v.is_finite() || v.fract() != 0.)
        {
            return Err(engine_error(noon::integration::AuthoringFailure::new(
                "invalid_input",
                "authoring.invalid_slice_operand",
                "slice bounds and step must be finite integers",
            )));
        }
        self.value()?
            .select_normalized(start as i64, stop as i64, step as i64)
            .map(|entries| entries.map(|entries| entries.into_iter().map(entry_key).collect()))
            .map_err(engine_error)
    }
    fn free(&mut self) {
        self.plan.take();
    }
}
pub(crate) fn entry_handle(
    py: Python<'_>,
    entry: Option<noon::CompositeEntryHandle>,
) -> PyResult<Py<PyAny>> {
    match entry {
        None => Ok(py.None()),
        Some(noon::CompositeEntryHandle::Mobject(handle)) => {
            Ok(Py::new(py, MobjectHandle::from_semantic_mobject(handle))?.into_any())
        }
        Some(noon::CompositeEntryHandle::Family(family)) => {
            Ok(Py::new(py, FamilyHandle { family })?.into_any())
        }
    }
}
fn entry_key(entry: noon::CompositeEntryHandle) -> String {
    let node = match entry {
        noon::CompositeEntryHandle::Mobject(handle) => handle.node_id(),
        noon::CompositeEntryHandle::Family(handle) => handle.node_id(),
    };
    format!("{}:{}", node.slot(), node.generation())
}
pub(crate) fn inclusion(target: noon::MobjectTarget<'_>, include: Option<bool>) -> PyResult<bool> {
    include.map_or_else(
        || {
            target
                .get_num_points()
                .map(|count| count != 0)
                .map_err(engine_error)
        },
        Ok,
    )
}
pub(crate) fn split_iterator(
    target: noon::CompositeEntryHandle,
    include: Option<bool>,
) -> PyResult<FamilyIterator> {
    let include = inclusion(target.as_target(), include)?;
    target
        .split_iter_with_self(include)
        .map(|iterator| FamilyIterator {
            iterator: Some(iterator),
        })
        .map_err(engine_error)
}
pub(crate) fn split_count(
    target: noon::CompositeEntryHandle,
    include: Option<bool>,
) -> PyResult<usize> {
    target
        .split_count_with_self(inclusion(target.as_target(), include)?)
        .map_err(engine_error)
}
pub(crate) fn split_keys(
    target: noon::CompositeEntryHandle,
    include: Option<bool>,
) -> PyResult<Vec<String>> {
    target
        .split_members_with_self(inclusion(target.as_target(), include)?)
        .map(|values| values.into_iter().map(entry_key).collect())
        .map_err(engine_error)
}
pub(crate) fn split_at(
    py: Python<'_>,
    target: noon::CompositeEntryHandle,
    index: usize,
    include: Option<bool>,
) -> PyResult<Py<PyAny>> {
    entry_handle(
        py,
        target
            .split_at_with_self(index, inclusion(target.as_target(), include)?)
            .map_err(engine_error)?,
    )
}
pub(crate) fn slice_plan(
    target: noon::CompositeEntryHandle,
    include: Option<bool>,
) -> PyResult<FamilySlicePlan> {
    target
        .slice_plan_with_self(inclusion(target.as_target(), include)?)
        .map(|plan| FamilySlicePlan { plan: Some(plan) })
        .map_err(engine_error)
}
