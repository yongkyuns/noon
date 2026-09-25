//! Effective retained-Graph endpoint propagation.
//!
//! Graph semantics lower to ordinary compiled object rows plus sparse dependency
//! metadata. This module derives only effective render geometry/style; it never
//! mutates Semantic Scene geometry or introduces a renderer Graph primitive.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use noon_compile::{CompiledGraphEdgeDependency, CompiledGraphEdgeKind};
use noon_core::{GeometryRef, PathCommand, Transform2D, Vec2};

use crate::{frame::FrameRowState, SceneInstance};

impl SceneInstance {
    pub(crate) fn apply_graph_dependency_patch(
        &mut self,
        patch: &noon_compile::ExecutionPatch,
    ) -> Result<(), noon_compile::CompilePatchError> {
        let noon_compile::ExecutionPatch::SetGraphDependencies { owner, .. } = patch else {
            unreachable!("graph dependency helper accepts only graph patches");
        };
        self.compiled.apply_execution_patch(patch)?;
        let dependencies = self.compiled.graph_dependencies_for_owner(*owner).to_vec();
        for dependency_index in dependencies {
            let dependency = self
                .compiled
                .graph_edge_dependency(dependency_index)
                .expect("owner index names one live graph dependency");
            let line = dependency.line_index() as usize;
            self.frame.objects[line].content = self.compiled.objects()[line].content.clone();
            self.frame.objects[line].text_bounds = self.compiled.objects()[line].text_bounds;
            if let noon_compile::CompiledGraphEdgeKind::Arrow {
                end_tip_index,
                start_tip_index,
                ..
            } = dependency.kind()
            {
                for tip in std::iter::once(end_tip_index).chain(start_tip_index) {
                    let tip = tip as usize;
                    self.frame.objects[tip].content = self.compiled.objects()[tip].content.clone();
                    self.frame.objects[tip].text_bounds = self.compiled.objects()[tip].text_bounds;
                }
            }
            self.graph_dependency_visits = self.graph_dependency_visits.saturating_add(1);
            for changed_row in apply_graph_dependency(&self.compiled, &mut self.frame, dependency)
                .into_iter()
                .flatten()
            {
                self.changes.insert(changed_row);
                self.spatial_changes.insert(changed_row);
            }
        }
        self.last_stats = crate::EvaluationStats::default();
        self.last_patch_stats = crate::RuntimePatchStats::default();
        Ok(())
    }

    /// Re-derive dependencies owned by one changed effective row.
    ///
    /// Vertex rows visit only their incident dependencies. Designated edge/tip
    /// rows normally map to one dependency so generic effective edits cannot
    /// temporarily escape retained Graph endpoint semantics.
    pub(crate) fn refresh_graph_dependencies_for_changed_row(&mut self, object_index: usize) {
        let Ok(object_index) = u32::try_from(object_index) else {
            return;
        };
        let Self {
            compiled,
            frame,
            changes,
            spatial_changes,
            graph_dependency_visits,
            ..
        } = self;
        for &dependency_index in compiled.graph_dependencies_for_changed_row(object_index) {
            let Some(dependency) = compiled.graph_edge_dependency(dependency_index) else {
                debug_assert!(false, "compiled Graph dependency index remains in range");
                continue;
            };
            *graph_dependency_visits = (*graph_dependency_visits).saturating_add(1);
            for changed_row in apply_graph_dependency(compiled, frame, dependency)
                .into_iter()
                .flatten()
            {
                changes.insert(changed_row);
                spatial_changes.insert(changed_row);
            }
        }
    }

    /// Re-derive the complete reachable graph dependency set after a full seek or
    /// bootstrap. Incremental frame paths use the row-local hook above instead.
    pub(crate) fn refresh_all_graph_dependencies(&mut self) {
        let Self {
            compiled,
            frame,
            changes,
            spatial_changes,
            graph_dependency_visits,
            ..
        } = self;
        for dependency in compiled
            .graph_edge_dependencies()
            .iter()
            .copied()
            .filter(|dependency| dependency.is_live())
        {
            *graph_dependency_visits = (*graph_dependency_visits).saturating_add(1);
            for changed_row in apply_graph_dependency(compiled, frame, dependency)
                .into_iter()
                .flatten()
            {
                changes.insert(changed_row);
                spatial_changes.insert(changed_row);
            }
        }
    }

    /// Extend one speculative prepared phase with exactly the graph dependencies
    /// touched by its staged rows. This preserves callback-phase coherence without
    /// mutating the committed frame or scanning unrelated graph/scene state.
    pub(crate) fn refresh_prepared_graph_dependencies(
        &self,
        rows: &mut BTreeMap<usize, FrameRowState>,
    ) {
        let mut dependencies = BTreeSet::new();
        for &object_index in rows.keys() {
            let Ok(object_index) = u32::try_from(object_index) else {
                continue;
            };
            dependencies.extend(
                self.compiled
                    .graph_dependencies_for_changed_row(object_index)
                    .iter()
                    .copied(),
            );
        }

        for dependency_index in dependencies {
            let Some(dependency) = self.compiled.graph_edge_dependency(dependency_index) else {
                debug_assert!(false, "compiled Graph dependency index remains in range");
                continue;
            };
            apply_prepared_graph_dependency(&self.compiled, &self.frame, rows, dependency);
        }
    }

    #[cfg(test)]
    pub(crate) const fn graph_dependency_visits(&self) -> u64 {
        self.graph_dependency_visits
    }
}

fn apply_prepared_graph_dependency(
    compiled: &noon_compile::CompiledScene,
    frame: &crate::FrameState,
    rows: &mut BTreeMap<usize, FrameRowState>,
    dependency: CompiledGraphEdgeDependency,
) {
    let start_index = dependency.start_vertex_index() as usize;
    let end_index = dependency.end_vertex_index() as usize;
    let line_index = dependency.line_index() as usize;
    if !compiled.object_slot_is_live(dependency.start_vertex_index())
        || !compiled.object_slot_is_live(dependency.end_vertex_index())
        || !compiled.object_slot_is_live(dependency.line_index())
    {
        return;
    }

    let start = prepared_render_transform(frame, rows, start_index).translation;
    let end = prepared_render_transform(frame, rows, end_index).translation;
    match dependency.kind() {
        CompiledGraphEdgeKind::Line => {
            set_prepared_segment_transform(frame, rows, line_index, start, end);
        }
        CompiledGraphEdgeKind::Arrow {
            end_tip_index,
            start_tip_index,
            policy,
        } => {
            if !compiled.object_slot_is_live(end_tip_index)
                || start_tip_index.is_some_and(|index| !compiled.object_slot_is_live(index))
            {
                return;
            }
            let geometry = arrow_geometry(start, end, start_tip_index.is_some(), policy);
            set_prepared_segment_transform(
                frame,
                rows,
                line_index,
                geometry.shaft_start,
                geometry.shaft_end,
            );
            set_prepared_stroke_width(frame, rows, line_index, geometry.stroke_width);

            set_prepared_tip_transform(
                frame,
                rows,
                end_tip_index as usize,
                geometry.visible_end,
                geometry.direction,
                geometry.tip_length,
            );
            if let Some(start_tip_index) = start_tip_index {
                set_prepared_tip_transform(
                    frame,
                    rows,
                    start_tip_index as usize,
                    geometry.visible_start,
                    Vec2::new(-geometry.direction.x, -geometry.direction.y),
                    geometry.tip_length,
                );
            }
        }
    }
}

fn prepared_render_transform(
    frame: &crate::FrameState,
    rows: &BTreeMap<usize, FrameRowState>,
    object_index: usize,
) -> Transform2D {
    rows.get(&object_index)
        .map(|row| row.render_transform.unwrap_or(row.transform))
        .unwrap_or_else(|| frame.render_transform(object_index))
}

fn set_prepared_segment_transform(
    frame: &crate::FrameState,
    rows: &mut BTreeMap<usize, FrameRowState>,
    object_index: usize,
    start: Vec2,
    end: Vec2,
) {
    let Some(geometry) = frame.objects[object_index].geometry() else {
        return;
    };
    let Some((source_start, source_end)) = segment_axis(geometry) else {
        return;
    };
    let row = rows
        .entry(object_index)
        .or_insert_with(|| FrameRowState::from_frame(frame, object_index));
    row.render_geometry = frame.render_geometries[object_index]
        .clone()
        .or_else(|| Some(Arc::new(geometry.clone())));
    row.render_transform = Some(similarity_transform(*source_start, *source_end, start, end));
}

fn set_prepared_tip_transform(
    frame: &crate::FrameState,
    rows: &mut BTreeMap<usize, FrameRowState>,
    object_index: usize,
    apex: Vec2,
    direction: Vec2,
    length: f32,
) {
    let Some(GeometryRef::VectorPath(path)) = frame.objects[object_index].geometry() else {
        return;
    };
    let Some((source_apex, source_base)) = tip_axis(path) else {
        return;
    };
    let row = rows
        .entry(object_index)
        .or_insert_with(|| FrameRowState::from_frame(frame, object_index));
    row.render_geometry = frame.render_geometries[object_index]
        .clone()
        .or_else(|| Some(Arc::new(GeometryRef::VectorPath(path.clone()))));
    row.render_transform = Some(similarity_transform(
        source_base,
        source_apex,
        apex - direction * length,
        apex,
    ));
}

fn set_prepared_stroke_width(
    frame: &crate::FrameState,
    rows: &mut BTreeMap<usize, FrameRowState>,
    object_index: usize,
    stroke_width: f32,
) {
    let current = rows
        .get(&object_index)
        .map(|row| row.style.stroke_width)
        .unwrap_or(frame.objects[object_index].style.stroke_width);
    if current == stroke_width {
        return;
    }
    rows.entry(object_index)
        .or_insert_with(|| FrameRowState::from_frame(frame, object_index))
        .style
        .stroke_width = stroke_width;
}

fn apply_graph_dependency(
    compiled: &noon_compile::CompiledScene,
    frame: &mut crate::FrameState,
    dependency: CompiledGraphEdgeDependency,
) -> [Option<usize>; 3] {
    let start_index = dependency.start_vertex_index() as usize;
    let end_index = dependency.end_vertex_index() as usize;
    let line_index = dependency.line_index() as usize;
    if !compiled.object_slot_is_live(dependency.start_vertex_index())
        || !compiled.object_slot_is_live(dependency.end_vertex_index())
        || !compiled.object_slot_is_live(dependency.line_index())
    {
        return [None, None, None];
    }

    let start = frame.render_transform(start_index).translation;
    let end = frame.render_transform(end_index).translation;

    match dependency.kind() {
        CompiledGraphEdgeKind::Line => {
            let changed = set_effective_segment_transform(frame, line_index, start, end);
            [changed.then_some(line_index), None, None]
        }
        CompiledGraphEdgeKind::Arrow {
            end_tip_index,
            start_tip_index,
            policy,
        } => {
            if !compiled.object_slot_is_live(end_tip_index)
                || start_tip_index.is_some_and(|index| !compiled.object_slot_is_live(index))
            {
                return [None, None, None];
            }
            let geometry = arrow_geometry(start, end, start_tip_index.is_some(), policy);
            let line_geometry_changed = set_effective_segment_transform(
                frame,
                line_index,
                geometry.shaft_start,
                geometry.shaft_end,
            );
            let stroke_width_changed =
                set_effective_stroke_width(frame, line_index, geometry.stroke_width);
            let line_changed = line_geometry_changed || stroke_width_changed;

            let end_tip_index = end_tip_index as usize;
            let end_tip_changed = set_effective_tip_transform(
                frame,
                end_tip_index,
                geometry.visible_end,
                geometry.direction,
                geometry.tip_length,
            );
            let start_tip_changed = start_tip_index.map(|start_tip_index| {
                let start_tip_index = start_tip_index as usize;
                let changed = set_effective_tip_transform(
                    frame,
                    start_tip_index,
                    geometry.visible_start,
                    Vec2::new(-geometry.direction.x, -geometry.direction.y),
                    geometry.tip_length,
                );
                (start_tip_index, changed)
            });

            [
                line_changed.then_some(line_index),
                end_tip_changed.then_some(end_tip_index),
                start_tip_changed.and_then(|(index, changed)| changed.then_some(index)),
            ]
        }
    }
}

/// Install world-space effective geometry through the existing renderer-neutral
/// override lane. The first evaluation retains the compiler-owned local basis;
/// later evaluations move that same resource through the ordinary render transform
/// lane. Graph dependencies never mint a world-space path per frame.
fn set_effective_segment_transform(
    frame: &mut crate::FrameState,
    object_index: usize,
    target_start: Vec2,
    target_end: Vec2,
) -> bool {
    let Some(geometry) = frame.objects[object_index].geometry().cloned() else {
        return false;
    };
    let Some((start, end)) = segment_axis(&geometry) else {
        return false;
    };
    let transform = similarity_transform(*start, *end, target_start, target_end);
    set_effective_render_transform(frame, object_index, geometry, transform)
}

fn set_effective_tip_transform(
    frame: &mut crate::FrameState,
    object_index: usize,
    apex: Vec2,
    direction: Vec2,
    length: f32,
) -> bool {
    let Some(geometry) = frame.objects[object_index].geometry().cloned() else {
        return false;
    };
    let GeometryRef::VectorPath(path) = &geometry else {
        return false;
    };
    let Some((source_apex, source_base)) = tip_axis(path) else {
        return false;
    };
    let target_base = apex - direction * length;
    set_effective_render_transform(
        frame,
        object_index,
        geometry,
        similarity_transform(source_base, source_apex, target_base, apex),
    )
}

fn set_effective_render_transform(
    frame: &mut crate::FrameState,
    object_index: usize,
    geometry: GeometryRef,
    transform: Transform2D,
) -> bool {
    if frame.render_geometries[object_index].is_some()
        && frame.render_transforms[object_index] == Some(transform)
    {
        return false;
    }
    if frame.render_geometries[object_index].is_none() {
        frame.render_geometries[object_index] = Some(Arc::new(geometry));
    }
    frame.render_transforms[object_index] = Some(transform);
    true
}

fn tip_axis(path: &noon_core::VectorPath) -> Option<(Vec2, Vec2)> {
    let [PathCommand::MoveTo { to: apex }, PathCommand::LineTo { to: first }, PathCommand::LineTo { to: second }, ..] =
        path.commands()
    else {
        return None;
    };
    Some((*apex, (*first + *second) * 0.5))
}

fn segment_axis(geometry: &GeometryRef) -> Option<(&Vec2, &Vec2)> {
    match geometry {
        GeometryRef::Line { start, end } => Some((start, end)),
        GeometryRef::VectorPath(path) => {
            let [PathCommand::MoveTo { to: start }, PathCommand::LineTo { to: end }, ..] =
                path.commands()
            else {
                return None;
            };
            Some((start, end))
        }
        GeometryRef::Circle { .. } | GeometryRef::Rectangle { .. } | GeometryRef::External(_) => {
            None
        }
    }
}

fn similarity_transform(
    source_start: Vec2,
    source_end: Vec2,
    target_start: Vec2,
    target_end: Vec2,
) -> Transform2D {
    let source = source_end - source_start;
    let target = target_end - target_start;
    let source_length = source.length();
    let target_length = target.length();
    let scale = if source_length > 0.0 {
        target_length / source_length
    } else {
        0.0
    };
    let rotation = if source_length > 0.0 && target_length > 0.0 {
        target.y.atan2(target.x) - source.y.atan2(source.x)
    } else {
        0.0
    };
    let translation = target_start - (source_start * scale).rotate(rotation);
    Transform2D {
        translation,
        rotation,
        scale: Vec2::new(scale, scale),
    }
}

fn set_effective_stroke_width(
    frame: &mut crate::FrameState,
    object_index: usize,
    stroke_width: f32,
) -> bool {
    let current = &mut frame.objects[object_index].style.stroke_width;
    if *current == stroke_width {
        return false;
    }
    *current = stroke_width;
    true
}

#[derive(Clone, Copy, Debug)]
struct EffectiveArrowGeometry {
    visible_start: Vec2,
    visible_end: Vec2,
    direction: Vec2,
    tip_length: f32,
    shaft_start: Vec2,
    shaft_end: Vec2,
    stroke_width: f32,
}

fn arrow_geometry(
    start: Vec2,
    end: Vec2,
    start_tip: bool,
    policy: noon_compile::CompiledGraphArrowPolicy,
) -> EffectiveArrowGeometry {
    let geometry = noon_geometry::ArrowGeometry::between(
        (f64::from(start.x), f64::from(start.y)),
        (f64::from(end.x), f64::from(end.y)),
        f64::from(policy.buff()),
        f64::from(policy.tip_length()),
        f64::from(policy.max_tip_length_to_length_ratio()),
        start_tip,
    );
    let point = |(x, y): (f64, f64)| Vec2::new(x as f32, y as f32);
    EffectiveArrowGeometry {
        visible_start: point(geometry.visible_start),
        visible_end: point(geometry.visible_end),
        direction: point(geometry.direction),
        tip_length: geometry.tip_length as f32,
        shaft_start: point(geometry.shaft_start),
        shaft_end: point(geometry.shaft_end),
        stroke_width: geometry.stroke_width(
            f64::from(policy.initial_stroke_width()),
            f64::from(policy.max_stroke_width_to_length_ratio()),
        ) as f32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use noon_compile::{lower_semantic_execution, SemanticExecutionIndex};
    use noon_core::{
        GraphTopology, RateFunction, SemanticArrowShaftRole, SemanticGraphArrowPolicy,
        SemanticMutationTransaction, SemanticNodeCreation, SemanticObjectProperty,
        SemanticObjectRole, SemanticObjectState, SemanticStore,
        SemanticTransactionGraphDeclaration, SemanticTransactionGraphEdgeBinding, SemanticVec3,
        StoredGeometry, VectorPath,
    };

    fn circle_at(x: f64, y: f64) -> SemanticObjectState {
        let mut state = SemanticObjectState::new(StoredGeometry::Circle { radius: 0.2 });
        state.transform.translation = SemanticVec3::new(x, y, 0.0);
        state
    }

    fn rendered_line(instance: &SceneInstance, index: usize) -> (Vec2, Vec2) {
        let geometry = instance.frame().render_geometry(index).unwrap();
        let (start, end) =
            segment_axis(geometry).expect("graph shaft keeps an ordinary segment resource");
        let transform = instance.frame().render_transform(index);
        (
            transform.transform_point(*start),
            transform.transform_point(*end),
        )
    }

    fn triangle_tip_path(apex: Vec2, direction: Vec2, length: f32) -> VectorPath {
        let [apex, first, second] = noon_geometry::arrow_tip_vertices(
            (f64::from(apex.x), f64::from(apex.y)),
            (f64::from(direction.x), f64::from(direction.y)),
            f64::from(length),
        )
        .map(|(x, y)| Vec2::new(x as f32, y as f32));
        VectorPath::new()
            .move_to(apex)
            .line_to(first)
            .line_to(second)
            .close()
    }

    fn rendered_tip(instance: &SceneInstance, index: usize) -> [Vec2; 3] {
        let GeometryRef::VectorPath(path) = instance.frame().render_geometry(index).unwrap() else {
            panic!("graph tip keeps an ordinary compiled path resource");
        };
        let [PathCommand::MoveTo { to: apex }, PathCommand::LineTo { to: first }, PathCommand::LineTo { to: second }, ..] =
            path.commands()
        else {
            panic!("graph tip keeps the shared triangular command order");
        };
        let transform = instance.frame().render_transform(index);
        [*apex, *first, *second].map(|point| transform.transform_point(point))
    }

    fn assert_vec2_close(actual: Vec2, expected: Vec2) {
        assert!(
            (actual.x - expected.x).abs() <= 1.0e-6 && (actual.y - expected.y).abs() <= 1.0e-6,
            "expected {expected:?}, got {actual:?}"
        );
    }

    fn assert_points_close<const N: usize>(actual: [Vec2; N], expected: [Vec2; N]) {
        for (actual, expected) in actual.into_iter().zip(expected) {
            assert_vec2_close(actual, expected);
        }
    }

    struct LineGraphFixture {
        store: SemanticStore,
        a: noon_core::SemanticNodeId,
        b: noon_core::SemanticNodeId,
        line: noon_core::SemanticNodeId,
        position: Option<noon_core::SemanticNodeId>,
    }

    struct ArrowGraphFixture {
        store: SemanticStore,
        start: noon_core::SemanticNodeId,
        shaft: noon_core::SemanticNodeId,
        tip: noon_core::SemanticNodeId,
        position: noon_core::SemanticNodeId,
    }

    fn arrow_graph_fixture(start: Vec2, end: Vec2) -> ArrowGraphFixture {
        let mut store = SemanticStore::new();
        let mut topology = GraphTopology::new();
        let start_id = topology.add_vertex();
        let end_id = topology.add_vertex();
        let edge_id = topology.add_edge(start_id, end_id, true).unwrap();
        let policy = noon_compile::CompiledGraphArrowPolicy::new(0.25, 0.35, 0.25, 0.06, 0.05);
        let geometry = arrow_geometry(start, end, false, policy);

        let mut shaft_state = SemanticObjectState::new(StoredGeometry::Line {
            start: geometry.shaft_start,
            end: geometry.shaft_end,
        });
        shaft_state.style.stroke_width = f64::from(geometry.stroke_width);
        shaft_state.set_role(SemanticObjectRole::ArrowShaft(SemanticArrowShaftRole::new(
            f64::from(policy.initial_stroke_width()),
            f64::from(policy.max_stroke_width_to_length_ratio()),
        )));
        let tip_resource = store
            .insert_geometry_path(triangle_tip_path(
                geometry.visible_end,
                geometry.direction,
                geometry.tip_length,
            ))
            .unwrap();
        let mut tip_state = SemanticObjectState::new(StoredGeometry::Resource(tip_resource));
        tip_state.set_role(SemanticObjectRole::ArrowEndTip);

        let mut tx = SemanticMutationTransaction::new();
        let root = tx.create_node(SemanticNodeCreation::family());
        let family = tx.create_node(SemanticNodeCreation::family());
        let shaft = tx.create_node(SemanticNodeCreation::object(shaft_state));
        let tip = tx.create_node(SemanticNodeCreation::object(tip_state));
        let start_node = tx.create_node(SemanticNodeCreation::object(circle_at(
            f64::from(start.x),
            f64::from(start.y),
        )));
        let end_node = tx.create_node(SemanticNodeCreation::object(circle_at(
            f64::from(end.x),
            f64::from(end.y),
        )));
        tx.add_member(family, shaft)
            .add_member(family, tip)
            .add_member(root, family)
            .add_member(root, start_node)
            .add_member(root, end_node)
            .set_graph_declaration(
                root,
                SemanticTransactionGraphDeclaration::new(
                    topology,
                    [(start_id, start_node), (end_id, end_node)],
                    [SemanticTransactionGraphEdgeBinding::new_arrow(
                        edge_id,
                        family.into(),
                        shaft.into(),
                        tip.into(),
                        None,
                        SemanticGraphArrowPolicy::new(
                            f64::from(policy.buff()),
                            f64::from(policy.tip_length()),
                            f64::from(policy.max_tip_length_to_length_ratio()),
                        ),
                    )],
                ),
            );
        let result = tx.apply(&mut store).unwrap();
        let root = result.resolve(root).unwrap();
        let start_node = result.resolve(start_node).unwrap();
        let shaft = result.resolve(shaft).unwrap();
        let tip = result.resolve(tip).unwrap();
        store.attach_to_scene(root).unwrap();
        let position = store
            .insert_semantic_input_signal(SemanticVec3::new(
                f64::from(start.x),
                f64::from(start.y),
                0.0,
            ))
            .unwrap();
        store
            .bind_semantic_signal(position, start_node, SemanticObjectProperty::Translation)
            .unwrap();

        ArrowGraphFixture {
            store,
            start: start_node,
            shaft,
            tip,
            position,
        }
    }

    fn line_graph_fixture(unrelated: usize, reactive: bool) -> LineGraphFixture {
        let mut store = SemanticStore::new();
        for _ in 0..unrelated {
            let node =
                store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                    radius: 0.1,
                }));
            store.attach_to_scene(node).unwrap();
        }

        let mut topology = GraphTopology::new();
        let a_id = topology.add_vertex();
        let b_id = topology.add_vertex();
        let edge_id = topology.add_edge(a_id, b_id, false).unwrap();

        let mut tx = SemanticMutationTransaction::new();
        let root = tx.create_node(SemanticNodeCreation::family());
        let edge_family = tx.create_node(SemanticNodeCreation::family());
        let line = tx.create_node(SemanticNodeCreation::object(SemanticObjectState::new(
            StoredGeometry::Line {
                start: Vec2::new(-1.0, 0.0),
                end: Vec2::new(1.0, 0.0),
            },
        )));
        let a = tx.create_node(SemanticNodeCreation::object(circle_at(-1.0, 0.0)));
        let b = tx.create_node(SemanticNodeCreation::object(circle_at(1.0, 0.0)));
        tx.add_member(edge_family, line)
            .add_member(root, edge_family)
            .add_member(root, a)
            .add_member(root, b)
            .set_graph_declaration(
                root,
                SemanticTransactionGraphDeclaration::new(
                    topology,
                    [(a_id, a), (b_id, b)],
                    [SemanticTransactionGraphEdgeBinding::new(
                        edge_id,
                        edge_family.into(),
                        line.into(),
                    )],
                ),
            );
        let result = tx.apply(&mut store).unwrap();
        let root = result.resolve(root).unwrap();
        let a = result.resolve(a).unwrap();
        let b = result.resolve(b).unwrap();
        let line = result.resolve(line).unwrap();
        store.attach_to_scene(root).unwrap();
        let position = reactive.then(|| {
            let position = store
                .insert_semantic_input_signal(SemanticVec3::new(-1.0, 0.0, 0.0))
                .unwrap();
            store
                .bind_semantic_signal(position, a, SemanticObjectProperty::Translation)
                .unwrap();
            position
        });

        LineGraphFixture {
            store,
            a,
            b,
            line,
            position,
        }
    }

    #[test]
    fn reactive_vertex_motion_updates_only_vertex_and_incident_line_independent_of_scene_size() {
        let fixture = line_graph_fixture(10_000, true);
        let mut index = SemanticExecutionIndex::new();
        let lowered = lower_semantic_execution(&fixture.store, &mut index).unwrap();
        let a_object = index.execution_object_id(fixture.a).unwrap();
        let b_object = index.execution_object_id(fixture.b).unwrap();
        let line_object = index.execution_object_id(fixture.line).unwrap();
        let position = lowered
            .reactive()
            .execution_signal_id(fixture.position.expect("reactive fixture has an input"))
            .unwrap();
        let revision = fixture.store.scene_revision();
        let mut instance = SceneInstance::from_semantic_execution(lowered);
        instance.take_frame_changes();
        let before_visits = instance.graph_dependency_visits();

        instance
            .set_reactive_input(
                position,
                noon_core::ReactiveValue::Vec2(Vec2::new(-2.0, 1.0)),
            )
            .unwrap();

        assert_eq!(fixture.store.scene_revision(), revision);
        assert_eq!(instance.graph_dependency_visits() - before_visits, 1);
        let changed = instance.take_frame_changes().object_indices().to_vec();
        let a_index = instance.frame_index_for_object(a_object).unwrap();
        let b_index = instance.frame_index_for_object(b_object).unwrap();
        let line_index = instance.frame_index_for_object(line_object).unwrap();
        let mut expected = vec![a_index, line_index];
        expected.sort_unstable();
        assert_eq!(changed, expected);
        assert!(!changed.contains(&b_index));
        assert_eq!(
            rendered_line(&instance, line_index),
            (Vec2::new(-2.0, 1.0), Vec2::new(1.0, 0.0))
        );
    }

    #[test]
    fn graph_arrow_dependency_rebuilds_shaft_tip_and_stroke_cap_from_effective_centers() {
        let fixture = arrow_graph_fixture(Vec2::new(-2.0, 0.0), Vec2::new(2.0, 0.0));

        let mut index = SemanticExecutionIndex::new();
        let lowered = lower_semantic_execution(&fixture.store, &mut index).unwrap();
        let signal = lowered
            .reactive()
            .execution_signal_id(fixture.position)
            .unwrap();
        let shaft_object = index.execution_object_id(fixture.shaft).unwrap();
        let tip_object = index.execution_object_id(fixture.tip).unwrap();
        let mut instance = SceneInstance::from_semantic_execution(lowered);
        instance.take_frame_changes();

        instance
            .set_reactive_input(signal, noon_core::ReactiveValue::Vec2(Vec2::new(-4.0, 0.0)))
            .unwrap();

        let shaft_index = instance.frame_index_for_object(shaft_object).unwrap();
        let tip_index = instance.frame_index_for_object(tip_object).unwrap();
        let (shaft_start, shaft_end) = rendered_line(&instance, shaft_index);
        assert_vec2_close(shaft_start, Vec2::new(-3.75, 0.0));
        assert_vec2_close(shaft_end, Vec2::new(1.4, 0.0));
        assert_points_close(
            rendered_tip(&instance, tip_index),
            [
                Vec2::new(1.75, 0.0),
                Vec2::new(1.4, 0.175),
                Vec2::new(1.4, -0.175),
            ],
        );
        assert!(instance.frame().render_geometries[shaft_index].is_some());
        assert!(instance.frame().render_geometries[tip_index].is_some());
        assert_eq!(
            instance.frame().objects[shaft_index].style.stroke_width,
            0.06
        );
    }

    #[test]
    fn initially_collapsed_arrow_expands_to_short_geometry_without_resource_replacement() {
        let fixture = arrow_graph_fixture(Vec2::ZERO, Vec2::ZERO);
        let revision = fixture.store.scene_revision();
        let mut index = SemanticExecutionIndex::new();
        let lowered = lower_semantic_execution(&fixture.store, &mut index).unwrap();
        let signal = lowered
            .reactive()
            .execution_signal_id(fixture.position)
            .unwrap();
        let start_object = index.execution_object_id(fixture.start).unwrap();
        let shaft_object = index.execution_object_id(fixture.shaft).unwrap();
        let tip_object = index.execution_object_id(fixture.tip).unwrap();
        let mut instance = SceneInstance::from_semantic_execution(lowered);
        instance.take_frame_changes();
        let start_index = instance.frame_index_for_object(start_object).unwrap();
        let shaft_index = instance.frame_index_for_object(shaft_object).unwrap();
        let tip_index = instance.frame_index_for_object(tip_object).unwrap();
        let shaft_resource = instance.frame().render_geometries[shaft_index]
            .as_ref()
            .unwrap()
            .clone();
        let tip_resource = instance.frame().render_geometries[tip_index]
            .as_ref()
            .unwrap()
            .clone();

        instance
            .set_reactive_input(signal, noon_core::ReactiveValue::Vec2(Vec2::new(-0.2, 0.0)))
            .unwrap();

        assert_eq!(fixture.store.scene_revision(), revision);
        let mut expected = vec![start_index, shaft_index, tip_index];
        expected.sort_unstable();
        assert_eq!(instance.take_frame_changes().object_indices(), expected);
        let (shaft_start, shaft_end) = rendered_line(&instance, shaft_index);
        assert_vec2_close(shaft_start, Vec2::new(-0.2, 0.0));
        assert_vec2_close(shaft_end, Vec2::new(-0.05, 0.0));
        assert_points_close(
            rendered_tip(&instance, tip_index),
            [
                Vec2::ZERO,
                Vec2::new(-0.05, 0.025),
                Vec2::new(-0.05, -0.025),
            ],
        );
        assert!(Arc::ptr_eq(
            instance.frame().render_geometries[shaft_index]
                .as_ref()
                .unwrap(),
            &shaft_resource
        ));
        assert!(Arc::ptr_eq(
            instance.frame().render_geometries[tip_index]
                .as_ref()
                .unwrap(),
            &tip_resource
        ));
        for transform in [
            instance.frame().render_transform(shaft_index),
            instance.frame().render_transform(tip_index),
        ] {
            assert!(transform.translation.x.is_finite());
            assert!(transform.translation.y.is_finite());
            assert!(transform.scale.x.is_finite());
            assert!(transform.scale.y.is_finite());
            assert!(transform.rotation.is_finite());
        }
    }

    #[test]
    fn arrow_geometry_matches_shared_constructor_ordering_rules() {
        let policy = noon_compile::CompiledGraphArrowPolicy::new(0.25, 0.35, 0.25, 0.06, 0.05);
        let geometry = arrow_geometry(Vec2::new(-2.0, 0.0), Vec2::new(2.0, 0.0), false, policy);
        assert_eq!(geometry.visible_start, Vec2::new(-1.75, 0.0));
        assert_eq!(geometry.visible_end, Vec2::new(1.75, 0.0));
        assert_eq!(geometry.tip_length, 0.35);
        assert_eq!(geometry.shaft_start, Vec2::new(-1.75, 0.0));
        assert_eq!(geometry.shaft_end, Vec2::new(1.4, 0.0));
        assert_eq!(geometry.stroke_width, 0.06);
    }

    #[test]
    fn arrow_policy_handles_short_and_zero_length_edges_without_nonfinite_geometry() {
        let policy = noon_compile::CompiledGraphArrowPolicy::new(0.25, 0.35, 0.25, 0.06, 0.05);
        for (start, end) in [(Vec2::ZERO, Vec2::new(0.2, 0.0)), (Vec2::ZERO, Vec2::ZERO)] {
            let geometry = arrow_geometry(start, end, false, policy);
            for value in [
                geometry.visible_start.x,
                geometry.visible_start.y,
                geometry.visible_end.x,
                geometry.visible_end.y,
                geometry.direction.x,
                geometry.direction.y,
                geometry.tip_length,
                geometry.stroke_width,
            ] {
                assert!(value.is_finite());
            }
        }
    }

    #[test]
    fn prepared_vertex_motion_exposes_coherent_incident_edge_before_commit() {
        let fixture = line_graph_fixture(0, false);
        let mut index = SemanticExecutionIndex::new();
        let lowered = lower_semantic_execution(&fixture.store, &mut index).unwrap();
        let a_object = index.execution_object_id(fixture.a).unwrap();
        let line_object = index.execution_object_id(fixture.line).unwrap();
        let mut instance = SceneInstance::from_semantic_execution(lowered);
        instance
            .apply_execution_patch(&noon_compile::ExecutionPatch::AddTrack(
                noon_core::TrackDefinition {
                    id: noon_core::TrackId::new(91),
                    object: a_object,
                    property: noon_core::Property::Position,
                    values: noon_core::TrackValues::Vec2 {
                        from: Vec2::new(-1.0, 0.0),
                        to: Vec2::new(-3.0, 2.0),
                    },
                    timing: noon_core::TrackTiming::new(0.0, 1.0, RateFunction::Linear),
                    time_map: noon_core::CompositionTimeMap::identity(),
                },
            ))
            .unwrap();
        let line_index = instance.frame_index_for_object(line_object).unwrap();
        let committed = instance.frame().clone();

        let prepared = instance.prepare_advance_to(0.5).unwrap();
        let line = instance
            .prepared_properties_at(&prepared, line_index, None)
            .unwrap();
        assert_eq!(
            line.bounds,
            Some(noon_core::Rect::new(
                Vec2::new(-2.0, 0.0),
                Vec2::new(1.0, 1.0),
            ))
        );
        assert_eq!(
            instance.frame(),
            &committed,
            "preparation must not publish the staged graph dependency"
        );
    }

    #[test]
    fn final_host_effective_vertex_write_updates_incident_edge_in_same_commit() {
        let fixture = line_graph_fixture(0, false);
        let mut index = SemanticExecutionIndex::new();
        let lowered = lower_semantic_execution(&fixture.store, &mut index).unwrap();
        let a_object = index.execution_object_id(fixture.a).unwrap();
        let line_object = index.execution_object_id(fixture.line).unwrap();
        let revision = fixture.store.scene_revision();
        let mut instance = SceneInstance::from_semantic_execution(lowered);
        instance.take_frame_changes();

        let prepared = instance.prepare_advance_to(0.0).unwrap();
        let effective = instance
            .prepare_effective_property_batch(&[crate::EffectivePropertyWrite::Transform {
                object: a_object,
                transform: Transform2D {
                    translation: Vec2::new(-4.0, 2.0),
                    ..Transform2D::IDENTITY
                },
            }])
            .unwrap();
        instance.commit_prepared_frame(prepared, effective).unwrap();

        assert_eq!(fixture.store.scene_revision(), revision);
        let line_index = instance.frame_index_for_object(line_object).unwrap();
        assert_eq!(
            rendered_line(&instance, line_index),
            (Vec2::new(-4.0, 2.0), Vec2::new(1.0, 0.0))
        );
        let changed = instance.take_frame_changes().object_indices().to_vec();
        assert!(changed.contains(&instance.frame_index_for_object(a_object).unwrap()));
        assert!(changed.contains(&line_index));
    }

    #[test]
    fn timeline_vertex_motion_reuses_same_incident_dependency_path() {
        let fixture = line_graph_fixture(0, false);
        let mut index = SemanticExecutionIndex::new();
        let lowered = lower_semantic_execution(&fixture.store, &mut index).unwrap();
        let a_object = index.execution_object_id(fixture.a).unwrap();
        let line_object = index.execution_object_id(fixture.line).unwrap();
        let mut instance = SceneInstance::from_semantic_execution(lowered);
        instance
            .apply_execution_patch(&noon_compile::ExecutionPatch::AddTrack(
                noon_core::TrackDefinition {
                    id: noon_core::TrackId::new(90),
                    object: a_object,
                    property: noon_core::Property::Position,
                    values: noon_core::TrackValues::Vec2 {
                        from: Vec2::new(-1.0, 0.0),
                        to: Vec2::new(-3.0, 2.0),
                    },
                    timing: noon_core::TrackTiming::new(0.0, 1.0, RateFunction::Linear),
                    time_map: noon_core::CompositionTimeMap::identity(),
                },
            ))
            .unwrap();
        instance.take_frame_changes();
        let before = instance.graph_dependency_visits();
        instance.advance_to(0.5).unwrap();

        let a_index = instance.frame_index_for_object(a_object).unwrap();
        let line_index = instance.frame_index_for_object(line_object).unwrap();
        assert_eq!(instance.graph_dependency_visits() - before, 1);
        let mut expected = vec![a_index, line_index];
        expected.sort_unstable();
        assert_eq!(instance.take_frame_changes().object_indices(), expected);

        assert_eq!(
            rendered_line(&instance, line_index),
            (Vec2::new(-2.0, 1.0), Vec2::new(1.0, 0.0))
        );
    }
}
