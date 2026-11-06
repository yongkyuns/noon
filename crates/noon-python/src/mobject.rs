//! Store-scoped native handles; all geometry/state behavior is shared Rust.
use crate::{context::MembershipBatch, engine_error};
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
#[allow(clippy::too_many_arguments)]
impl MobjectHandle {
    #[pyo3(name = "prependMembers")]
    fn prepend_members(&self, batch: &MembershipBatch) -> PyResult<()> {
        if batch.kind != "add" {
            return Err(engine_error("member prepend requires an add batch"));
        }
        self.handle
            .prepend_many(&batch.targets())
            .map_err(engine_error)
    }

    #[pyo3(name="splitIterator",signature=(include=None))]
    fn split_iterator(&self, include: Option<bool>) -> PyResult<crate::family::FamilyIterator> {
        crate::family::split_iterator(
            noon::CompositeEntryHandle::Mobject(self.handle.clone()),
            include,
        )
    }
    #[pyo3(name="splitCount",signature=(include=None))]
    fn split_count(&self, include: Option<bool>) -> PyResult<usize> {
        crate::family::split_count(
            noon::CompositeEntryHandle::Mobject(self.handle.clone()),
            include,
        )
    }
    #[pyo3(name="splitKeys",signature=(include=None))]
    fn split_keys(&self, include: Option<bool>) -> PyResult<Vec<String>> {
        crate::family::split_keys(
            noon::CompositeEntryHandle::Mobject(self.handle.clone()),
            include,
        )
    }
    #[pyo3(name="splitAt",signature=(index,include=None))]
    fn split_at(&self, py: Python<'_>, index: usize, include: Option<bool>) -> PyResult<Py<PyAny>> {
        crate::family::split_at(
            py,
            noon::CompositeEntryHandle::Mobject(self.handle.clone()),
            index,
            include,
        )
    }
    #[pyo3(name="prepareSlice",signature=(include=None))]
    fn slice_plan(&self, include: Option<bool>) -> PyResult<crate::family::FamilySlicePlan> {
        crate::family::slice_plan(
            noon::CompositeEntryHandle::Mobject(self.handle.clone()),
            include,
        )
    }
    #[pyo3(name = "vectorBoundaryPoints3D")]
    fn vector_boundary_points_3d(&self) -> PyResult<Vec<f64>> {
        noon::MobjectTarget::from(&self.handle)
            .vector_boundary_points_3d()
            .map(crate::path_input::flat_points_3d)
            .map_err(engine_error)
    }

    #[pyo3(name = "cubicBezierPointRanges")]
    pub fn cubic_bezier_point_ranges(
        &self,
        point_count: u32,
        points_per_curve: f64,
    ) -> Result<Vec<u32>, PyErr> {
        self.handle.validate().map_err(engine_error)?;
        Ok(
            noon::manim_cubic_bezier_point_ranges(point_count as usize, points_per_curve)
                .into_iter()
                .map(|index| index as u32)
                .collect(),
        )
    }

    #[pyo3(name = "subpathSplitIndices")]
    pub fn subpath_split_indices(
        &self,
        points: Vec<f64>,
        point_count: u32,
        point_dimensions: u32,
        compared_dimensions: u32,
        absolute_tolerance: f64,
    ) -> Result<Vec<u32>, PyErr> {
        self.handle.validate().map_err(engine_error)?;
        noon::manim_subpath_split_indices(
            &points,
            point_count as usize,
            point_dimensions as usize,
            compared_dimensions as usize,
            absolute_tolerance,
        )
        .map(|indices| indices.into_iter().map(|index| index as u32).collect())
        .map_err(engine_error)
    }

    #[pyo3(name = "subpathRanges")]
    pub fn subpath_ranges(
        &self,
        points: Vec<f64>,
        point_count: u32,
        point_dimensions: u32,
        compared_dimensions: u32,
        points_per_curve: f64,
        absolute_tolerance: f64,
        drop_short_ranges: bool,
    ) -> Result<Vec<u32>, PyErr> {
        self.handle.validate().map_err(engine_error)?;
        noon::manim_subpath_ranges(
            &points,
            point_count as usize,
            point_dimensions as usize,
            compared_dimensions as usize,
            points_per_curve,
            absolute_tolerance,
            drop_short_ranges,
        )
        .map(|indices| indices.into_iter().map(|index| index as u32).collect())
        .map_err(engine_error)
    }

    #[pyo3(name = "pointsAllClose")]
    pub fn points_allclose(
        &self,
        left: Vec<f64>,
        right: Vec<f64>,
        absolute_tolerance: f64,
    ) -> Result<bool, PyErr> {
        self.handle.validate().map_err(engine_error)?;
        if left.len() != right.len() {
            return Err(engine_error(noon::AuthoringError::InvalidPointCoordinates(
                left.len().max(right.len()),
            )));
        }
        Ok(noon::manim_points_allclose(
            &left,
            &right,
            absolute_tolerance,
        ))
    }

    #[pyo3(name = "points2dClose")]
    pub fn points_2d_close(
        &self,
        left: Vec<f64>,
        right: Vec<f64>,
        absolute_tolerance: f64,
    ) -> Result<bool, PyErr> {
        self.handle.validate().map_err(engine_error)?;
        if left.len() < 2 || right.len() < 2 {
            return Err(engine_error(noon::AuthoringError::InvalidPointCoordinates(
                left.len().min(right.len()),
            )));
        }
        Ok(noon::manim_points_2d_close(
            [left[0], left[1]],
            [right[0], right[1]],
            absolute_tolerance,
        ))
    }

    #[pyo3(name = "pointArray")]
    fn point_array(&self) -> PyResult<(Vec<f64>, Vec<usize>)> {
        noon::MobjectTarget::from(&self.handle)
            .point_array()
            .map(crate::path_input::point_array)
            .map_err(engine_error)
    }
    #[getter(pointCount)]
    fn point_count(&self) -> PyResult<usize> {
        self.handle.get_num_points().map_err(engine_error)
    }

    #[pyo3(name = "backgroundStrokeColor")]
    fn background_stroke_color(&self) -> PyResult<Option<crate::family::ColorObservation>> {
        self.handle
            .background_stroke_color()
            .map(|v| v.map(Into::into))
            .map_err(engine_error)
    }
    #[getter(backgroundStrokeWidth)]
    fn background_stroke_width(&self) -> PyResult<f64> {
        self.handle.background_stroke_width().map_err(engine_error)
    }
    #[getter(backgroundStrokeOpacity)]
    fn background_stroke_opacity(&self) -> PyResult<f64> {
        self.handle
            .background_stroke_opacity()
            .map_err(engine_error)
    }

    #[pyo3(name="setFillImage",signature=(paint,opacity))]
    fn fill_image(
        &self,
        paint: &crate::paints::ImagePaintOptions,
        opacity: Option<f64>,
    ) -> PyResult<()> {
        self.handle
            .clone()
            .set_fill_image(paint.options.clone(), opacity)
            .map_err(engine_error)
    }

    #[pyo3(name="setFillImageMapping",signature=(paint,opacity))]
    fn fill_image_mapping(
        &self,
        paint: &crate::paints::ImageMapping,
        opacity: Option<f64>,
    ) -> PyResult<()> {
        self.handle
            .clone()
            .set_fill_image_mapping(paint.value, opacity)
            .map_err(engine_error)
    }

    #[pyo3(name="setFillGradient", signature=(paint,opacity))]
    fn fill_gradient(
        &self,
        paint: &crate::paints::LinearGradient,
        opacity: Option<f64>,
    ) -> PyResult<()> {
        self.handle
            .clone()
            .set_fill_gradient(paint.value.clone(), opacity)
            .map_err(engine_error)
    }

    #[pyo3(name="setFillRadialGradient", signature=(paint,opacity))]
    fn fill_radial_gradient(
        &self,
        paint: &crate::paints::RadialGradient,
        opacity: Option<f64>,
    ) -> PyResult<()> {
        self.handle
            .clone()
            .set_fill_radial_gradient(paint.value.clone(), opacity)
            .map_err(engine_error)
    }

    #[pyo3(name="setFillNoise", signature=(paint,opacity))]
    fn fill_noise(&self, paint: &crate::paints::NoisePaint, opacity: Option<f64>) -> PyResult<()> {
        self.handle
            .clone()
            .set_fill_noise(paint.value.clone(), opacity)
            .map_err(engine_error)
    }

    #[pyo3(name="setFillHatch", signature=(paint,opacity))]
    fn fill_hatch(&self, paint: &crate::paints::HatchPaint, opacity: Option<f64>) -> PyResult<()> {
        self.handle
            .clone()
            .set_fill_hatch(paint.value.clone(), opacity)
            .map_err(engine_error)
    }

    #[pyo3(name = "setGlow")]
    fn set_glow(&self, update: &crate::effects::EffectUpdate) -> PyResult<()> {
        self.handle
            .clone()
            .set_glow(update.value.clone())
            .map_err(engine_error)
    }
    #[pyo3(name = "removeGlow")]
    fn remove_glow(&self) -> PyResult<()> {
        self.handle.clone().remove_glow().map_err(engine_error)
    }
    #[pyo3(name = "addEffect")]
    fn add_effect(
        &self,
        definition: &crate::effects::EffectDefinition,
        name: &str,
    ) -> PyResult<()> {
        self.handle
            .clone()
            .add_effect(definition.definition(), name)
            .map_err(engine_error)
    }
    #[pyo3(name = "getEffect")]
    fn get_effect(&self, name: &str) -> PyResult<crate::effects::EffectHandle> {
        self.handle
            .get_effect(name)
            .map(|handle| crate::effects::EffectHandle { handle })
            .map_err(engine_error)
    }
    #[pyo3(name = "setEffect")]
    fn set_effect(&self, name: &str, update: &crate::effects::EffectUpdate) -> PyResult<()> {
        self.handle
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
        self.handle
            .clone()
            .set_effect(&handle.handle, update.value.clone())
            .map_err(engine_error)
    }
    #[pyo3(name = "removeEffect")]
    fn remove_effect(&self, name: &str) -> PyResult<()> {
        self.handle
            .clone()
            .remove_effect(name)
            .map_err(engine_error)
    }
    #[pyo3(name = "removeEffectHandle")]
    fn remove_effect_handle(&self, handle: &crate::effects::EffectHandle) -> PyResult<()> {
        self.handle
            .clone()
            .remove_effect(&handle.handle)
            .map_err(engine_error)
    }

    #[pyo3(name = "familyKeys")]
    fn family_keys(&self) -> PyResult<Vec<String>> {
        noon::MobjectTarget::from(&self.handle)
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
        noon::MobjectTarget::from(&self.handle)
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
        let target = noon::MobjectTarget::from(&self.handle);
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
        noon::MobjectTarget::from(&self.handle)
            .all_points_3d()
            .map(|points| points.into_iter().flat_map(|p| [p.x, p.y, p.z]).collect())
            .map_err(engine_error)
    }

    #[pyo3(name = "fillColor")]
    fn fill_color(&self) -> PyResult<Option<crate::family::ColorObservation>> {
        self.handle
            .fill_color()
            .map(|color| color.map(Into::into))
            .map_err(engine_error)
    }
    #[pyo3(name = "strokeColor")]
    fn stroke_color(&self) -> PyResult<Option<crate::family::ColorObservation>> {
        self.handle
            .stroke_color()
            .map(|color| color.map(Into::into))
            .map_err(engine_error)
    }
    #[pyo3(name = "imagePixelWidth")]
    fn image_pixel_width(&self) -> PyResult<u32> {
        self.handle
            .image_dimensions()
            .map(|(width, _)| width)
            .map_err(engine_error)
    }
    #[pyo3(name = "imagePixelHeight")]
    fn image_pixel_height(&self) -> PyResult<u32> {
        self.handle
            .image_dimensions()
            .map(|(_, height)| height)
            .map_err(engine_error)
    }
    #[pyo3(name = "setImageSampling")]
    fn set_image_sampling(&mut self, value: &str) -> PyResult<()> {
        let sampling = noon::integration::image_sampling(value).map_err(engine_error)?;
        self.handle
            .set_image_sampling(sampling)
            .map_err(engine_error)
    }

    #[pyo3(name = "manimColor")]
    fn manim_color(&self) -> PyResult<crate::family::ColorObservation> {
        self.handle
            .manim_color()
            .map(crate::family::ColorObservation::from)
            .map_err(engine_error)
    }
    #[pyo3(name = "applyMatrix")]
    pub fn apply_matrix(
        &mut self,
        values: Vec<f64>,
        rows: u32,
        columns: u32,
        about_x: f64,
        about_y: f64,
    ) -> Result<(), PyErr> {
        self.handle
            .apply_matrix(&values, rows as usize, columns as usize, about_x, about_y)
            .map_err(engine_error)
    }

    /// Replace raw world-space points through the shared semantic transaction.
    #[pyo3(name = "setPoints")]
    pub fn set_points(&mut self, values: Vec<f64>) -> Result<(), PyErr> {
        let points = crate::path_input::points_f64(&values)?;
        self.handle.set_points_f64(&points).map_err(engine_error)
    }

    #[pyo3(name = "setEmptyPointArray")]
    pub fn set_empty_point_array(&mut self, dimensions: Vec<f64>) -> Result<(), PyErr> {
        self.handle
            .set_empty_point_array(crate::path_input::point_array_shape(&dimensions)?)
            .map_err(engine_error)
    }

    #[pyo3(name = "setPoints3D")]
    pub fn set_points_3d(&mut self, values: Vec<f64>) -> Result<(), PyErr> {
        let points = crate::path_input::points_3d(&values)?;
        self.handle.set_points_3d(&points).map_err(engine_error)
    }

    #[pyo3(name = "appendPoints")]
    pub fn append_points(&mut self, values: Vec<f64>) -> Result<(), PyErr> {
        let points = crate::path_input::points_f64(&values)?;
        self.handle.append_points_f64(&points).map_err(engine_error)
    }

    #[pyo3(name = "appendPoints3D")]
    pub fn append_points_3d(&mut self, values: Vec<f64>) -> Result<(), PyErr> {
        let points = crate::path_input::points_3d(&values)?;
        self.handle.append_points_3d(&points).map_err(engine_error)
    }

    /// Replace world-space corners through the shared semantic transaction.
    #[pyo3(name = "setPointsAsCorners")]
    pub fn set_points_as_corners(&mut self, values: Vec<f64>) -> Result<(), PyErr> {
        let points = crate::path_input::points_3d(&values)?;
        self.handle
            .set_points_as_corners_3d(&points)
            .map_err(engine_error)
    }

    #[pyo3(name = "setPointsAsCornersWithSamples")]
    pub fn set_points_as_corners_with_samples(
        &mut self,
        values: Vec<f64>,
        sample_values: Vec<f64>,
    ) -> Result<(), PyErr> {
        let points = crate::path_input::points_3d(&values)?;
        let sample_values: [f64; 4] = sample_values.try_into().map_err(|_| {
            pyo3::exceptions::PyTypeError::new_err("set_points_as_corners requires four samples")
        })?;
        self.handle
            .set_points_as_corners_3d_with_samples(&points, &sample_values)
            .map_err(engine_error)
    }

    #[pyo3(name = "clearPoints")]
    pub fn clear_points(&mut self) -> Result<(), PyErr> {
        self.handle.clear_points().map_err(engine_error)
    }

    #[pyo3(name = "startNewPath")]
    pub fn start_new_path(
        &mut self,
        point_x: f64,
        point_y: f64,
        point_z: f64,
    ) -> Result<(), PyErr> {
        let point = crate::path_input::point_3d(point_x, point_y, point_z)?;
        self.handle.start_new_path_3d(point).map_err(engine_error)
    }

    #[pyo3(name = "startNewPathWithStride")]
    pub fn start_new_path_with_stride(
        &mut self,
        point_x: f64,
        point_y: f64,
        point_z: f64,
        stride: i32,
    ) -> Result<(), PyErr> {
        let point = crate::path_input::point_3d(point_x, point_y, point_z)?;
        self.handle
            .start_new_path_3d_with_stride(point, stride)
            .map_err(engine_error)
    }

    #[pyo3(name = "addLineTo")]
    #[pyo3(signature=(point_x,point_y,point_z,stride=None))]
    pub fn add_line_to(
        &mut self,
        point_x: f64,
        point_y: f64,
        point_z: f64,
        stride: Option<i32>,
    ) -> Result<(), PyErr> {
        let point = crate::path_input::point_3d(point_x, point_y, point_z)?;
        self.handle
            .add_line_to_3d_with_stride(point, stride.unwrap_or(4))
            .map_err(engine_error)
    }

    #[pyo3(name = "addLineToWithSamples")]
    pub fn add_line_to_with_samples(
        &mut self,
        point_x: f64,
        point_y: f64,
        point_z: f64,
        stride: i32,
        bezier_t_values: Vec<f64>,
    ) -> Result<(), PyErr> {
        let point = crate::path_input::point_3d(point_x, point_y, point_z)?;
        let bezier_t_values: [f64; 4] = bezier_t_values.try_into().map_err(|_| {
            engine_error("add_line_to requires four constructor interpolation samples")
        })?;
        self.handle
            .add_line_to_3d_with_stride_and_samples(point, stride, &bezier_t_values)
            .map_err(engine_error)
    }

    #[pyo3(name = "addQuadraticBezierCurveTo")]
    #[pyo3(signature=(control_x,control_y,control_z,anchor_x,anchor_y,anchor_z,stride=None))]
    pub fn add_quadratic_bezier_curve_to(
        &mut self,
        control_x: f64,
        control_y: f64,
        control_z: f64,
        anchor_x: f64,
        anchor_y: f64,
        anchor_z: f64,
        stride: Option<i32>,
    ) -> Result<(), PyErr> {
        let control = crate::path_input::point_3d(control_x, control_y, control_z)?;
        let anchor = crate::path_input::point_3d(anchor_x, anchor_y, anchor_z)?;
        self.handle
            .add_quadratic_bezier_curve_to_3d_with_stride(control, anchor, stride.unwrap_or(4))
            .map_err(engine_error)
    }

    #[pyo3(name = "addCubicBezierCurveTo")]
    #[pyo3(signature=(control1_x,control1_y,control1_z,control2_x,control2_y,control2_z,anchor_x,anchor_y,anchor_z,stride=None))]
    pub fn add_cubic_bezier_curve_to(
        &mut self,
        control1_x: f64,
        control1_y: f64,
        control1_z: f64,
        control2_x: f64,
        control2_y: f64,
        control2_z: f64,
        anchor_x: f64,
        anchor_y: f64,
        anchor_z: f64,
        stride: Option<i32>,
    ) -> Result<(), PyErr> {
        let control1 = crate::path_input::point_3d(control1_x, control1_y, control1_z)?;
        let control2 = crate::path_input::point_3d(control2_x, control2_y, control2_z)?;
        let anchor = crate::path_input::point_3d(anchor_x, anchor_y, anchor_z)?;
        self.handle
            .add_cubic_bezier_curve_to_3d_with_stride(
                control1,
                control2,
                anchor,
                stride.unwrap_or(4),
            )
            .map_err(engine_error)
    }

    #[pyo3(name = "addSmoothCurveTo")]
    #[pyo3(signature=(values,stride=None))]
    pub fn add_smooth_curve_to(
        &mut self,
        values: Vec<f64>,
        stride: Option<i32>,
    ) -> Result<(), PyErr> {
        let points = crate::path_input::points(&values)?;
        let (handle2, anchor) = match points.as_slice() {
            [anchor] => (None, *anchor),
            [handle2, anchor] => (Some(*handle2), *anchor),
            _ => {
                return Err(engine_error(
                    "Only call add_smooth_curve_to with 1 or 2 points",
                ));
            }
        };
        self.handle
            .add_smooth_curve_to_with_stride(handle2, anchor, stride.unwrap_or(4))
            .map_err(engine_error)
    }

    #[pyo3(name = "scaleHandleToAnchorDistances")]
    pub fn scale_handle_to_anchor_distances(&mut self, factor: f64) -> Result<(), PyErr> {
        self.handle
            .scale_handle_to_anchor_distances(factor)
            .map_err(engine_error)
    }

    #[pyo3(name = "addCubicBezierCurve")]
    pub fn add_cubic_bezier_curve(
        &mut self,
        anchor1_x: f64,
        anchor1_y: f64,
        anchor1_z: f64,
        handle1_x: f64,
        handle1_y: f64,
        handle1_z: f64,
        handle2_x: f64,
        handle2_y: f64,
        handle2_z: f64,
        anchor2_x: f64,
        anchor2_y: f64,
        anchor2_z: f64,
    ) -> Result<(), PyErr> {
        let anchor1 = crate::path_input::point_3d(anchor1_x, anchor1_y, anchor1_z)?;
        let handle1 = crate::path_input::point_3d(handle1_x, handle1_y, handle1_z)?;
        let handle2 = crate::path_input::point_3d(handle2_x, handle2_y, handle2_z)?;
        let anchor2 = crate::path_input::point_3d(anchor2_x, anchor2_y, anchor2_z)?;
        self.handle
            .add_cubic_bezier_curve_3d(anchor1, handle1, handle2, anchor2)
            .map_err(engine_error)
    }

    #[pyo3(name = "addCubicBezierCurves")]
    pub fn add_cubic_bezier_curves(&mut self, values: Vec<f64>) -> Result<(), PyErr> {
        let curves = crate::path_input::cubic_curves_3d(&values)?;
        self.handle
            .add_cubic_bezier_curves_3d(&curves)
            .map_err(engine_error)
    }

    #[pyo3(name = "setCubicBezierCurves")]
    pub fn set_cubic_bezier_curves(&mut self, values: Vec<f64>) -> Result<(), PyErr> {
        let curves = crate::path_input::cubic_curves_3d(&values)?;
        self.handle
            .set_cubic_bezier_curves_3d(&curves)
            .map_err(engine_error)
    }

    #[pyo3(name = "addPointsAsCorners")]
    pub fn add_points_as_corners(&mut self, values: Vec<f64>) -> Result<(), PyErr> {
        let points = crate::path_input::points_3d(&values)?;
        self.handle
            .add_points_as_corners_3d(&points)
            .map_err(engine_error)
    }

    #[pyo3(name = "addPointsAsCornersWithStride")]
    pub fn add_points_as_corners_with_stride(
        &mut self,
        values: Vec<f64>,
        stride: i32,
        bezier_t_values: Vec<f64>,
    ) -> Result<(), PyErr> {
        let points = crate::path_input::points_3d(&values)?;
        self.handle
            .add_points_as_corners_3d_with_stride(&points, stride, &bezier_t_values)
            .map_err(engine_error)
    }

    #[pyo3(name = "appendVectorizedMobject")]
    pub fn append_vectorized_mobject(&mut self, source: &MobjectHandle) -> Result<(), PyErr> {
        self.handle
            .append_vectorized_mobject(&source.handle)
            .map_err(engine_error)
    }

    #[pyo3(name = "appendVectorizedMobjectWithStride")]
    pub fn append_vectorized_mobject_with_stride(
        &mut self,
        source: &MobjectHandle,
        stride: i32,
    ) -> Result<(), PyErr> {
        self.handle
            .append_vectorized_mobject_with_stride(&source.handle, stride)
            .map_err(engine_error)
    }

    #[pyo3(name = "setPointsSmoothly")]
    pub fn set_points_smoothly(&mut self, coordinates: Vec<f64>) -> Result<(), PyErr> {
        self.handle
            .set_points_smoothly(&crate::path_input::points(&coordinates)?)
            .map_err(engine_error)
    }
    #[pyo3(name = "makeSmooth")]
    pub fn make_smooth(&mut self) -> Result<(), PyErr> {
        self.handle.make_smooth().map_err(engine_error)
    }
    #[pyo3(name = "makeJagged")]
    pub fn make_jagged(&mut self) -> Result<(), PyErr> {
        self.handle.make_jagged().map_err(engine_error)
    }
    #[pyo3(name = "insertNCurves")]
    pub fn insert_n_curves(&mut self, additional: u32) -> Result<(), PyErr> {
        self.handle
            .insert_n_curves(additional as usize)
            .map_err(engine_error)
    }
    #[pyo3(name = "reverseDirection")]
    pub fn reverse_direction(&mut self) -> Result<(), PyErr> {
        self.handle.reverse_direction().map_err(engine_error)
    }
    #[pyo3(name = "reversePoints")]
    pub fn reverse_points(&mut self) -> Result<(), PyErr> {
        self.handle.reverse_points().map_err(engine_error)
    }
    #[pyo3(name = "repeatPoints")]
    pub fn repeat_points(&mut self, count: u32) -> Result<(), PyErr> {
        self.handle
            .repeat_points(count as usize)
            .map_err(engine_error)
    }
    pub fn subcurve(&self, a: f64, b: f64) -> Result<MobjectHandle, PyErr> {
        self.handle
            .subcurve(a, b)
            .map(Self::from_semantic_mobject)
            .map_err(engine_error)
    }
    #[pyo3(name = "alignPoints")]
    pub fn align_points(&self, other: &MobjectHandle) -> Result<(), PyErr> {
        self.handle
            .align_points(&other.handle)
            .map_err(engine_error)
    }

    #[pyo3(name = "pointwiseBecomePartial")]
    pub fn pointwise_become_partial(
        &self,
        source: &MobjectHandle,
        a: f64,
        b: f64,
    ) -> Result<(), PyErr> {
        self.handle
            .clone()
            .pointwise_become_partial(&source.handle, a, b)
            .map_err(engine_error)
    }

    #[pyo3(name = "closePath")]
    pub fn close_path(&mut self) -> Result<(), PyErr> {
        self.handle.close_path().map_err(engine_error)
    }

    #[pyo3(name = "closePathWithStride")]
    pub fn close_path_with_stride(
        &mut self,
        stride: i32,
        tolerance: f64,
        bezier_t_values: Vec<f64>,
    ) -> Result<(), PyErr> {
        self.handle
            .close_path_with_stride(stride, tolerance, &bezier_t_values)
            .map_err(engine_error)
    }

    /// Share another vector object’s geometry and transform while preserving
    /// the source identity, paint, and painter priority.
    #[pyo3(name = "matchPoints")]
    pub fn match_points(&mut self, target: &MobjectHandle) -> Result<(), PyErr> {
        self.handle
            .match_points(&target.handle)
            .map_err(engine_error)
    }

    #[pyo3(name = "pathQuery")]
    fn path_query(&self) -> PyResult<crate::path_query::PathQuery> {
        self.handle
            .path_query()
            .map(crate::path_query::PathQuery::new)
            .map_err(engine_error)
    }
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
    #[pyo3(signature=(x,y,z=None))]
    pub fn shift(&mut self, x: f64, y: f64, z: Option<f64>) -> Result<(), PyErr> {
        self.handle
            .shift_3d(noon::SemanticVec3::new(x, y, z.unwrap_or(0.0)))
            .map_err(engine_error)
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
    #[getter(strokeWidth)]
    fn stroke_width(&self) -> PyResult<f64> {
        self.handle.stroke_width().map_err(engine_error)
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
        LayoutAnchor::from_anchor(noon::LayoutAnchor::from(&self.handle), index)
    }
}
#[pyclass(unsendable, skip_from_py_object, module = "_noon_native")]
#[derive(Clone)]
pub struct LayoutAnchor {
    pub(crate) anchor: noon::LayoutAnchor,
}
impl LayoutAnchor {
    pub(crate) fn from_anchor(anchor: noon::LayoutAnchor, index: Option<isize>) -> Self {
        Self {
            anchor: match index {
                Some(index) => anchor.member(index),
                None => anchor,
            },
        }
    }
}
#[pymethods]
impl LayoutAnchor {
    #[pyo3(name = "nextToEdgeInFrame")]
    #[allow(clippy::too_many_arguments)]
    pub fn next_to_edge_in_frame(
        &self,
        target: &LayoutAnchor,
        edge_x: f64,
        edge_y: f64,
        direction_x: f64,
        direction_y: f64,
        buff: f64,
        frame_buff: f64,
    ) -> Result<(), PyErr> {
        self.anchor
            .next_to_edge_in_frame(
                &target.anchor,
                (edge_x, edge_y),
                noon::ManimNextToArgs {
                    direction: (direction_x, direction_y),
                    buff,
                    aligned_edge: (0.0, 0.0),
                    mask: (1.0, 1.0),
                },
                frame_buff,
            )
            .map_err(engine_error)
    }

    #[allow(clippy::too_many_arguments)]
    fn stretch(
        &self,
        factor: f64,
        dimension: u32,
        x: f64,
        y: f64,
        z: f64,
        point: bool,
    ) -> PyResult<()> {
        self.anchor
            .stretch(
                factor,
                dimension.try_into().map_err(engine_error)?,
                pivot_3d(x, y, z, point),
            )
            .map_err(engine_error)
    }

    #[pyo3(name = "rescaleToFit")]
    #[allow(clippy::too_many_arguments)]
    fn rescale_to_fit(
        &self,
        length: f64,
        dimension: u32,
        stretch: bool,
        x: f64,
        y: f64,
        z: f64,
        point: bool,
    ) -> PyResult<()> {
        self.anchor
            .rescale_to_fit_with_pivot(
                length,
                dimension.try_into().map_err(engine_error)?,
                stretch,
                pivot_3d(x, y, z, point),
            )
            .map_err(engine_error)
    }

    #[pyo3(name = "dimensionLength")]
    fn dimension_length(&self, dimension: u32) -> PyResult<f64> {
        self.anchor
            .dimension_length(dimension.try_into().map_err(engine_error)?)
            .map_err(engine_error)
    }

    #[pyo3(name = "scale3d")]
    #[allow(clippy::too_many_arguments)]
    fn scale_3d(
        &self,
        sx: f64,
        sy: f64,
        sz: f64,
        x: f64,
        y: f64,
        z: f64,
        point: bool,
    ) -> PyResult<()> {
        self.anchor
            .scale_3d(
                noon::SemanticVec3::new(sx, sy, sz),
                pivot_3d(x, y, z, point),
            )
            .map_err(engine_error)
    }

    #[pyo3(name = "contentCriticalPoint3D")]
    fn content_critical_point_3d(&self, x: f64, y: f64, z: f64) -> PyResult<(f64, f64, f64)> {
        self.anchor
            .critical_point_3d(noon::SemanticVec3::new(x, y, z))
            .map(|p| (p.x, p.y, p.z))
            .map_err(engine_error)
    }
    fn free(&self) {}

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
    #[pyo3(name = "criticalX")]
    fn critical_x(&self, x: f64, _y: f64) -> f64 {
        self.center.0
            + if x < 0.0 {
                -self.width * 0.5
            } else if x > 0.0 {
                self.width * 0.5
            } else {
                0.0
            }
    }
    #[pyo3(name = "criticalY")]
    fn critical_y(&self, _x: f64, y: f64) -> f64 {
        self.center.1
            + if y < 0.0 {
                -self.height * 0.5
            } else if y > 0.0 {
                self.height * 0.5
            } else {
                0.0
            }
    }
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

pub(crate) fn pivot_3d(x: f64, y: f64, z: f64, point: bool) -> noon::ManimRotationPivot {
    if point {
        noon::ManimRotationPivot::Point3d(noon::SemanticVec3::new(x, y, z))
    } else {
        noon::ManimRotationPivot::Edge3d(noon::SemanticVec3::new(x, y, z))
    }
}
