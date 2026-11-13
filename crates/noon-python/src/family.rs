//! Native family handles project canonical Rust ownership and metadata only.
use crate::{context::MembershipBatch, engine_error, mobject::MobjectHandle};
use noon::integration::coordinate_failure;
use noon::ManimNumberPlane;
use noon::ManimPolarPlane;
use noon::{CoordinateAuthoringError, ManimAxes, ManimNumberLine, ManimThreeDAxes};
use pyo3::prelude::*;
use std::rc::Rc;

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
impl FamilyHandle {
    pub(crate) fn from_semantic_family(family: noon::MobjectFamily) -> Self {
        Self { family }
    }
}
#[pymethods]
impl FamilyHandle {
    #[pyo3(name = "becomeFamily")]
    fn become_family(
        &self,
        other: &Self,
        match_height: bool,
        match_width: bool,
        match_center: bool,
        stretch: bool,
    ) -> PyResult<()> {
        self.family
            .become_family(
                &other.family,
                noon::ManimBecomeOptions {
                    match_height,
                    match_width,
                    match_center,
                    stretch,
                },
            )
            .map_err(engine_error)
    }

    #[allow(clippy::too_many_arguments)]
    #[pyo3(name="setStyle",signature=(fill_enabled,fill_red,fill_green,fill_blue,fill_alpha,fill_opacity,stroke_enabled,stroke_red,stroke_green,stroke_blue,stroke_alpha,stroke_width,stroke_opacity,background_stroke_enabled,background_stroke_red,background_stroke_green,background_stroke_blue,background_stroke_alpha,background_stroke_width,background_stroke_opacity))]
    pub fn set_style(
        &mut self,
        fill_enabled: bool,
        fill_red: f64,
        fill_green: f64,
        fill_blue: f64,
        fill_alpha: f64,
        fill_opacity: Option<f64>,
        stroke_enabled: bool,
        stroke_red: f64,
        stroke_green: f64,
        stroke_blue: f64,
        stroke_alpha: f64,
        stroke_width: Option<f64>,
        stroke_opacity: Option<f64>,
        background_stroke_enabled: bool,
        background_stroke_red: f64,
        background_stroke_green: f64,
        background_stroke_blue: f64,
        background_stroke_alpha: f64,
        background_stroke_width: Option<f64>,
        background_stroke_opacity: Option<f64>,
    ) -> Result<(), PyErr> {
        let update = noon::integration::style_update(
            fill_enabled,
            fill_red,
            fill_green,
            fill_blue,
            fill_alpha,
            fill_opacity,
            stroke_enabled,
            stroke_red,
            stroke_green,
            stroke_blue,
            stroke_alpha,
            stroke_width,
            stroke_opacity,
        )
        .map_err(engine_error)?;
        let update = noon::integration::style_update_with_background_stroke(
            update,
            background_stroke_enabled,
            [
                background_stroke_red,
                background_stroke_green,
                background_stroke_blue,
                background_stroke_alpha,
            ],
            background_stroke_width,
            background_stroke_opacity,
        )
        .map_err(engine_error)?;
        self.family.set_style(update).map_err(engine_error)
    }
    #[pyo3(name = "worldFamilyCenter")]
    fn world_center(&self) -> PyResult<Vec<f64>> {
        self.family
            .world_center()
            .map(|v| vec![v.x, v.y, v.z])
            .map_err(engine_error)
    }
    #[pyo3(name = "shiftWorld")]
    fn shift_world(&mut self, x: f64, y: f64, z: f64) -> PyResult<()> {
        self.family
            .world_affine(noon::WorldAffineEdit::Shift(noon::SemanticVec3::new(
                x, y, z,
            )))
            .map_err(engine_error)
    }
    #[pyo3(name = "rotateWorld")]
    fn rotate_world(&mut self, angle: f64, axis: Vec<f64>, pivot: Vec<f64>) -> PyResult<()> {
        self.family
            .world_affine(noon::WorldAffineEdit::Rotate {
                axis: noon::integration::spatial_input::vec3(&axis, "axis")
                    .map_err(engine_error)?,
                radians: angle,
                about: noon::integration::spatial_input::about(&pivot).map_err(engine_error)?,
            })
            .map_err(engine_error)
    }
    #[pyo3(name = "scaleWorld")]
    fn scale_world(&mut self, factor: f64, pivot: Vec<f64>) -> PyResult<()> {
        self.family
            .world_affine(noon::WorldAffineEdit::Scale {
                factor,
                about: noon::integration::spatial_input::about(&pivot).map_err(engine_error)?,
            })
            .map_err(engine_error)
    }
    #[pyo3(name = "setFillByCheckerboard")]
    fn checkerboard(&self, first: Vec<f64>, second: Vec<f64>, opacity: f64) -> PyResult<()> {
        noon::SurfaceFamily::from_family(self.family.clone())
            .map_err(engine_error)?
            .set_fill_by_checkerboard(crate::spatial::checkerboard(&first, &second)?, opacity)
            .map_err(engine_error)
    }
    #[pyo3(name = "gridOptions")]
    #[pyo3(signature=(rows,columns,gap_x,gap_y))]
    pub fn grid_options(
        &self,
        rows: Option<u32>,
        columns: Option<u32>,
        gap_x: f64,
        gap_y: f64,
    ) -> FamilyGridOptions {
        FamilyGridOptions {
            options: noon::FamilyGridOptions {
                rows: rows.map(|v| v as usize),
                columns: columns.map(|v| v as usize),
                gap: (gap_x, gap_y),
                ..Default::default()
            },
        }
    }

    #[pyo3(name = "arrangeInGrid")]
    pub fn arrange_in_grid(&self, options: &FamilyGridOptions) -> Result<(), PyErr> {
        self.family
            .clone()
            .arrange_in_grid_with_options(&options.options)
            .map_err(engine_error)
    }

    #[pyo3(name = "arrangeOptions")]
    #[allow(clippy::too_many_arguments)]
    pub fn arrange_options(
        &self,
        direction_x: f64,
        direction_y: f64,
        buff: f64,
        center: bool,
        edge_x: f64,
        edge_y: f64,
        mask_x: f64,
        mask_y: f64,
        member_index: Option<i32>,
    ) -> FamilyArrangeOptions {
        let mut options = noon::FamilyArrangeOptions::new(direction_x, direction_y, buff, center);
        options.placement.aligned_edge = (edge_x, edge_y);
        options.placement.mask = (mask_x, mask_y);
        options.member_index = member_index.map(|index| index as isize);
        FamilyArrangeOptions { options }
    }

    /// Arrange authored state with all options in one semantic transaction.
    pub fn arrange(&self, options: &FamilyArrangeOptions) -> Result<(), PyErr> {
        self.family
            .clone()
            .arrange_with_options(&options.options)
            .map_err(engine_error)
    }

    #[pyo3(name = "setObjectOpacity")]
    fn object_opacity(&self, value: f64) -> PyResult<()> {
        self.family.set_object_opacity(value).map_err(engine_error)
    }

    #[pyo3(name = "setEmission")]
    fn emission(&self, value: f64) -> PyResult<()> {
        self.family.set_emission(value).map_err(engine_error)
    }

    #[cfg(feature = "native-text")]
    #[pyo3(name = "numberLabelFamily")]
    fn number_labels(
        &self,
        values: Vec<f64>,
        automatic: bool,
        options: &crate::coordinates::NumberLabelOptions,
        attach: bool,
    ) -> PyResult<Self> {
        use crate::coordinates::label_failure;
        if automatic && !values.is_empty() {
            return Err(label_failure(
                "automatic labels cannot supply explicit values",
            ));
        }
        let line =
            noon::ManimNumberLine::from_family(self.family.clone()).map_err(label_failure)?;
        let values = if automatic {
            None
        } else {
            Some(values.as_slice())
        };
        let family = if attach {
            line.add_numbers(values, &options.options)
        } else {
            line.get_number_mobjects(values, &options.options)
        };
        family
            .map(Self::from_semantic_family)
            .map_err(label_failure)
    }
    #[cfg(feature = "native-text")]
    #[pyo3(name = "numberLabelMembers")]
    fn number_label_members(&self) -> PyResult<Vec<(MobjectHandle, String)>> {
        use crate::coordinates::label_failure;
        let store = self.family.integration_store();
        let members = store
            .borrow()
            .semantic_family_members_checked(self.family.node_id())
            .map_err(label_failure)?;
        members
            .into_iter()
            .map(|member| {
                let object =
                    noon::Mobject::from_node(Rc::clone(store), member).map_err(label_failure)?;
                let handle = object
                    .state()
                    .map_err(label_failure)?
                    .content
                    .text()
                    .ok_or_else(|| label_failure("label member is not Text"))?;
                let source = store
                    .borrow()
                    .text_resources()
                    .get(handle)
                    .ok_or_else(|| label_failure("label text resource is missing"))?
                    .source
                    .to_string();
                Ok((MobjectHandle::from_semantic_mobject(object), source))
            })
            .collect()
    }

    #[pyo3(name = "memberIsFamily")]
    pub fn member_is_family(&self, index: usize) -> Result<bool, PyErr> {
        let family = self.family.clone();
        let store = family.integration_store();
        let member = store
            .borrow()
            .semantic_family_members_checked(family.node_id())
            .map_err(engine_error)?
            .get(index)
            .copied()
            .ok_or_else(|| engine_error("family member index is out of bounds"))?;
        let is_family = store.borrow().semantic_family_checked(member).is_ok();
        Ok(is_family)
    }

    #[pyo3(name = "memberFamily")]
    pub fn member_family(&self, index: usize) -> Result<FamilyHandle, PyErr> {
        let family = self.family.clone();
        let store = family.integration_store();
        let member = store
            .borrow()
            .semantic_family_members_checked(family.node_id())
            .map_err(engine_error)?
            .get(index)
            .copied()
            .ok_or_else(|| engine_error("family member index is out of bounds"))?;
        noon::MobjectFamily::from_node(std::rc::Rc::clone(store), member)
            .map(FamilyHandle::from_semantic_family)
            .map_err(engine_error)
    }

    #[pyo3(name = "polarPlanePart")]
    pub fn polar_plane_part(&self, index: u32) -> Result<Self, PyErr> {
        let plane = ManimPolarPlane::from_family(self.family.clone())
            .map_err(coordinate_failure)
            .map_err(engine_error)?;
        let family = match index {
            0 => plane.faded_lines(),
            1 => plane.background_lines(),
            2 => plane.x_axis().map(|axis| axis.family().clone()),
            3 => plane.y_axis().map(|axis| axis.family().clone()),
            _ => {
                return Err(engine_error(
                    "PolarPlane part index must be between zero and three",
                ))
            }
        }
        .map_err(coordinate_failure)
        .map_err(engine_error)?;
        Ok(Self::from_semantic_family(family))
    }
    #[pyo3(name = "polarPlaneFrame")]
    pub fn polar_plane_frame(&self) -> Result<crate::coordinates::PolarObservation, PyErr> {
        ManimPolarPlane::from_family(self.family.clone())
            .and_then(|plane| plane.authored_polar_frame())
            .map(|frame| crate::coordinates::PolarObservation { frame })
            .map_err(coordinate_failure)
            .map_err(engine_error)
    }
    #[pyo3(name = "numberPlaneGridLineIndices")]
    pub fn number_plane_grid_line_indices(&self, horizontal: bool) -> Result<Vec<u32>, PyErr> {
        ManimNumberPlane::from_family(self.family.clone())
            .and_then(|plane| plane.grid_line_indices(horizontal))
            .map_err(coordinate_failure)
            .map_err(engine_error)
    }

    /// Return authoritative root members in their painter order, without new IDs.
    #[pyo3(name = "numberPlanePart")]
    pub fn number_plane_part(&self, index: u32) -> Result<Self, PyErr> {
        let plane = ManimNumberPlane::from_family(self.family.clone())
            .map_err(coordinate_failure)
            .map_err(engine_error)?;
        let family = match index {
            0 => plane.faded_lines(),
            1 => plane.background_lines(),
            2 => plane.x_axis().map(|axis| axis.family().clone()),
            3 => plane.y_axis().map(|axis| axis.family().clone()),
            _ => {
                return Err(engine_error(
                    "NumberPlane part index must be between zero and three",
                ))
            }
        }
        .map_err(coordinate_failure)
        .map_err(engine_error)?;
        Ok(Self::from_semantic_family(family))
    }

    #[pyo3(name = "numberPlaneFrame")]
    pub fn number_plane_frame(&self) -> Result<crate::coordinates::AxesObservation, PyErr> {
        ManimNumberPlane::from_family(self.family.clone())
            .and_then(|plane| plane.authored_frame())
            .map(|frame| crate::coordinates::AxesObservation { frame })
            .map_err(coordinate_failure)
            .map_err(engine_error)
    }
    /// Only wrapper identities are reconstructed; ranges remain on the shafts.
    #[pyo3(name = "coordinateAxis")]
    pub fn coordinate_axis(&self, index: u32) -> Result<FamilyHandle, PyErr> {
        let axes = ManimAxes::from_family(self.family.clone())
            .map_err(coordinate_failure)
            .map_err(engine_error)?;
        let axis = match index {
            0 => axes.x_axis(),
            1 => axes.y_axis(),
            _ => return Err(engine_error("coordinate axis index must be zero or one")),
        }
        .map_err(coordinate_failure)
        .map_err(engine_error)?;
        Ok(Self::from_semantic_family(axis.family().clone()))
    }

    #[pyo3(name = "threeDAxesAxis")]
    pub fn three_d_axes_axis(&self, index: u32) -> Result<FamilyHandle, PyErr> {
        let axes = ManimThreeDAxes::from_family(self.family.clone())
            .map_err(coordinate_failure)
            .map_err(engine_error)?;
        axes.axis(index as usize)
            .map(|axis| Self::from_semantic_family(axis.family().clone()))
            .map_err(coordinate_failure)
            .map_err(engine_error)
    }

    #[pyo3(name = "threeDAxesTip")]
    pub fn three_d_axes_tip(&self, index: u32) -> Result<Option<MobjectHandle>, PyErr> {
        let axes = ManimThreeDAxes::from_family(self.family.clone())
            .map_err(coordinate_failure)
            .map_err(engine_error)?;
        axes.tip(index as usize)
            .map(|tip| tip.map(MobjectHandle::from_semantic_mobject))
            .map_err(coordinate_failure)
            .map_err(engine_error)
    }

    /// Family-valued variant for existing multi-leaf retained TeX labels.
    #[pyo3(name = "threeDAxesLabelFamilies")]
    pub fn three_d_axes_label_families(
        &self,
        x: &FamilyHandle,
        y: &FamilyHandle,
        z: &FamilyHandle,
        selected_axes: u32,
        buff: f64,
        fixed_orientation: bool,
    ) -> Result<FamilyHandle, PyErr> {
        // wasm-bindgen cannot represent optional borrowed exported handles.
        // Inactive arguments alias an active handle; this mask preserves one
        // typed, atomic call without consuming any caller-owned wrappers.
        if selected_axes == 0 || selected_axes & !7 != 0 {
            return Err(engine_error(
                "axis labels require a nonempty three-axis mask",
            ));
        }
        let axes = ManimThreeDAxes::from_family(self.family.clone())
            .map_err(coordinate_failure)
            .map_err(engine_error)?;
        let mut families = Vec::new();
        for (index, handle) in [(0, x), (1, y), (2, z)] {
            if selected_axes & (1 << index) != 0 {
                families.push((index, handle.family.clone()));
            }
        }
        let labels = families
            .iter()
            .map(|(index, family)| (*index, noon::MobjectTarget::Family(family)))
            .collect::<Vec<_>>();
        axes.create_axis_label_targets(&labels, buff, fixed_orientation)
            .map(Self::from_semantic_family)
            .map_err(coordinate_failure)
            .map_err(engine_error)
    }

    #[pyo3(name = "coordinateShaft")]
    pub fn coordinate_shaft(&self) -> Result<MobjectHandle, PyErr> {
        ManimNumberLine::from_family(self.family.clone())
            .and_then(|line| line.shaft())
            .map(MobjectHandle::from_semantic_mobject)
            .map_err(coordinate_failure)
            .map_err(engine_error)
    }

    #[pyo3(name = "coordinateTicks")]
    pub fn coordinate_ticks(&self) -> Result<FamilyHandle, PyErr> {
        ManimNumberLine::from_family(self.family.clone())
            .and_then(|line| line.ticks())
            .map(Self::from_semantic_family)
            .map_err(coordinate_failure)
            .map_err(engine_error)
    }

    /// Constructor-only wrapper materialization, bounded by this line's ticks.
    #[pyo3(name = "coordinateTickObjects")]
    fn coordinate_tick_objects(&self) -> PyResult<Vec<MobjectHandle>> {
        let ticks = ManimNumberLine::from_family(self.family.clone())
            .and_then(|v| v.ticks())
            .map_err(coordinate_failure)
            .map_err(engine_error)?;
        Self::from_semantic_family(ticks).direct_mobjects()
    }
    #[pyo3(name = "coordinateTip")]
    pub fn coordinate_tip(&self) -> Result<MobjectHandle, PyErr> {
        ManimNumberLine::from_family(self.family.clone())
            .and_then(|line| line.tip()?.ok_or(CoordinateAuthoringError::InvalidTopology))
            .map(MobjectHandle::from_semantic_mobject)
            .map_err(coordinate_failure)
            .map_err(engine_error)
    }

    /// Batch direct leaf handles once, avoiding one full membership scan per leaf.
    #[pyo3(name = "directMobjects")]
    fn direct_mobjects(&self) -> PyResult<Vec<MobjectHandle>> {
        let store = self.family.integration_store().borrow();
        let node = store
            .semantic_family_checked(self.family.node_id())
            .map_err(engine_error)?;
        node.members_iter()
            .map(|id| {
                noon::Mobject::from_node(Rc::clone(self.family.integration_store()), id)
                    .map(MobjectHandle::from_semantic_mobject)
                    .map_err(engine_error)
            })
            .collect()
    }
    #[pyo3(name = "numberLineFrame")]
    pub fn number_line_frame(&self) -> Result<crate::coordinates::NumberLineObservation, PyErr> {
        ManimNumberLine::from_family(self.family.clone())
            .and_then(|line| line.authored_frame())
            .map(|frame| crate::coordinates::NumberLineObservation { frame })
            .map_err(coordinate_failure)
            .map_err(engine_error)
    }

    #[pyo3(name = "axesFrame")]
    pub fn axes_frame(&self) -> Result<crate::coordinates::AxesObservation, PyErr> {
        ManimAxes::from_family(self.family.clone())
            .and_then(|axes| axes.authored_frame())
            .map(|frame| crate::coordinates::AxesObservation { frame })
            .map_err(coordinate_failure)
            .map_err(engine_error)
    }

    #[pyo3(name = "threeDAxesFrame")]
    pub fn three_d_axes_frame(&self) -> Result<crate::coordinates::ThreeDAxesObservation, PyErr> {
        ManimThreeDAxes::from_family(self.family.clone())
            .and_then(|axes| axes.authored_frame())
            .map(|frame| crate::coordinates::ThreeDAxesObservation { frame })
            .map_err(coordinate_failure)
            .map_err(engine_error)
    }

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
    #[pyo3(name = "extremumFromPoints")]
    fn extremum_from_points(
        &self,
        coordinates: Vec<f64>,
        dimensions: usize,
        dim: usize,
        key: f64,
    ) -> PyResult<f64> {
        noon::reduce_point_extremum(&coordinates, dimensions, dim, key).map_err(engine_error)
    }

    #[pyo3(name = "pathBoundaryPoints3D")]
    fn path_boundary_points_3d(&self) -> PyResult<Vec<f64>> {
        noon::MobjectTarget::from(&self.family)
            .path_boundary_points_3d()
            .map(crate::path_input::flat_points_3d)
            .map_err(engine_error)
    }

    #[pyo3(name = "editMembership")]
    fn edit_membership(&self, batch: &MembershipBatch) -> PyResult<Vec<bool>> {
        batch.edit_children((&self.family).into())
    }

    #[pyo3(name = "insertMember")]
    fn insert_member(&self, index: usize, batch: &MembershipBatch) -> PyResult<()> {
        self.family
            .insert_member(index, batch.single_member()?)
            .map_err(engine_error)
    }

    #[pyo3(name = "replaceMembers")]
    fn replace_members(&self, batch: &MembershipBatch) -> PyResult<()> {
        self.family
            .replace_members(&batch.add_targets()?)
            .map_err(engine_error)
    }

    #[pyo3(name = "invertMembers")]
    fn invert_members(&self, recursive: bool) -> PyResult<()> {
        self.family.invert(recursive).map_err(engine_error)
    }

    #[pyo3(name = "promoteOwnPoints")]
    fn promote_own_points(&self, references: &MembershipBatch) -> PyResult<FamilyCopy> {
        noon::MobjectTarget::from(&self.family)
            .promote_own_points_with_references(&references.add_targets()?)
            .map(|copied| FamilyCopy { copied })
            .map_err(engine_error)
    }

    #[pyo3(name = "copyFamily")]
    fn copy_family(&self, references: &MembershipBatch) -> PyResult<FamilyCopy> {
        if references.kind != "add" {
            return Err(engine_error("family copy references require an add batch"));
        }
        self.family
            .copy_with_references(&references.targets())
            .map(|copied| FamilyCopy {
                copied: copied.into_entry_copy(),
            })
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
    #[pyo3(name = "alignOnFrame")]
    pub fn align_on_frame(
        &self,
        direction_x: f64,
        direction_y: f64,
        buff: f64,
    ) -> Result<(), PyErr> {
        self.layout
            .align_on_frame((direction_x, direction_y), buff)
            .map_err(engine_error)
    }

    #[pyo3(name = "shiftBy")]
    pub fn shift_by(&self, delta_x: f64, delta_y: f64, delta_z: Option<f64>) -> Result<(), PyErr> {
        self.layout
            .shift_3d(noon_core::SemanticVec3::new(
                delta_x,
                delta_y,
                delta_z.unwrap_or(0.0),
            ))
            .map_err(engine_error)
    }

    #[pyo3(name = "moveToPoint")]
    pub fn move_to_point(
        &self,
        point_x: f64,
        point_y: f64,
        aligned_edge_x: f64,
        aligned_edge_y: f64,
        mask_x: f64,
        mask_y: f64,
    ) -> Result<(), PyErr> {
        self.layout
            .move_to(
                noon::FamilyLayoutTarget::Point(point_x, point_y),
                (aligned_edge_x, aligned_edge_y),
                (mask_x, mask_y),
            )
            .map_err(engine_error)
    }

    #[pyo3(name = "moveToMobject")]
    pub fn move_to_mobject(
        &self,
        target: &MobjectHandle,
        aligned_edge_x: f64,
        aligned_edge_y: f64,
        mask_x: f64,
        mask_y: f64,
    ) -> Result<(), PyErr> {
        self.layout
            .move_to(
                noon::FamilyLayoutTarget::Mobject(&target.handle),
                (aligned_edge_x, aligned_edge_y),
                (mask_x, mask_y),
            )
            .map_err(engine_error)
    }

    #[pyo3(name = "moveToFamily")]
    pub fn move_to_family(
        &self,
        target: &FamilyLayoutObservation,
        aligned_edge_x: f64,
        aligned_edge_y: f64,
        mask_x: f64,
        mask_y: f64,
    ) -> Result<(), PyErr> {
        self.layout
            .move_to(
                noon::FamilyLayoutTarget::Family(&target.layout),
                (aligned_edge_x, aligned_edge_y),
                (mask_x, mask_y),
            )
            .map_err(engine_error)
    }

    #[pyo3(name = "alignToPoint")]
    pub fn align_to_point(
        &self,
        point_x: f64,
        point_y: f64,
        axis_x: f64,
        axis_y: f64,
    ) -> Result<(), PyErr> {
        self.layout
            .align_to(
                noon::FamilyLayoutTarget::Point(point_x, point_y),
                (axis_x, axis_y),
            )
            .map_err(engine_error)
    }

    #[pyo3(name = "alignToMobject")]
    pub fn align_to_mobject(
        &self,
        target: &MobjectHandle,
        axis_x: f64,
        axis_y: f64,
    ) -> Result<(), PyErr> {
        self.layout
            .align_to(
                noon::FamilyLayoutTarget::Mobject(&target.handle),
                (axis_x, axis_y),
            )
            .map_err(engine_error)
    }

    #[pyo3(name = "alignToFamily")]
    pub fn align_to_family(
        &self,
        target: &FamilyLayoutObservation,
        axis_x: f64,
        axis_y: f64,
    ) -> Result<(), PyErr> {
        self.layout
            .align_to(
                noon::FamilyLayoutTarget::Family(&target.layout),
                (axis_x, axis_y),
            )
            .map_err(engine_error)
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
    pub(crate) copied: noon::SemanticCopy<noon::CompositeEntryHandle>,
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

/// Host-normalized options for one shared arrangement transaction.
#[pyclass(unsendable, module = "_noon_native")]
pub struct FamilyArrangeOptions {
    pub(crate) options: noon::FamilyArrangeOptions,
}

#[pymethods]
impl FamilyArrangeOptions {
    fn free(&self) {}
    #[pyo3(name = "setAligner")]
    pub fn set_aligner(&mut self, aligner: &crate::mobject::LayoutAnchor) {
        self.options.aligner = Some(aligner.anchor.clone());
    }
}

/// Transient grid call arguments; all sizing and validation belong to shared Rust.
#[pyclass(unsendable, module = "_noon_native")]
pub struct FamilyGridOptions {
    pub(crate) options: noon::FamilyGridOptions,
}

#[pymethods]
impl FamilyGridOptions {
    fn free(&self) {}
    #[pyo3(name = "setAlignment")]
    pub fn set_alignment(&mut self, x: f64, y: f64, rows: Option<String>, columns: Option<String>) {
        self.options.cell_alignment = (x, y);
        self.options.row_alignments = rows;
        self.options.column_alignments = columns;
    }
    #[pyo3(name = "setFlow")]
    pub fn set_flow(&mut self, flow: &str) -> Result<(), PyErr> {
        self.options.flow = flow.parse().map_err(engine_error)?;
        Ok(())
    }
    #[pyo3(name = "setSizeLists")]
    pub fn set_size_lists(&mut self, rows: bool, columns: bool) {
        self.options.row_heights = rows.then(Vec::new);
        self.options.column_widths = columns.then(Vec::new);
    }
    #[pyo3(name = "addRowHeight")]
    pub fn add_row_height(&mut self, value: Option<f64>) {
        self.options
            .row_heights
            .get_or_insert_with(Vec::new)
            .push(value);
    }
    #[pyo3(name = "addColumnWidth")]
    pub fn add_column_width(&mut self, value: Option<f64>) {
        self.options
            .column_widths
            .get_or_insert_with(Vec::new)
            .push(value);
    }
}
