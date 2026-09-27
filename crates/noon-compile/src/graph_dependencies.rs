//! Local mutation of compiled Graph endpoint dependency slots and reverse indices.

use std::collections::{HashMap, HashSet};

use noon_core::{GraphEdgeId, ObjectId};

use crate::{
    graph_line_execution_content, graph_tip_execution_content, CompilePatchError,
    CompiledGraphDependencyDefinition, CompiledGraphDependencyKind, CompiledGraphEdgeDependency,
    CompiledGraphEdgeKind, CompiledScene,
};

impl CompiledScene {
    pub(super) fn graph_dependencies_patch_changes(
        &self,
        owner: ObjectId,
        definitions: &[CompiledGraphDependencyDefinition],
    ) -> bool {
        let existing = self
            .graph_dependencies_for_owner(owner)
            .iter()
            .map(|&index| (self.graph_edge_dependencies[index as usize].edge(), index))
            .collect::<HashMap<_, _>>();
        existing.len() != definitions.len()
            || definitions.iter().any(|definition| {
                let Some(&index) = existing.get(&definition.edge) else {
                    return true;
                };
                match self.graph_dependency_from_definition(owner, definition) {
                    Ok(dependency) => self.graph_edge_dependencies[index as usize] != dependency,
                    Err(_) => true,
                }
            })
    }

    fn graph_dependency_from_definition(
        &self,
        owner: ObjectId,
        definition: &CompiledGraphDependencyDefinition,
    ) -> Result<CompiledGraphEdgeDependency, CompilePatchError> {
        let resolve = |object| {
            self.object_index(object)
                .ok_or(CompilePatchError::UnknownObject(object))
        };
        let kind = match definition.kind {
            CompiledGraphDependencyKind::Line => CompiledGraphEdgeKind::Line,
            CompiledGraphDependencyKind::Arrow {
                end_tip,
                start_tip,
                policy,
            } => CompiledGraphEdgeKind::Arrow {
                end_tip_index: resolve(end_tip)?,
                start_tip_index: start_tip.map(resolve).transpose()?,
                policy,
            },
        };
        Ok(CompiledGraphEdgeDependency::new(
            owner,
            definition.edge,
            resolve(definition.start_vertex)?,
            resolve(definition.end_vertex)?,
            resolve(definition.line)?,
            kind,
        ))
    }

    /// Remove an owner's old dependency indices from every touched reverse row.
    /// Each row is retained once, so replacing a star graph remains O(E + degree)
    /// instead of repeatedly scanning the center adjacency for every edge.
    fn unindex_graph_dependency_rows(&mut self, indices: &HashSet<u32>) {
        let mut incident_rows = HashSet::new();
        let mut dirty_rows = HashSet::new();
        for &index in indices {
            let dependency = self.graph_edge_dependencies[index as usize];
            incident_rows.insert(dependency.start_vertex_index());
            incident_rows.insert(dependency.end_vertex_index());
            dirty_rows.insert(dependency.start_vertex_index());
            dirty_rows.insert(dependency.end_vertex_index());
            dirty_rows.insert(dependency.line_index());
            if let CompiledGraphEdgeKind::Arrow {
                end_tip_index,
                start_tip_index,
                ..
            } = dependency.kind()
            {
                dirty_rows.insert(end_tip_index);
                dirty_rows.extend(start_tip_index);
            }
        }
        Self::remove_dependency_indices_from_rows(
            &mut self.graph_incident_dependencies,
            incident_rows,
            indices,
        );
        Self::remove_dependency_indices_from_rows(
            &mut self.graph_dirty_dependencies,
            dirty_rows,
            indices,
        );
    }

    fn remove_dependency_indices_from_rows(
        rows: &mut HashMap<u32, Vec<u32>>,
        touched: HashSet<u32>,
        removed: &HashSet<u32>,
    ) {
        for row in touched {
            let remove_row = rows.get_mut(&row).is_some_and(|dependencies| {
                dependencies.retain(|index| !removed.contains(index));
                dependencies.is_empty()
            });
            if remove_row {
                rows.remove(&row);
            }
        }
    }

    fn index_graph_dependency_rows(&mut self, index: u32, dependency: CompiledGraphEdgeDependency) {
        let start = dependency.start_vertex_index();
        let end = dependency.end_vertex_index();
        self.graph_incident_dependencies
            .entry(start)
            .or_default()
            .push(index);
        self.graph_dirty_dependencies
            .entry(start)
            .or_default()
            .push(index);
        if end != start {
            self.graph_incident_dependencies
                .entry(end)
                .or_default()
                .push(index);
            self.graph_dirty_dependencies
                .entry(end)
                .or_default()
                .push(index);
        }
        for row in std::iter::once(dependency.line_index()).chain(match dependency.kind() {
            CompiledGraphEdgeKind::Line => [None, None].into_iter().flatten(),
            CompiledGraphEdgeKind::Arrow {
                end_tip_index,
                start_tip_index,
                ..
            } => [Some(end_tip_index), start_tip_index].into_iter().flatten(),
        }) {
            self.graph_dirty_dependencies
                .entry(row)
                .or_default()
                .push(index);
        }
    }

    pub(super) fn apply_graph_dependencies(
        &mut self,
        owner: ObjectId,
        definitions: &[CompiledGraphDependencyDefinition],
    ) -> Result<(), CompilePatchError> {
        let mut lowered = Vec::with_capacity(definitions.len());
        let mut seen = HashSet::with_capacity(definitions.len());
        for definition in definitions {
            if !seen.insert(definition.edge) {
                return Err(CompilePatchError::DuplicateGraphDependency {
                    owner,
                    edge: definition.edge,
                });
            }
            lowered.push(self.graph_dependency_from_definition(owner, definition)?);
        }

        let old_indices = self.graph_dependencies_for_owner(owner).to_vec();
        let old_index_set = old_indices.iter().copied().collect::<HashSet<_>>();
        self.unindex_graph_dependency_rows(&old_index_set);
        let keep = lowered
            .iter()
            .map(|dependency| dependency.edge())
            .collect::<HashSet<GraphEdgeId>>();
        for index in old_indices {
            let edge = self.graph_edge_dependencies[index as usize].edge();
            if keep.contains(&edge) {
                continue;
            }
            self.graph_dependency_indices.remove(&(owner, edge));
            self.graph_edge_dependencies[index as usize].live = false;
            self.free_graph_dependency_indices.push(index);
        }

        self.graph_owner_dependencies.remove(&owner);
        for dependency in lowered {
            let key = (owner, dependency.edge());
            let index = if let Some(&index) = self.graph_dependency_indices.get(&key) {
                index
            } else if let Some(index) = self.free_graph_dependency_indices.pop() {
                self.graph_dependency_indices.insert(key, index);
                index
            } else {
                let index = u32::try_from(self.graph_edge_dependencies.len()).map_err(|_| {
                    CompilePatchError::TooManyGraphDependencies(
                        self.graph_edge_dependencies.len().saturating_add(1),
                    )
                })?;
                self.graph_dependency_indices.insert(key, index);
                self.graph_edge_dependencies.push(dependency);
                index
            };
            self.graph_edge_dependencies[index as usize] = dependency;
            self.index_graph_dependency_rows(index, dependency);
            self.graph_owner_dependencies
                .entry(owner)
                .or_default()
                .push(index);
            self.objects[dependency.line_index() as usize].content = graph_line_execution_content();
            if let CompiledGraphEdgeKind::Arrow {
                end_tip_index,
                start_tip_index,
                ..
            } = dependency.kind()
            {
                self.objects[end_tip_index as usize].content = graph_tip_execution_content();
                if let Some(start_tip_index) = start_tip_index {
                    self.objects[start_tip_index as usize].content = graph_tip_execution_content();
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use noon_core::{GeometryRef, Style, Transform2D};

    use super::*;
    use crate::{CompiledObject, ExecutionMutationTransaction, ExecutionPatch};

    fn object(id: u64) -> CompiledObject {
        CompiledObject::new(
            ObjectId::new(id),
            GeometryRef::circle(1.0),
            Transform2D::IDENTITY,
            Style::default(),
        )
    }

    fn line(edge: u64, start: u64, end: u64, line: u64) -> CompiledGraphDependencyDefinition {
        CompiledGraphDependencyDefinition {
            edge: GraphEdgeId::new(edge),
            start_vertex: ObjectId::new(start),
            end_vertex: ObjectId::new(end),
            line: ObjectId::new(line),
            kind: CompiledGraphDependencyKind::Line,
        }
    }

    #[test]
    fn replacing_one_owner_reuses_slots_without_relocating_other_graphs() {
        let mut scene = CompiledScene::compile_objects((1..=7).map(object).collect(), &[])
            .expect("objects compile");
        let first_owner = ObjectId::new(100);
        let second_owner = ObjectId::new(200);
        scene
            .apply_execution_patch(&ExecutionPatch::SetGraphDependencies {
                owner: first_owner,
                dependencies: vec![line(10, 1, 2, 5), line(11, 1, 3, 6)],
            })
            .unwrap();
        scene
            .apply_execution_patch(&ExecutionPatch::SetGraphDependencies {
                owner: second_owner,
                dependencies: vec![line(20, 3, 4, 7)],
            })
            .unwrap();

        let retained = scene.graph_dependency_indices[&(first_owner, GraphEdgeId::new(10))];
        let retired = scene.graph_dependency_indices[&(first_owner, GraphEdgeId::new(11))];
        let unrelated = scene.graph_dependency_indices[&(second_owner, GraphEdgeId::new(20))];
        let slot_count = scene.graph_edge_dependencies.len();
        scene
            .apply_execution_patch(&ExecutionPatch::SetGraphDependencies {
                owner: first_owner,
                dependencies: vec![line(10, 2, 3, 5), line(12, 1, 4, 6)],
            })
            .unwrap();

        assert_eq!(
            scene.graph_dependency_indices[&(first_owner, GraphEdgeId::new(10))],
            retained
        );
        assert_eq!(
            scene.graph_dependency_indices[&(first_owner, GraphEdgeId::new(12))],
            retired
        );
        assert_eq!(
            scene.graph_dependency_indices[&(second_owner, GraphEdgeId::new(20))],
            unrelated
        );
        assert_eq!(scene.graph_edge_dependencies.len(), slot_count);
        assert!(!scene
            .graph_dependency_indices
            .contains_key(&(first_owner, GraphEdgeId::new(11))));
        assert_eq!(
            scene.graph_dependencies_for_owner(second_owner),
            &[unrelated]
        );
        assert!(scene
            .incident_graph_dependencies(scene.object_index(ObjectId::new(1)).unwrap())
            .iter()
            .all(|&index| index != unrelated));
    }

    #[test]
    fn graph_dependency_preflight_failure_leaves_compiled_scene_unchanged() {
        let scene = CompiledScene::compile_objects((1..=3).map(object).collect(), &[])
            .expect("objects compile");
        let before = scene.clone();
        let transaction =
            ExecutionMutationTransaction::from_mutations([ExecutionPatch::SetGraphDependencies {
                owner: ObjectId::new(100),
                dependencies: vec![line(10, 1, 2, 3), line(11, 1, 999, 3)],
            }]);

        assert_eq!(
            scene.preflight_execution_transaction(&transaction),
            Err(CompilePatchError::UnknownObject(ObjectId::new(999)))
        );
        assert_eq!(scene, before);
    }
}
