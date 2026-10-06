//! Bounded linear ThreeDAxes authoring on the shared semantic family substrate.
//!
//! The axes remain three ordinary NumberLine families and one ordinary family;
//! coordinate conversion is derived from their checked authored shafts. Cairo
//! shading lives on retained path materials, while the original shaft nodes
//! keep NumberLine identity and coordinate metadata.

use super::*;
use noon_core::{
    SemanticCairoPathAppearance, SemanticMutationTransaction, SemanticNodeCreation,
    SemanticObjectState, SemanticPaint, SemanticSpatialCompositionDomain, SemanticSpatialMaterial,
    SemanticStyle, SemanticTransform, SemanticVec3, SemanticWorldTransform3D, VectorPath,
};

/// Supported linear ThreeDAxes request. Ranges are `[min, max, step]`; lengths
/// are world-space units. Defaults match pinned Manim v0.21 at frame height 8.
/// The z normal is fixed to +Z; axis piece count and directional light are
/// bounded, while arbitrary axis configs, TeX compilation, and custom tip
/// shapes remain outside this slice. Label strings/families arrive as retained
/// text.
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
    /// Number of retained pieces used to render each Cairo-shaded axis shaft.
    pub num_axis_pieces: usize,
    /// Captured world-space light direction used by the Cairo axis gradient.
    pub light_source: SemanticVec3,
    /// Disable Cairo path materials while retaining the same axis topology.
    pub shade_in_3d: bool,
    pub ticks: CoordinateTicks,
    pub style: SemanticStyle,
    /// Per-axis values override the shared constructor defaults when present.
    pub axis_overrides: [ManimThreeDAxisOverrides; 3],
}

#[derive(Clone, Debug, Default)]
pub struct ManimThreeDAxisOverrides {
    pub ticks_enabled: Option<bool>,
    pub tick_size: Option<f64>,
    pub exclude_origin_tick: Option<bool>,
    pub tips: Option<bool>,
    pub tip_length: Option<f64>,
    pub color: Option<noon_core::Color>,
    pub stroke_width: Option<f64>,
    pub stroke_opacity: Option<f64>,
    pub opacity: Option<f64>,
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
            num_axis_pieces: 20,
            light_source: SemanticVec3::new(-7.0, -9.0, 10.0),
            shade_in_3d: true,
            ticks: CoordinateTicks {
                exclude_origin: true,
                ..CoordinateTicks::default()
            },
            style: three_d_axis_style(),
            axis_overrides: std::array::from_fn(|_| ManimThreeDAxisOverrides::default()),
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
            group.next_member(ticks).filter(|id| {
                borrowed
                    .node(*id)
                    .is_some_and(|node| node.semantic_object_state().is_some())
            })
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

    /// Place retained text objects or families using the pinned Manim layout
    /// sequence: axis-family edge center, label critical corner, raw direction
    /// buffer, screen fit, then Y/Z label rotation about the family center.
    pub fn create_axis_label_targets(
        &self,
        labels: &[(usize, crate::MobjectTarget<'_>)],
        buff: f64,
        fixed_orientation: bool,
    ) -> Result<MobjectFamily, CoordinateAuthoringError> {
        if labels.is_empty() || !buff.is_finite() || buff < 0.0 {
            return Err(CoordinateAuthoringError::InvalidOptions(
                "invalid ThreeDAxes labels",
            ));
        }
        let store = Rc::clone(self.family.integration_store());
        let mut seen = [false; 3];
        let mut all_nodes = std::collections::HashSet::new();
        let mut prepared = Vec::new();
        let mut family_members = Vec::new();
        for &(index, label) in labels {
            if index > 2 || seen[index] || !Rc::ptr_eq(&store, label.integration_store()) {
                return Err(CoordinateAuthoringError::InvalidOptions(
                    "invalid ThreeDAxes labels",
                ));
            }
            seen[index] = true;
            label.validate()?;
            let layout = match label {
                crate::MobjectTarget::Object(object) => {
                    crate::LayoutAnchor::from(object).layout()?
                }
                crate::MobjectTarget::Family(family) => {
                    crate::LayoutAnchor::from(family).layout()?
                }
            };
            let (direction, edge, rotation_axis, rotation_angle) = match index {
                0 => (
                    SemanticVec3::new(1.0, 1.0, 0.0),
                    SemanticVec3::new(1.0, 1.0, 0.0),
                    SemanticVec3::new(0.0, 0.0, 1.0),
                    0.0,
                ),
                1 => (
                    SemanticVec3::new(1.0, 1.0, 0.0),
                    SemanticVec3::new(1.0, 1.0, 0.0),
                    SemanticVec3::new(0.0, 0.0, 1.0),
                    std::f64::consts::FRAC_PI_2,
                ),
                _ => (
                    SemanticVec3::new(1.0, 0.0, 0.0),
                    SemanticVec3::new(0.0, 0.0, 1.0),
                    SemanticVec3::new(1.0, 0.0, 0.0),
                    std::f64::consts::FRAC_PI_2,
                ),
            };
            let axis_family = self.axis(index)?;
            let (axis_min, axis_max) = crate::world_affine::target_world_bounds(
                &store,
                crate::MobjectTarget::Family(axis_family.family()),
            )?;
            let anchor = critical_world_point(axis_min, axis_max, edge);
            let source_corner = layout.critical_point(-direction.x, -direction.y);
            let mut delta = (
                anchor.x - source_corner.0 + buff * direction.x,
                anchor.y - source_corner.1 + buff * direction.y,
            );
            shift_label_onto_default_frame(&layout, &mut delta);
            let label_center = layout.critical_point(0.0, 0.0);
            let label_center =
                SemanticVec3::new(label_center.0 + delta.0, label_center.1 + delta.1, 0.0);
            let rotation =
                noon_core::SemanticRotation3D::from_axis_angle(rotation_axis, rotation_angle)
                    .ok_or(CoordinateAuthoringError::InvalidOptions(
                        "invalid ThreeDAxes label rotation",
                    ))?;
            let target_node = label.node_id();
            family_members.push(target_node);
            let leaves = match label {
                crate::MobjectTarget::Object(object) => vec![object.node_id()],
                crate::MobjectTarget::Family(family) => store
                    .borrow()
                    .ordered_leaf_nodes(family.node_id())
                    .map_err(AuthoringError::from)?,
            };
            for node in leaves {
                if !all_nodes.insert(node) {
                    return Err(CoordinateAuthoringError::InvalidOptions(
                        "ThreeDAxes labels cannot share text leaves",
                    ));
                }
                let leaf = Mobject::from_node(Rc::clone(&store), node)?;
                let state = leaf.state()?;
                if state.content.text().is_none() {
                    return Err(CoordinateAuthoringError::InvalidOptions(
                        "ThreeDAxes labels require retained text objects",
                    ));
                }
                let planar =
                    state
                        .transform
                        .as_planar()
                        .ok_or(CoordinateAuthoringError::InvalidOptions(
                            "ThreeDAxes label leaves require planar source transforms",
                        ))?;
                if planar.translation.z != 0.0 || planar.scale.z != 1.0 {
                    return Err(CoordinateAuthoringError::InvalidOptions(
                        "ThreeDAxes label families must lie in the authored XY plane",
                    ));
                }
                let old_rotation = noon_core::SemanticRotation3D::from_axis_angle(
                    SemanticVec3::new(0.0, 0.0, 1.0),
                    planar.rotation_z,
                )
                .ok_or(CoordinateAuthoringError::InvalidOptions(
                    "invalid ThreeDAxes label rotation",
                ))?;
                let leaf_rotation = rotation.compose(old_rotation).ok_or(
                    CoordinateAuthoringError::InvalidOptions("invalid ThreeDAxes label rotation"),
                )?;
                let placed_translation = SemanticVec3::new(
                    planar.translation.x + delta.0,
                    planar.translation.y + delta.1,
                    0.0,
                );
                let relative = sub(placed_translation, label_center);
                let translation = add(
                    label_center,
                    rotation.rotate_vector(relative).ok_or(
                        CoordinateAuthoringError::InvalidOptions(
                            "invalid ThreeDAxes label placement",
                        ),
                    )?,
                );
                let leaf_transform =
                    SemanticWorldTransform3D::new(translation, leaf_rotation, planar.scale).ok_or(
                        CoordinateAuthoringError::InvalidOptions(
                            "invalid ThreeDAxes label placement",
                        ),
                    )?;
                prepared.push((node, leaf_transform, target_node));
            }
        }

        let mut transaction = SemanticMutationTransaction::new();
        let family_token = transaction.create_node(SemanticNodeCreation::family());
        for member in family_members {
            transaction.add_member(family_token, member);
        }
        for (node, transform, label_anchor) in prepared {
            transaction.set_object_transform(node, transform.into());
            if fixed_orientation {
                transaction.set_spatial_composition_domain_with_anchor(
                    node,
                    SemanticSpatialCompositionDomain::FixedOrientation,
                    Some(label_anchor),
                );
            } else {
                transaction
                    .set_spatial_composition_domain(node, SemanticSpatialCompositionDomain::World);
            }
        }
        let result = transaction
            .apply(&mut store.borrow_mut())
            .map_err(AuthoringError::from)?;
        MobjectFamily::from_node(
            store,
            result
                .resolve(family_token)
                .expect("published family token"),
        )
        .map_err(Into::into)
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
            publish_prepared_three_d_axes(prepared.axes, prepared.tip_paths, store, publish)
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
            publish_prepared_three_d_axes(prepared.axes, prepared.tip_paths, store, publish)
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
    axes: Vec<PreparedThreeDAxis>,
    tip_paths: Vec<VectorPath>,
}

struct PreparedThreeDAxis {
    states: Vec<SemanticObjectState>,
    pieces: Vec<SemanticObjectState>,
    world: SemanticWorldTransform3D,
    tips: bool,
    style: SemanticStyle,
    shade_in_3d: bool,
    light_source: SemanticVec3,
}

fn prepare_three_d_axes(
    options: &ManimThreeDAxesOptions,
) -> Result<PreparedThreeDAxes, CoordinateAuthoringError> {
    if !(1..=256).contains(&options.num_axis_pieces)
        || !options.light_source.is_finite()
        || options
            .light_source
            .x
            .abs()
            .max(options.light_source.y.abs())
            .max(options.light_source.z.abs())
            == 0.0
    {
        return Err(CoordinateAuthoringError::InvalidOptions(
            "invalid ThreeDAxes Cairo piece count or light direction",
        ));
    }
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
    let mut prepared_axes = Vec::with_capacity(3);
    let mut tip_paths = Vec::with_capacity(3);
    for (index, (range, length, axis, angle)) in axes.into_iter().enumerate() {
        let overrides = &options.axis_overrides[index];
        let tips = overrides.tips.unwrap_or(options.tips);
        let tip_length = overrides.tip_length.unwrap_or(options.tip_length);
        let ticks = CoordinateTicks {
            enabled: overrides.ticks_enabled.unwrap_or(options.ticks.enabled),
            half_length: overrides.tick_size.unwrap_or(options.ticks.half_length),
            exclude_origin: overrides
                .exclude_origin_tick
                .unwrap_or(options.ticks.exclude_origin),
            ..options.ticks
        };
        let mut style = options.style.clone();
        if let Some(color) = overrides.color {
            style.stroke = Some(SemanticPaint::Solid(color));
        }
        if let Some(width) = overrides.stroke_width {
            style.stroke_width = width;
        }
        if let Some(opacity) = overrides.stroke_opacity {
            style.stroke_opacity = opacity;
        }
        if let Some(opacity) = overrides.opacity {
            style.object_opacity = opacity;
        }
        if tips
            && (!tip_length.is_finite()
                || tip_length <= 0.0
                || !matches!(style.stroke.as_ref(), Some(SemanticPaint::Solid(_))))
        {
            return Err(CoordinateAuthoringError::InvalidOptions(
                "invalid ThreeDAxes tip dimensions or tip color",
            ));
        }
        noon_geometry::validate_coordinate_range(range)?;
        if !length.is_finite() || length <= 0.0 || (tips && tip_length >= length) {
            return Err(CoordinateError::InvalidLength.into());
        }
        let frame = NumberLineFrame::centered(range, length, 0.0)?;
        let mut states = if tips {
            super::prepare_line_with_elongated_ticks(frame, ticks, &style, &[], 2.0, true)?
        } else {
            prepare_line(frame, ticks, &style)?
        };
        let shaft = states
            .first_mut()
            .ok_or(CoordinateAuthoringError::InvalidTopology)?;
        // Keep the canonical NumberLine role and geometry for c2p/p2c while
        // drawing its retained Cairo path pieces below.
        shaft.style.stroke_width = 0.0;
        if options.shade_in_3d {
            for tick in states.iter_mut().skip(1) {
                configure_cairo_axis_path(tick, options.light_source)?;
            }
        }
        let start = frame.start();
        let mut end = frame.end();
        // Manim's path stops at the triangular tip base. The hidden NumberLine
        // shaft remains full-length so shared coordinate queries keep their range.
        if tips {
            end[0] -= tip_length;
        }
        let pieces = (0..options.num_axis_pieces)
            .map(|piece| {
                let t0 = piece as f64 / options.num_axis_pieces as f64;
                let t1 = (piece + 1) as f64 / options.num_axis_pieces as f64;
                let mut state = line_state(
                    [
                        start[0] + (end[0] - start[0]) * t0,
                        start[1] + (end[1] - start[1]) * t0,
                    ],
                    [
                        start[0] + (end[0] - start[0]) * t1,
                        start[1] + (end[1] - start[1]) * t1,
                    ],
                    &style,
                )?;
                if options.shade_in_3d {
                    configure_cairo_axis_path(&mut state, options.light_source)?;
                }
                Ok(state)
            })
            .collect::<Result<Vec<_>, CoordinateAuthoringError>>()?;
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
        if tips {
            let end = frame.end();
            tip_paths.push(filled_tip_path(end, (1.0, 0.0), tip_length)?);
        }
        prepared_axes.push(PreparedThreeDAxis {
            states,
            pieces,
            world,
            tips,
            style,
            shade_in_3d: options.shade_in_3d,
            light_source: options.light_source,
        });
    }
    Ok(PreparedThreeDAxes {
        axes: prepared_axes,
        tip_paths,
    })
}

fn publish_prepared_three_d_axes(
    prepared_axes: Vec<PreparedThreeDAxis>,
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
        for axis in prepared_axes {
            let world = axis.world;
            let mut states = axis.states.into_iter().map(|mut state| {
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
                let shaded = state.spatial_material() == SemanticSpatialMaterial::CairoPath;
                let token = transaction.create_node(SemanticNodeCreation::object(state));
                transaction.add_member(ticks, token);
                if shaded {
                    transaction.set_spatial_composition_domain_with_anchor_ref(
                        token,
                        SemanticSpatialCompositionDomain::World,
                        Some(group.into()),
                    );
                } else {
                    transaction.set_spatial_composition_domain(
                        token,
                        SemanticSpatialCompositionDomain::World,
                    );
                }
            }
            if axis.tips {
                let tip_handle = tip_handles
                    .get(tip_index)
                    .copied()
                    .ok_or(AuthoringError::NonFiniteObjectState)?;
                let mut tip =
                    SemanticObjectState::new(noon_core::StoredGeometry::Resource(tip_handle));
                tip.transform = SemanticTransform::from(world);
                tip.style =
                    tip_style(&axis.style).map_err(|_| AuthoringError::NonFiniteObjectState)?;
                if axis.shade_in_3d {
                    configure_cairo_axis_path(&mut tip, axis.light_source)
                        .map_err(|_| AuthoringError::NonFiniteObjectState)?;
                }
                let token = transaction.create_node(SemanticNodeCreation::object(tip));
                transaction.add_member(group, token);
                if axis.shade_in_3d {
                    transaction.set_spatial_composition_domain_with_anchor_ref(
                        token,
                        SemanticSpatialCompositionDomain::World,
                        Some(group.into()),
                    );
                } else {
                    transaction.set_spatial_composition_domain(
                        token,
                        SemanticSpatialCompositionDomain::World,
                    );
                }
                tip_index += 1;
            }
            let piece_family = transaction.create_node(SemanticNodeCreation::family());
            transaction.add_member(group, piece_family);
            for mut piece in axis.pieces {
                piece.transform = SemanticTransform::from(world);
                let shaded = piece.spatial_material() == SemanticSpatialMaterial::CairoPath;
                let token = transaction.create_node(SemanticNodeCreation::object(piece));
                transaction.add_member(piece_family, token);
                if shaded {
                    transaction.set_spatial_composition_domain_with_anchor_ref(
                        token,
                        SemanticSpatialCompositionDomain::World,
                        Some(group.into()),
                    );
                } else {
                    transaction.set_spatial_composition_domain(
                        token,
                        SemanticSpatialCompositionDomain::World,
                    );
                }
            }
            transaction.add_member(root, group);
        }
        let result = publish(store, transaction)?;
        result
            .resolve(root)
            .ok_or(AuthoringError::UnresolvedCreatedNode(root))
    })
}

fn configure_cairo_axis_path(
    state: &mut SemanticObjectState,
    light_source: SemanticVec3,
) -> Result<(), CoordinateAuthoringError> {
    state.set_spatial_material(SemanticSpatialMaterial::CairoPath);
    state
        .set_cairo_path_appearance(SemanticCairoPathAppearance {
            sheen_factor: 0.2,
            gradient_direction: Some(light_source),
        })
        .map_err(|_| CoordinateAuthoringError::InvalidOptions("invalid Cairo axis appearance"))
}

fn tip_style(style: &SemanticStyle) -> Result<SemanticStyle, CoordinateAuthoringError> {
    filled_tip_style(style)
}

fn three_d_axis_style() -> SemanticStyle {
    let mut style = default_axis_style();
    style.stroke_width_mode = StrokeWidthMode::ScreenSpace;
    style.stroke_cap = noon_core::StrokeCap::Butt;
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

fn critical_world_point(
    min: SemanticVec3,
    max: SemanticVec3,
    direction: SemanticVec3,
) -> SemanticVec3 {
    let component = |low: f64, high: f64, direction: f64| {
        if direction < 0.0 {
            low
        } else if direction > 0.0 {
            high
        } else {
            (low + high) * 0.5
        }
    };
    SemanticVec3::new(
        component(min.x, max.x, direction.x),
        component(min.y, max.y, direction.y),
        component(min.z, max.z, direction.z),
    )
}

/// Match Manim's `shift_onto_screen(buff=MED_SMALL_BUFF)` in the default
/// authored frame. This runs before ThreeDAxes rotates its Y/Z label, as it
/// does in the pinned get-axis-label methods.
fn shift_label_onto_default_frame(layout: &crate::FamilyLayout, delta: &mut (f64, f64)) {
    let edge_buff = f64::from(noon_core::MED_SMALL_BUFF);
    let half_width = f64::from(noon_core::DEFAULT_FRAME_WIDTH) * 0.5;
    let half_height = f64::from(noon_core::DEFAULT_FRAME_HEIGHT) * 0.5;
    let top = layout.critical_point(0.0, 1.0).1 + delta.1;
    let top_limit = half_height - edge_buff;
    if top > top_limit {
        delta.1 += top_limit - top;
    }
    let bottom = layout.critical_point(0.0, -1.0).1 + delta.1;
    let bottom_limit = -half_height + edge_buff;
    if bottom < bottom_limit {
        delta.1 += bottom_limit - bottom;
    }
    let left = layout.critical_point(-1.0, 0.0).0 + delta.0;
    let left_limit = -half_width + edge_buff;
    if left < left_limit {
        delta.0 += left_limit - left;
    }
    let right = layout.critical_point(1.0, 0.0).0 + delta.0;
    let right_limit = half_width - edge_buff;
    if right > right_limit {
        delta.0 += right_limit - right;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_vec3_near(actual: SemanticVec3, expected: SemanticVec3) {
        assert!(
            (actual.x - expected.x).abs() < 1e-6,
            "{actual:?} != {expected:?}"
        );
        assert!(
            (actual.y - expected.y).abs() < 1e-6,
            "{actual:?} != {expected:?}"
        );
        assert!(
            (actual.z - expected.z).abs() < 1e-6,
            "{actual:?} != {expected:?}"
        );
    }

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
    fn three_d_axes_cairo_paths_share_one_axis_family_anchor_and_keep_numberline_shaft() {
        let mut scene = Scene::new();
        let options = ManimThreeDAxesOptions::default();
        let axes = scene.three_d_axes(&options).unwrap();
        let store = axes.family().integration_store();
        let axis_groups = three_axis_members(axes.family()).unwrap();
        let borrowed = store.borrow();
        for group_id in axis_groups {
            let group = borrowed.node(group_id).unwrap();
            let shaft = group.first_member().unwrap();
            let ticks = group.next_member(shaft).unwrap();
            let tip = group.next_member(ticks).unwrap();
            let pieces = group.next_member(tip).unwrap();

            let shaft_state = borrowed.semantic_object_state_checked(shaft).unwrap();
            assert!(matches!(
                shaft_state.role(),
                SemanticObjectRole::NumberLine(_)
            ));
            assert_eq!(shaft_state.style.stroke_width, 0.0);
            assert_eq!(
                shaft_state.spatial_material(),
                SemanticSpatialMaterial::Unlit
            );

            let piece_family = borrowed.node(pieces).unwrap();
            let mut piece_count = 0;
            let mut piece = piece_family.first_member();
            while let Some(piece_id) = piece {
                let state = borrowed.semantic_object_state_checked(piece_id).unwrap();
                assert_eq!(state.spatial_material(), SemanticSpatialMaterial::CairoPath);
                assert_eq!(
                    state.spatial_composition_domain(),
                    SemanticSpatialCompositionDomain::World
                );
                assert_eq!(state.spatial_anchor_family(), Some(group_id));
                assert_eq!(
                    state.cairo_path_appearance(),
                    Some(SemanticCairoPathAppearance {
                        sheen_factor: 0.2,
                        gradient_direction: Some(options.light_source),
                    })
                );
                piece_count += 1;
                piece = piece_family.next_member(piece_id);
            }
            assert_eq!(piece_count, options.num_axis_pieces);
            let last_piece = borrowed
                .semantic_object_state_checked(piece_family.last_member().unwrap())
                .unwrap();
            let Some(StoredGeometry::Line { end, .. }) = last_piece.content.geometry() else {
                panic!("axis piece is not a line")
            };
            let shaft = borrowed.semantic_object_state_checked(shaft).unwrap();
            let Some(StoredGeometry::Line {
                end: coordinate_end,
                ..
            }) = shaft.content.geometry()
            else {
                panic!("coordinate shaft is not a line")
            };
            assert!((f64::from(coordinate_end.x - end.x) - options.tip_length).abs() < 1e-5);
            assert_eq!(last_piece.style.stroke_cap, noon_core::StrokeCap::Butt);

            let tick_family = borrowed.node(ticks).unwrap();
            let mut tick = tick_family.first_member();
            while let Some(tick_id) = tick {
                let state = borrowed.semantic_object_state_checked(tick_id).unwrap();
                assert_eq!(state.spatial_material(), SemanticSpatialMaterial::CairoPath);
                assert_eq!(state.spatial_anchor_family(), Some(group_id));
                tick = tick_family.next_member(tick_id);
            }

            let tip_state = borrowed.semantic_object_state_checked(tip).unwrap();
            assert_eq!(
                tip_state.spatial_material(),
                SemanticSpatialMaterial::CairoPath
            );
            assert_eq!(tip_state.spatial_anchor_family(), Some(group_id));
        }
        drop(borrowed);
        for index in 0..3 {
            assert!(axes.tip(index).unwrap().is_some());
        }
    }

    #[test]
    fn three_d_axes_piece_bounds_and_unshaded_profile_are_explicit() {
        let options = ManimThreeDAxesOptions {
            num_axis_pieces: 1,
            tips: false,
            shade_in_3d: false,
            ..ManimThreeDAxesOptions::default()
        };
        let mut scene = Scene::new();
        let axes = scene.three_d_axes(&options).unwrap();
        assert!(axes.tip(0).unwrap().is_none());
        let store = axes.family().integration_store();
        let axis_id = three_axis_members(axes.family()).unwrap()[0];
        let borrowed = store.borrow();
        let axis_group = borrowed.node(axis_id).unwrap();
        let shaft = axis_group.first_member().unwrap();
        let ticks = axis_group.next_member(shaft).unwrap();
        let pieces = axis_group.next_member(ticks).unwrap();
        let pieces = borrowed.node(pieces).unwrap();
        let piece = pieces.first_member().unwrap();
        let state = borrowed.semantic_object_state_checked(piece).unwrap();
        assert_eq!(state.spatial_material(), SemanticSpatialMaterial::Unlit);
        assert_eq!(state.spatial_anchor_family(), None);
    }

    #[test]
    fn three_d_axes_piece_and_light_validation_is_atomic() {
        let invalid = [
            ManimThreeDAxesOptions {
                num_axis_pieces: 0,
                ..ManimThreeDAxesOptions::default()
            },
            ManimThreeDAxesOptions {
                num_axis_pieces: 257,
                ..ManimThreeDAxesOptions::default()
            },
            ManimThreeDAxesOptions {
                light_source: SemanticVec3::ZERO,
                ..ManimThreeDAxesOptions::default()
            },
            ManimThreeDAxesOptions {
                light_source: SemanticVec3::new(f64::NAN, -9.0, 10.0),
                ..ManimThreeDAxesOptions::default()
            },
        ];
        let mut scene = Scene::new();
        let store = Rc::clone(scene.integration_store());
        for options in invalid {
            let before_revision = store.borrow().scene_revision();
            let before_resources = store.borrow().geometry_resources().stats();
            assert!(scene.three_d_axes(&options).is_err());
            assert_eq!(store.borrow().scene_revision(), before_revision);
            assert_eq!(
                store.borrow().geometry_resources().stats(),
                before_resources
            );
        }
    }

    #[test]
    fn three_d_axes_keep_screen_space_width_on_every_shaft_and_tick() {
        let mut scene = Scene::new();
        let axes = scene
            .three_d_axes(&ManimThreeDAxesOptions::default())
            .unwrap();
        for shaft in axes.axis_shafts().unwrap() {
            let state = shaft.state().unwrap();
            assert_eq!(state.style.stroke_width_mode, StrokeWidthMode::ScreenSpace);
            assert_eq!(state.style.stroke_width, 0.0);
        }
        let store = axes.family().integration_store();
        let axis_groups = three_axis_members(axes.family()).unwrap();
        let borrowed = store.borrow();
        for group_id in axis_groups {
            let group = borrowed.node(group_id).unwrap();
            let shaft = group.first_member().unwrap();
            let ticks = group.next_member(shaft).unwrap();
            let tip = group.next_member(ticks).unwrap();
            let pieces = group.next_member(tip).unwrap();
            let pieces = borrowed.node(pieces).unwrap();
            let first_piece = pieces.first_member().unwrap();
            assert_eq!(
                borrowed
                    .semantic_object_state_checked(first_piece)
                    .unwrap()
                    .style
                    .stroke_width_mode,
                StrokeWidthMode::ScreenSpace
            );
        }
    }

    #[test]
    fn per_axis_overrides_keep_shared_frame_and_change_only_selected_members() {
        let mut options = ManimThreeDAxesOptions::default();
        options.axis_overrides[0].tips = Some(false);
        options.axis_overrides[0].ticks_enabled = Some(false);
        options.axis_overrides[0].color = Some(noon_core::Color::RED);
        options.axis_overrides[0].stroke_width = Some(0.04);
        let mut scene = Scene::new();
        let axes = scene.three_d_axes(&options).unwrap();
        assert!(axes.tip(0).unwrap().is_none());
        assert!(axes.tip(1).unwrap().is_some());
        let axis_group = three_axis_members(axes.family()).unwrap()[0];
        let store = axes.family().integration_store();
        let borrowed = store.borrow();
        let group = borrowed.node(axis_group).unwrap();
        let shaft = group.first_member().unwrap();
        let ticks = group.next_member(shaft).unwrap();
        let pieces = group.next_member(ticks).unwrap();
        let pieces = borrowed.node(pieces).unwrap();
        let first_piece = pieces.first_member().unwrap();
        let x_style = borrowed
            .semantic_object_state_checked(first_piece)
            .unwrap()
            .style
            .clone();
        drop(borrowed);
        assert_eq!(x_style.stroke_width, 0.04);
        assert_eq!(
            x_style.stroke,
            Some(SemanticPaint::Solid(noon_core::Color::RED))
        );
        let coordinates = SemanticVec3::new(2.0, -1.0, 1.5);
        let configured_point =
            axes.authored_frame()
                .unwrap()
                .c2p(coordinates.x, coordinates.y, coordinates.z);
        let default_axes = Scene::new()
            .three_d_axes(&ManimThreeDAxesOptions::default())
            .unwrap();
        let default_point =
            default_axes
                .authored_frame()
                .unwrap()
                .c2p(coordinates.x, coordinates.y, coordinates.z);
        assert_eq!(configured_point, default_point);
    }

    #[cfg(all(feature = "native-text", feature = "typst", feature = "bundled-fonts"))]
    #[test]
    fn axis_label_frame_fit_uses_default_frame_not_camera_frame_center() {
        let bounds_at = |frame_center| {
            let mut scene = Scene::new();
            scene
                .camera_3d_profile(
                    noon_core::ManimCamera3DProfile {
                        phi: 0.6,
                        theta: -1.2,
                        gamma: 0.0,
                        focal_distance: 5.0,
                        zoom: 1.0,
                        frame_height: 8.0,
                        frame_center,
                    },
                    0.1,
                    100.0,
                )
                .unwrap();
            let axes = scene
                .three_d_axes(&ManimThreeDAxesOptions::default())
                .unwrap();
            let label = scene.text(crate::Text::new("axis label")).unwrap();
            axes.create_axis_label_targets(
                &[(0, crate::MobjectTarget::Object(&label))],
                0.1,
                false,
            )
            .unwrap();
            crate::world_affine::target_world_bounds(
                scene.integration_store(),
                crate::MobjectTarget::Object(&label),
            )
            .unwrap()
        };

        let origin_center = bounds_at(SemanticVec3::ZERO);
        let offset_center = bounds_at(SemanticVec3::new(2.0, -1.5, 3.0));
        assert_eq!(origin_center, offset_center);
        let frame_right =
            f64::from(noon_core::DEFAULT_FRAME_WIDTH) * 0.5 - f64::from(noon_core::MED_SMALL_BUFF);
        let frame_top =
            f64::from(noon_core::DEFAULT_FRAME_HEIGHT) * 0.5 - f64::from(noon_core::MED_SMALL_BUFF);
        assert!((origin_center.1.x - frame_right).abs() < 1e-5);
        assert!(origin_center.1.y < frame_top);
    }

    #[cfg(all(feature = "native-text", feature = "typst", feature = "bundled-fonts"))]
    #[test]
    fn rotated_multi_leaf_axis_labels_keep_family_center_and_compose_leaf_rotation() {
        let mut scene = Scene::new();
        let mut options = ManimThreeDAxesOptions::new(
            [-1.0, 1.0, 1.0],
            [-1.0, 1.0, 1.0],
            [-1.0, 1.0, 1.0],
            2.0,
            2.0,
            2.0,
        );
        options.tips = false;
        let axes = scene.three_d_axes(&options).unwrap();
        let old_rotation =
            noon_core::SemanticRotation3D::from_axis_angle(SemanticVec3::new(0.0, 0.0, 1.0), 0.35)
                .unwrap();

        let mut label_family = || {
            let left = scene.text(crate::Text::new("m")).unwrap();
            let right = scene.text(crate::Text::new("m")).unwrap();
            let mut transaction = SemanticMutationTransaction::new();
            for (label, x) in [(&left, -0.75), (&right, 0.75)] {
                let scale = label.state().unwrap().transform.scale;
                transaction.set_object_transform(
                    label.node_id(),
                    noon_core::SemanticTransform {
                        translation: SemanticVec3::new(x, 0.0, 0.0),
                        scale,
                        orientation: noon_core::SemanticOrientation::Planar(0.35),
                    },
                );
            }
            transaction
                .apply(&mut scene.integration_store().borrow_mut())
                .unwrap();
            crate::MobjectFamily::create(
                Rc::clone(scene.integration_store()),
                &[(&left).into(), (&right).into()],
            )
            .unwrap()
        };
        let y_family = label_family();
        let z_family = label_family();
        let expected_centers = [(1usize, &y_family), (2usize, &z_family)].map(|(index, family)| {
            let (direction, edge) = if index == 1 {
                ((1.0, 1.0), SemanticVec3::new(1.0, 1.0, 0.0))
            } else {
                ((1.0, 0.0), SemanticVec3::new(0.0, 0.0, 1.0))
            };
            let layout = crate::LayoutAnchor::from(family).layout().unwrap();
            let (axis_min, axis_max) = crate::world_affine::target_world_bounds(
                scene.integration_store(),
                crate::MobjectTarget::Family(axes.axis(index).unwrap().family()),
            )
            .unwrap();
            let anchor = critical_world_point(axis_min, axis_max, edge);
            let corner = layout.critical_point(-direction.0, -direction.1);
            let center = layout.center();
            (
                center.0 + anchor.x - corner.0 + 0.1 * direction.0,
                center.1 + anchor.y - corner.1 + 0.1 * direction.1,
            )
        });

        axes.create_axis_label_targets(
            &[
                (1, crate::MobjectTarget::Family(&y_family)),
                (2, crate::MobjectTarget::Family(&z_family)),
            ],
            0.1,
            false,
        )
        .unwrap();

        for (family, center_xy, expected_rotation_axis) in [
            (
                &y_family,
                expected_centers[0],
                SemanticVec3::new(0.0, 0.0, 1.0),
            ),
            (
                &z_family,
                expected_centers[1],
                SemanticVec3::new(1.0, 0.0, 0.0),
            ),
        ] {
            let leaves = scene
                .integration_store()
                .borrow()
                .ordered_leaf_nodes(family.node_id())
                .unwrap();
            assert_eq!(leaves.len(), 2);
            let rotation = noon_core::SemanticRotation3D::from_axis_angle(
                expected_rotation_axis,
                std::f64::consts::FRAC_PI_2,
            )
            .unwrap();
            let expected_orientation = rotation.compose(old_rotation).unwrap();
            for (leaf, source_x) in leaves.iter().zip([-0.75, 0.75]) {
                let world = {
                    let store = scene.integration_store();
                    let borrowed = store.borrow();
                    borrowed
                        .semantic_object_state_checked(*leaf)
                        .unwrap()
                        .transform
                        .world_transform()
                        .unwrap()
                };
                let relative = rotation
                    .rotate_vector(SemanticVec3::new(source_x, 0.0, 0.0))
                    .unwrap();
                assert_vec3_near(
                    world.translation,
                    SemanticVec3::new(
                        center_xy.0 + relative.x,
                        center_xy.1 + relative.y,
                        relative.z,
                    ),
                );
                assert_eq!(world.rotation, expected_orientation);
            }
            let (min, max) = crate::world_affine::target_world_bounds(
                scene.integration_store(),
                crate::MobjectTarget::Family(family),
            )
            .unwrap();
            assert_vec3_near(
                SemanticVec3::new(
                    (min.x + max.x) * 0.5,
                    (min.y + max.y) * 0.5,
                    (min.z + max.z) * 0.5,
                ),
                SemanticVec3::new(center_xy.0, center_xy.1, 0.0),
            );
        }
    }

    #[cfg(all(feature = "native-text", feature = "typst", feature = "bundled-fonts"))]
    #[test]
    fn axis_label_layout_rejects_foreign_family_without_publication() {
        let mut scene = Scene::new();
        let axes = scene
            .three_d_axes(&ManimThreeDAxesOptions::default())
            .unwrap();
        let mut foreign_scene = Scene::new();
        let a = foreign_scene.text(crate::Text::new("a")).unwrap();
        let b = foreign_scene.text(crate::Text::new("b")).unwrap();
        let family = crate::MobjectFamily::create(
            Rc::clone(foreign_scene.integration_store()),
            &[(&a).into(), (&b).into()],
        )
        .unwrap();
        let revision = scene.integration_store().borrow().scene_revision();
        assert!(axes
            .create_axis_label_targets(&[(1, crate::MobjectTarget::Family(&family))], 0.1, false,)
            .is_err());
        assert_eq!(
            scene.integration_store().borrow().scene_revision(),
            revision
        );
    }

    #[cfg(all(feature = "native-text", feature = "typst", feature = "bundled-fonts"))]
    #[test]
    fn retained_axis_labels_publish_placement_and_composition_together() {
        let mut scene = Scene::new();
        let axes = scene
            .three_d_axes(&ManimThreeDAxesOptions::default())
            .unwrap();
        let x = scene.text(crate::Text::new("x")).unwrap();
        let y = scene.text(crate::Text::new("y")).unwrap();
        let z = scene.text(crate::Text::new("z")).unwrap();
        let y_layout = crate::LayoutAnchor::from(&y).layout().unwrap();
        let y_width = y_layout.critical_point(1.0, 0.0).0 - y_layout.critical_point(-1.0, 0.0).0;
        let y_height = y_layout.critical_point(0.0, 1.0).1 - y_layout.critical_point(0.0, -1.0).1;
        let before = scene.integration_store().borrow().scene_revision();
        let labels = axes
            .create_axis_label_targets(
                &[
                    (0, crate::MobjectTarget::Object(&x)),
                    (1, crate::MobjectTarget::Object(&y)),
                    (2, crate::MobjectTarget::Object(&z)),
                ],
                0.1,
                true,
            )
            .unwrap();
        assert_eq!(
            scene.integration_store().borrow().scene_revision(),
            before.checked_next().unwrap()
        );
        let member_count = scene
            .integration_store()
            .borrow()
            .semantic_family_checked(labels.node_id())
            .unwrap()
            .members_iter()
            .count();
        assert_eq!(member_count, 3);
        // Manim first shifts the unrotated Y label to the screen edge, then
        // rotates it around its center. The resulting upper extent includes
        // half its original width (rather than half its height).
        let (_, y_max) = crate::world_affine::target_world_bounds(
            scene.integration_store(),
            crate::MobjectTarget::Object(&y),
        )
        .unwrap();
        let expected_y_max = f64::from(noon_core::DEFAULT_FRAME_HEIGHT) * 0.5
            - f64::from(noon_core::MED_SMALL_BUFF)
            - y_height * 0.5
            + y_width * 0.5;
        assert!((y_max.y - expected_y_max).abs() < 1e-5);
        for label in [&x, &y, &z] {
            let state = label.state().unwrap();
            assert_eq!(
                state.spatial_composition_domain(),
                SemanticSpatialCompositionDomain::FixedOrientation
            );
            assert_eq!(state.spatial_anchor_family(), Some(label.node_id()));
            assert!(matches!(
                state.transform.orientation,
                noon_core::SemanticOrientation::Spatial(_)
            ));
        }
        assert!(axes
            .create_axis_label_targets(
                &[
                    (0, crate::MobjectTarget::Object(&x)),
                    (0, crate::MobjectTarget::Object(&y)),
                ],
                0.1,
                false,
            )
            .is_err());
        assert_eq!(
            scene.integration_store().borrow().scene_revision(),
            before.checked_next().unwrap()
        );
    }
}
