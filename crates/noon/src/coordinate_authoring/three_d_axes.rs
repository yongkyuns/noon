//! Bounded linear ThreeDAxes authoring on the shared semantic family substrate.
//!
//! This intentionally omits Manim's Cairo axis shading/pieces and labels.
//! The axes remain three ordinary NumberLine families and one ordinary family;
//! coordinate conversion is derived from their checked authored shafts.

use super::*;
use noon_core::{
    SemanticMutationTransaction, SemanticNodeCreation, SemanticObjectState, SemanticPaint,
    SemanticSpatialCompositionDomain, SemanticStyle, SemanticTransform, SemanticVec3,
    SemanticWorldTransform3D, VectorPath,
};

/// Supported linear ThreeDAxes request. Ranges are `[min, max, step]`; lengths
/// are world-space units. Defaults match pinned Manim v0.21 at frame height 8.
/// The z normal is fixed to +Z; custom axis configs, labels, Cairo piece counts,
/// directional shading, and custom tip shapes are outside this native slice.
#[derive(Clone, Debug)]
pub struct ManimThreeDAxesOptions {
    pub x_range: [f64; 3],
    pub y_range: [f64; 3],
    pub z_range: [f64; 3],
    pub x_length: f64,
    pub y_length: f64,
    pub z_length: f64,
    /// Manim's default is true; tips use the pinned filled triangular shape.
    pub tips: bool,
    pub tip_length: f64,
    pub ticks: CoordinateTicks,
    pub style: SemanticStyle,
}

impl Default for ManimThreeDAxesOptions {
    fn default() -> Self {
        Self {
            x_range: [-6.0, 6.0, 1.0],
            y_range: [-5.0, 5.0, 1.0],
            z_range: [-4.0, 4.0, 1.0],
            x_length: 10.5,
            y_length: 10.5,
            z_length: 6.5,
            tips: true,
            tip_length: 0.35,
            ticks: CoordinateTicks {
                exclude_origin: true,
                ..CoordinateTicks::default()
            },
            style: three_d_axis_style(),
        }
    }
}

impl ManimThreeDAxesOptions {
    pub fn new(
        x_range: [f64; 3],
        y_range: [f64; 3],
        z_range: [f64; 3],
        x_length: f64,
        y_length: f64,
        z_length: f64,
    ) -> Self {
        Self {
            x_range,
            y_range,
            z_range,
            x_length,
            y_length,
            z_length,
            ..Self::default()
        }
    }
}

/// Checked three-dimensional affine coordinate frame, in canonical f64 world
/// coordinates. A zero in each range is used as the shared axis intersection,
/// clamped to the range when zero is outside it (matching Manim's origin shift).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ManimThreeDAxesFrame {
    ranges: [[f64; 3]; 3],
    origin: SemanticVec3,
    basis: [SemanticVec3; 3],
    units_per_coordinate: [f64; 3],
}

impl ManimThreeDAxesFrame {
    fn from_axes(
        axes: [&Mobject; 3],
        effective_worlds: Option<[SemanticWorldTransform3D; 3]>,
    ) -> Result<Self, CoordinateAuthoringError> {
        let mut ranges = [[0.0; 3]; 3];
        let mut starts = [SemanticVec3::ZERO; 3];
        let mut ends = [SemanticVec3::ZERO; 3];
        for (index, axis) in axes.into_iter().enumerate() {
            let state = axis.state()?;
            let SemanticObjectRole::NumberLine(role) = state.role() else {
                return Err(CoordinateAuthoringError::InvalidTopology);
            };
            ranges[index] = role.range();
            let Some(StoredGeometry::Line { start, end }) = state.content.geometry() else {
                return Err(CoordinateAuthoringError::InvalidTopology);
            };
            let world = if let Some(worlds) = effective_worlds {
                worlds[index]
            } else {
                state.transform.world_transform().ok_or(
                    CoordinateAuthoringError::InvalidOptions("invalid ThreeDAxes transform"),
                )?
            };
            starts[index] = world
                .transform_point(SemanticVec3::new(start.x as f64, start.y as f64, 0.0))
                .ok_or(CoordinateAuthoringError::InvalidOptions(
                    "invalid ThreeDAxes shaft",
                ))?;
            ends[index] = world
                .transform_point(SemanticVec3::new(end.x as f64, end.y as f64, 0.0))
                .ok_or(CoordinateAuthoringError::InvalidOptions(
                    "invalid ThreeDAxes shaft",
                ))?;
        }
        let mut basis = [SemanticVec3::ZERO; 3];
        let mut units_per_coordinate = [0.0; 3];
        let mut anchors = [SemanticVec3::ZERO; 3];
        for i in 0..3 {
            let delta = sub(ends[i], starts[i]);
            let length = norm(delta);
            let span = ranges[i][1] - ranges[i][0];
            if !length.is_finite() || length <= 0.0 || !span.is_finite() || span <= 0.0 {
                return Err(CoordinateAuthoringError::InvalidTopology);
            }
            basis[i] = scale(delta, 1.0 / length);
            units_per_coordinate[i] = length / span;
            let zero = ranges[i][0].max(0.0).min(ranges[i][1]);
            anchors[i] = add(starts[i], scale(delta, (zero - ranges[i][0]) / span));
        }
        let tolerance = 1e-7;
        if dot(basis[0], basis[1]).abs() > tolerance
            || dot(basis[0], basis[2]).abs() > tolerance
            || dot(basis[1], basis[2]).abs() > tolerance
            || norm(sub(anchors[0], anchors[1])) > tolerance
            || norm(sub(anchors[0], anchors[2])) > tolerance
        {
            return Err(CoordinateAuthoringError::InvalidTopology);
        }
        Ok(Self {
            ranges,
            origin: anchors[0],
            basis,
            units_per_coordinate,
        })
    }

    pub fn coords_to_point(&self, coordinates: SemanticVec3) -> Option<SemanticVec3> {
        if !coordinates.is_finite() {
            return None;
        }
        let values = [coordinates.x, coordinates.y, coordinates.z];
        let point = (0..3).fold(self.origin, |point, i| {
            add(
                point,
                scale(
                    self.basis[i],
                    (values[i] - clamp_zero(self.ranges[i])) * self.units_per_coordinate[i],
                ),
            )
        });
        point.is_finite().then_some(point)
    }

    /// Manim-style alias for [`Self::coords_to_point`].
    pub fn c2p(&self, x: f64, y: f64, z: f64) -> Option<SemanticVec3> {
        self.coords_to_point(SemanticVec3::new(x, y, z))
    }

    pub fn point_to_coords(&self, point: SemanticVec3) -> Option<SemanticVec3> {
        if !point.is_finite() {
            return None;
        }
        let delta = sub(point, self.origin);
        let coordinates = SemanticVec3::new(
            clamp_zero(self.ranges[0]) + dot(delta, self.basis[0]) / self.units_per_coordinate[0],
            clamp_zero(self.ranges[1]) + dot(delta, self.basis[1]) / self.units_per_coordinate[1],
            clamp_zero(self.ranges[2]) + dot(delta, self.basis[2]) / self.units_per_coordinate[2],
        );
        coordinates.is_finite().then_some(coordinates)
    }

    /// Manim-style alias for [`Self::point_to_coords`].
    pub fn p2c(&self, point: SemanticVec3) -> Option<SemanticVec3> {
        self.point_to_coords(point)
    }
}

#[derive(Clone, Debug)]
pub struct ManimThreeDAxes {
    family: MobjectFamily,
}

impl ManimThreeDAxes {
    pub fn create(
        store: Rc<RefCell<SemanticStore>>,
        options: &ManimThreeDAxesOptions,
    ) -> Result<Self, CoordinateAuthoringError> {
        let prepared = prepare_three_d_axes(options)?;
        let root = publish_prepared_three_d_axes(
            options,
            prepared.axes,
            prepared.tip_paths,
            &mut store.borrow_mut(),
            |store, transaction| transaction.apply(store).map_err(AuthoringError::from),
        )?;
        Self::from_family(MobjectFamily::from_node(store, root)?)
    }

    pub fn from_family(family: MobjectFamily) -> Result<Self, CoordinateAuthoringError> {
        let axes = Self { family };
        let children = three_axis_members(axes.family())?;
        let nodes = axes.family.integration_store();
        let store = nodes.borrow();
        let shaft = |node| {
            store
                .node(node)
                .and_then(|family| family.first_member())
                .ok_or(CoordinateAuthoringError::InvalidTopology)
        };
        let shafts = [
            shaft(children[0])?,
            shaft(children[1])?,
            shaft(children[2])?,
        ];
        drop(store);
        let handles = [
            Mobject::from_node(Rc::clone(nodes), shafts[0])?,
            Mobject::from_node(Rc::clone(nodes), shafts[1])?,
            Mobject::from_node(Rc::clone(nodes), shafts[2])?,
        ];
        ManimThreeDAxesFrame::from_axes([&handles[0], &handles[1], &handles[2]], None)?;
        Ok(axes)
    }

    pub fn family(&self) -> &MobjectFamily {
        &self.family
    }

    pub fn axis(&self, index: usize) -> Result<ManimNumberLine, CoordinateAuthoringError> {
        if index > 2 {
            return Err(CoordinateAuthoringError::InvalidTopology);
        }
        let id = three_axis_members(&self.family)?[index];
        ManimNumberLine::from_family(MobjectFamily::from_node(
            Rc::clone(self.family.integration_store()),
            id,
        )?)
    }

    pub fn tip(&self, index: usize) -> Result<Option<Mobject>, CoordinateAuthoringError> {
        if index > 2 {
            return Err(CoordinateAuthoringError::InvalidTopology);
        }
        let store = Rc::clone(self.family.integration_store());
        let axis_id = three_axis_members(&self.family)?[index];
        let tip_id = {
            let borrowed = store.borrow();
            let group = borrowed
                .node(axis_id)
                .ok_or(CoordinateAuthoringError::InvalidTopology)?;
            let shaft = group
                .first_member()
                .ok_or(CoordinateAuthoringError::InvalidTopology)?;
            let ticks = group
                .next_member(shaft)
                .ok_or(CoordinateAuthoringError::InvalidTopology)?;
            group.next_member(ticks)
        };
        tip_id
            .map(|id| Mobject::from_node(store, id).map_err(Into::into))
            .transpose()
    }

    pub fn authored_frame(&self) -> Result<ManimThreeDAxesFrame, CoordinateAuthoringError> {
        let ids = three_axis_members(&self.family)?;
        let store = Rc::clone(self.family.integration_store());
        let shafts = {
            let borrowed = store.borrow();
            let shaft = |node| {
                borrowed
                    .node(node)
                    .and_then(|family| family.first_member())
                    .ok_or(CoordinateAuthoringError::InvalidTopology)
            };
            [shaft(ids[0])?, shaft(ids[1])?, shaft(ids[2])?]
        };
        let x = Mobject::from_node(Rc::clone(&store), shafts[0])?;
        let y = Mobject::from_node(Rc::clone(&store), shafts[1])?;
        let z = Mobject::from_node(store, shafts[2])?;
        ManimThreeDAxesFrame::from_axes([&x, &y, &z], None)
    }

    pub fn axis_shafts(&self) -> Result<[Mobject; 3], CoordinateAuthoringError> {
        let ids = three_axis_members(&self.family)?;
        let store = Rc::clone(self.family.integration_store());
        let shafts = {
            let borrowed = store.borrow();
            let shaft = |node| {
                borrowed
                    .node(node)
                    .and_then(|family| family.first_member())
                    .ok_or(CoordinateAuthoringError::InvalidTopology)
            };
            [shaft(ids[0])?, shaft(ids[1])?, shaft(ids[2])?]
        };
        Ok([
            Mobject::from_node(Rc::clone(&store), shafts[0])?,
            Mobject::from_node(Rc::clone(&store), shafts[1])?,
            Mobject::from_node(store, shafts[2])?,
        ])
    }

    pub fn frame_with_world_transforms(
        &self,
        worlds: [SemanticWorldTransform3D; 3],
    ) -> Result<ManimThreeDAxesFrame, CoordinateAuthoringError> {
        let shafts = self.axis_shafts()?;
        ManimThreeDAxesFrame::from_axes([&shafts[0], &shafts[1], &shafts[2]], Some(worlds))
    }
}

fn three_axis_members(
    family: &MobjectFamily,
) -> Result<[noon_core::SemanticNodeId; 3], CoordinateAuthoringError> {
    family.validate()?;
    let store = family.integration_store().borrow();
    let root = store
        .node(family.node_id())
        .ok_or(CoordinateAuthoringError::InvalidTopology)?;
    let first = root
        .first_member()
        .ok_or(CoordinateAuthoringError::InvalidTopology)?;
    let second = root
        .next_member(first)
        .ok_or(CoordinateAuthoringError::InvalidTopology)?;
    let third = root
        .next_member(second)
        .ok_or(CoordinateAuthoringError::InvalidTopology)?;
    // Later-added axis labels or other ordinary family members are preserved;
    // the first three children remain the x/y/z coordinate families.
    Ok([first, second, third])
}

impl Scene {
    pub fn three_d_axes(
        &mut self,
        options: &ManimThreeDAxesOptions,
    ) -> Result<ManimThreeDAxes, CoordinateAuthoringError> {
        let prepared = prepare_three_d_axes(options)?;
        let root = self.with_semantic_publication(|store, publish| {
            publish_prepared_three_d_axes(
                options,
                prepared.axes,
                prepared.tip_paths,
                store,
                publish,
            )
        })?;
        let family = MobjectFamily::from_node(Rc::clone(self.integration_store()), root)?;
        ManimThreeDAxes::from_family(family)
    }

    pub fn effective_three_d_axes_frame(
        &self,
        axes: &ManimThreeDAxes,
    ) -> Result<ManimThreeDAxesFrame, CoordinateAuthoringError> {
        if !Rc::ptr_eq(self.integration_store(), axes.family().integration_store()) {
            return Err(AuthoringError::ForeignStore.into());
        }
        let shafts = axes.axis_shafts()?;
        let worlds = [
            self.effective_world_transform(&shafts[0])?,
            self.effective_world_transform(&shafts[1])?,
            self.effective_world_transform(&shafts[2])?,
        ];
        axes.frame_with_world_transforms(worlds)
    }
}

impl crate::LiveSession<'_> {
    pub fn three_d_axes(
        &mut self,
        options: &ManimThreeDAxesOptions,
    ) -> Result<ManimThreeDAxes, CoordinateAuthoringError> {
        let prepared = prepare_three_d_axes(options)?;
        let root = self.with_semantic_publication(|store, publish| {
            publish_prepared_three_d_axes(
                options,
                prepared.axes,
                prepared.tip_paths,
                store,
                publish,
            )
        })?;
        let family = MobjectFamily::from_node(Rc::clone(self.integration_store()), root)?;
        ManimThreeDAxes::from_family(family)
    }

    pub fn effective_three_d_axes_frame(
        &self,
        axes: &ManimThreeDAxes,
    ) -> Result<ManimThreeDAxesFrame, CoordinateAuthoringError> {
        self.require_family(axes.family())?;
        let shafts = axes.axis_shafts()?;
        let worlds = [
            self.effective_world_transform(&shafts[0])?,
            self.effective_world_transform(&shafts[1])?,
            self.effective_world_transform(&shafts[2])?,
        ];
        axes.frame_with_world_transforms(worlds)
    }
}

struct PreparedThreeDAxes {
    axes: Vec<(Vec<SemanticObjectState>, SemanticWorldTransform3D)>,
    tip_paths: Vec<VectorPath>,
}

fn prepare_three_d_axes(
    options: &ManimThreeDAxesOptions,
) -> Result<PreparedThreeDAxes, CoordinateAuthoringError> {
    let axes = [
        (
            options.x_range,
            options.x_length,
            SemanticVec3::new(0.0, 0.0, 1.0),
            0.0,
        ),
        (
            options.y_range,
            options.y_length,
            SemanticVec3::new(0.0, 0.0, 1.0),
            std::f64::consts::FRAC_PI_2,
        ),
        (
            options.z_range,
            options.z_length,
            SemanticVec3::new(0.0, 1.0, 0.0),
            -std::f64::consts::FRAC_PI_2,
        ),
    ];
    if options.tips
        && (!options.tip_length.is_finite()
            || options.tip_length <= 0.0
            || !matches!(options.style.stroke.as_ref(), Some(SemanticPaint::Solid(_))))
    {
        return Err(CoordinateAuthoringError::InvalidOptions(
            "invalid ThreeDAxes tip dimensions or tip color",
        ));
    }
    if options.style.stroke_width_mode != StrokeWidthMode::ScaleWithObject {
        return Err(CoordinateAuthoringError::InvalidOptions(
            "ThreeDAxes World paths require ScaleWithObject stroke width; ScreenSpace expansion is unsupported",
        ));
    }
    let mut prepared_axes = Vec::with_capacity(3);
    let mut tip_paths = Vec::with_capacity(3);
    for (range, length, axis, angle) in axes {
        noon_geometry::validate_coordinate_range(range)?;
        if !length.is_finite() || length <= 0.0 {
            return Err(CoordinateError::InvalidLength.into());
        }
        let frame = NumberLineFrame::centered(range, length, 0.0)?;
        let states = if options.tips {
            super::prepare_line_with_elongated_ticks(
                frame,
                options.ticks,
                &options.style,
                &[],
                2.0,
                true,
            )?
        } else {
            prepare_line(frame, options.ticks, &options.style)?
        };
        let axis_rotation = noon_core::SemanticRotation3D::from_axis_angle(axis, angle).ok_or(
            CoordinateAuthoringError::InvalidOptions("invalid axis orientation"),
        )?;
        let zero_coordinate = clamp_zero(range);
        let zero_distance =
            ((zero_coordinate - range[0]) / (range[1] - range[0])) * length - length * 0.5;
        let origin_offset = axis_rotation
            .rotate_vector(SemanticVec3::new(zero_distance, 0.0, 0.0))
            .ok_or(CoordinateAuthoringError::InvalidOptions(
                "invalid axis origin",
            ))?;
        let world = SemanticWorldTransform3D::new(
            scale(origin_offset, -1.0),
            axis_rotation,
            SemanticVec3::new(1.0, 1.0, 1.0),
        )
        .ok_or(CoordinateAuthoringError::InvalidOptions(
            "invalid axis transform",
        ))?;
        if options.tips {
            let end = frame.end();
            tip_paths.push(triangle_tip_path(
                (end[0], end[1]),
                (1.0, 0.0),
                options.tip_length,
            )?);
        }
        prepared_axes.push((states, world));
    }
    Ok(PreparedThreeDAxes {
        axes: prepared_axes,
        tip_paths,
    })
}

fn publish_prepared_three_d_axes(
    options: &ManimThreeDAxesOptions,
    prepared_axes: Vec<(Vec<SemanticObjectState>, SemanticWorldTransform3D)>,
    tip_paths: Vec<VectorPath>,
    store: &mut noon_core::SemanticStore,
    mut publish: impl FnMut(
        &mut noon_core::SemanticStore,
        SemanticMutationTransaction,
    ) -> Result<noon_core::SemanticMutationTransactionResult, AuthoringError>,
) -> Result<noon_core::SemanticNodeId, AuthoringError> {
    store.with_geometry_paths(tip_paths, |store, tip_handles| {
        let mut transaction = SemanticMutationTransaction::new();
        let root = transaction.create_node(SemanticNodeCreation::family());
        let mut tip_index = 0;
        for (states, world) in prepared_axes {
            let mut states = states.into_iter().map(|mut state| {
                state.transform = SemanticTransform::from(world);
                state
            });
            let group = transaction.create_node(SemanticNodeCreation::family());
            let shaft = transaction.create_node(SemanticNodeCreation::object(
                states.next().ok_or(AuthoringError::NonFiniteObjectState)?,
            ));
            let ticks = transaction.create_node(SemanticNodeCreation::family());
            transaction.add_member(group, shaft);
            transaction.add_member(group, ticks);
            transaction
                .set_spatial_composition_domain(shaft, SemanticSpatialCompositionDomain::World);
            for state in states {
                let token = transaction.create_node(SemanticNodeCreation::object(state));
                transaction.add_member(ticks, token);
                transaction
                    .set_spatial_composition_domain(token, SemanticSpatialCompositionDomain::World);
            }
            if options.tips {
                let tip_handle = tip_handles
                    .get(tip_index)
                    .copied()
                    .ok_or(AuthoringError::NonFiniteObjectState)?;
                let mut tip =
                    SemanticObjectState::new(noon_core::StoredGeometry::Resource(tip_handle));
                tip.transform = SemanticTransform::from(world);
                tip.style =
                    tip_style(&options.style).map_err(|_| AuthoringError::NonFiniteObjectState)?;
                let token = transaction.create_node(SemanticNodeCreation::object(tip));
                transaction.add_member(group, token);
                transaction
                    .set_spatial_composition_domain(token, SemanticSpatialCompositionDomain::World);
                tip_index += 1;
            }
            transaction.add_member(root, group);
        }
        let result = publish(store, transaction)?;
        result
            .resolve(root)
            .ok_or(AuthoringError::UnresolvedCreatedNode(root))
    })
}

fn triangle_tip_path(
    apex: (f64, f64),
    direction: (f64, f64),
    length: f64,
) -> Result<VectorPath, AuthoringError> {
    let vertices = noon_geometry::arrow_tip_vertices(apex, direction, length);
    let point = |xy: (f64, f64)| -> Result<noon_core::Vec2, AuthoringError> {
        Ok(noon_core::Vec2::new(
            crate::integration::authoring_render_f64("ThreeDAxes tip x", xy.0)? as f32,
            crate::integration::authoring_render_f64("ThreeDAxes tip y", xy.1)? as f32,
        ))
    };
    Ok(VectorPath::new()
        .move_to(point(vertices[0])?)
        .line_to(point(vertices[1])?)
        .line_to(point(vertices[2])?)
        .close())
}

fn tip_style(style: &SemanticStyle) -> Result<SemanticStyle, CoordinateAuthoringError> {
    let Some(SemanticPaint::Solid(color)) = style.stroke.as_ref() else {
        return Err(CoordinateAuthoringError::InvalidOptions(
            "ThreeDAxes tips require a solid stroke color",
        ));
    };
    let mut tip = style.clone();
    tip.fill = Some(SemanticPaint::Solid(*color));
    tip.fill_opacity = 1.0;
    tip.stroke = Some(SemanticPaint::Solid(*color));
    Ok(tip)
}

fn three_d_axis_style() -> SemanticStyle {
    let mut style = default_axis_style();
    style.stroke_width_mode = StrokeWidthMode::ScaleWithObject;
    style
}

fn add(a: SemanticVec3, b: SemanticVec3) -> SemanticVec3 {
    SemanticVec3::new(a.x + b.x, a.y + b.y, a.z + b.z)
}
fn sub(a: SemanticVec3, b: SemanticVec3) -> SemanticVec3 {
    SemanticVec3::new(a.x - b.x, a.y - b.y, a.z - b.z)
}
fn scale(a: SemanticVec3, s: f64) -> SemanticVec3 {
    SemanticVec3::new(a.x * s, a.y * s, a.z * s)
}
fn norm(a: SemanticVec3) -> f64 {
    dot(a, a).sqrt()
}
fn dot(a: SemanticVec3, b: SemanticVec3) -> f64 {
    a.x * b.x + a.y * b.y + a.z * b.z
}
fn clamp_zero(range: [f64; 3]) -> f64 {
    0.0f64.max(range[0]).min(range[1])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asymmetric_three_d_axes_round_trip_in_world_coordinates() {
        let mut scene = Scene::new();
        let axes = scene
            .three_d_axes(&ManimThreeDAxesOptions::new(
                [-2.0, 6.0, 2.0],
                [-3.0, 5.0, 2.0],
                [-4.0, 4.0, 2.0],
                8.0,
                4.0,
                6.0,
            ))
            .unwrap();
        let frame = axes.authored_frame().unwrap();
        let coordinates = SemanticVec3::new(1.25, -2.5, 3.0);
        let point = frame.coords_to_point(coordinates).unwrap();
        let zero = frame.coords_to_point(SemanticVec3::ZERO).unwrap();
        assert!(norm(zero) < 1e-7);
        let recovered = frame.point_to_coords(point).unwrap();
        assert!((recovered.x - coordinates.x).abs() < 1e-12);
        assert!((recovered.y - coordinates.y).abs() < 1e-12);
        assert!((recovered.z - coordinates.z).abs() < 1e-12);
        assert!(frame
            .coords_to_point(SemanticVec3::new(f64::NAN, 0.0, 0.0))
            .is_none());
    }

    #[test]
    fn default_construction_keeps_three_filled_tips_in_the_axis_families() {
        let mut scene = Scene::new();
        let axes = scene
            .three_d_axes(&ManimThreeDAxesOptions::default())
            .unwrap();
        let store_rc = Rc::clone(axes.family().integration_store());
        let groups = three_axis_members(axes.family()).unwrap();
        let store = store_rc.borrow();
        for group_id in groups {
            let group = store.node(group_id).unwrap();
            let ticks = group.next_member(group.first_member().unwrap()).unwrap();
            let tip_id = group
                .next_member(ticks)
                .expect("default axes include a tip");
            let tip = store.semantic_object_state_checked(tip_id).unwrap();
            assert!(matches!(
                tip.content.geometry(),
                Some(StoredGeometry::Resource(_))
            ));
            assert_eq!(
                tip.spatial_composition_domain(),
                SemanticSpatialCompositionDomain::World
            );
        }
    }

    #[test]
    fn three_d_axes_validation_fails_before_publishing_any_family() {
        let mut scene = Scene::new();
        let before = scene.integration_store().borrow().scene_revision();
        let options = ManimThreeDAxesOptions {
            z_length: f64::INFINITY,
            ..ManimThreeDAxesOptions::default()
        };
        assert!(scene.three_d_axes(&options).is_err());
        assert_eq!(scene.integration_store().borrow().scene_revision(), before);
    }

    #[test]
    fn three_d_axes_reject_unimplemented_screen_space_strokes_atomically() {
        let mut scene = Scene::new();
        let before = scene.integration_store().borrow().scene_revision();
        let mut options = ManimThreeDAxesOptions::default();
        options.style.stroke_width_mode = StrokeWidthMode::ScreenSpace;
        assert!(scene.three_d_axes(&options).is_err());
        assert_eq!(scene.integration_store().borrow().scene_revision(), before);
    }
}
