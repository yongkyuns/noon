//! Logical immutable reveal-table storage retained by a prefix stream.
use std::{collections::BTreeSet, sync::Arc};

use noon_core::RevealPhaseMap;

use super::{PrefixError, ReplayError};

#[derive(Debug, Default)]
pub(super) struct PhaseRevealPayloads {
    maps: BTreeSet<usize>,
}

impl PhaseRevealPayloads {
    /// Only the new dependency closure is inspected. History pins admitted Arcs
    /// for this stream's lifetime, so allocation identity is stable across seeks.
    pub(super) fn additional<'a>(
        &self,
        maps: impl Iterator<Item = &'a Arc<RevealPhaseMap>>,
    ) -> Result<(Self, usize), PrefixError> {
        let mut delta = Self::default();
        let mut bytes = 0usize;
        for map in maps {
            let key = Arc::as_ptr(map) as usize;
            if !self.maps.contains(&key) && delta.maps.insert(key) {
                bytes = bytes
                    .checked_add(map.retained_bytes())
                    .ok_or(ReplayError::RetentionLimit)?;
            }
        }
        Ok((delta, bytes))
    }

    pub(super) fn extend(&mut self, delta: Self) {
        self.maps.extend(delta.maps);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ReplayLimits, SceneInstance};
    use noon_compile::{CompiledObject, CompiledScene, ExecutionPatch};
    use noon_core::{
        CompositionTimeMap, GeometryRef, ObjectId, Property, RateFunction, Style, TrackDefinition,
        TrackId, TrackTiming, TrackValues, Transform2D, Vec2, VectorPath,
    };

    fn phase_map() -> Arc<RevealPhaseMap> {
        Arc::new(RevealPhaseMap::new(vec![0.0, 0.13, 0.4, 1.0]).unwrap())
    }

    fn definition(
        map: &Arc<RevealPhaseMap>,
        track: u64,
        object: u64,
        start: f64,
        reverse: bool,
    ) -> TrackDefinition {
        let mut timing = TrackTiming::new(start, 1.0, RateFunction::Linear);
        timing.reverse_rate_function = reverse;
        TrackDefinition {
            id: TrackId::new(track),
            object: ObjectId::new(object),
            property: Property::Reveal,
            values: TrackValues::PhaseReveal {
                phase_map: map.clone(),
            },
            timing,
            time_map: CompositionTimeMap::identity(),
        }
    }

    fn source(map: &Arc<RevealPhaseMap>, payload_limit: usize) -> SceneInstance {
        let path = Arc::new(
            VectorPath::new()
                .move_to(Vec2::ZERO)
                .line_to(Vec2::new(1.0, 0.0))
                .line_to(Vec2::new(1.0, 2.0))
                .line_to(Vec2::new(-1.0, 2.0)),
        );
        let objects = [1, 2]
            .map(|id| {
                CompiledObject::new(
                    ObjectId::new(id),
                    GeometryRef::VectorPath(path.clone()),
                    Transform2D::IDENTITY,
                    Style::default(),
                )
            })
            .to_vec();
        let definitions = [
            definition(map, 1, 1, 0.0, false),
            definition(map, 2, 2, 0.0, false),
        ];
        let compiled = CompiledScene::compile_objects(objects, &definitions).unwrap();
        let mut source = SceneInstance::new(compiled);
        source
            .begin_replay_retention(ReplayLimits {
                revisions: 32,
                payloads: payload_limit,
            })
            .unwrap();
        source.advance_to(1.0).unwrap();
        source
    }

    #[test]
    fn phase_reveal_resource_bytes_deduplicate_shared_allocations_not_equal_values() {
        let map = phase_map();
        let shared = map.clone();
        let separately_allocated = Arc::new((*map).clone());
        assert_eq!(map, separately_allocated);
        let mut known = PhaseRevealPayloads::default();
        let (delta, bytes) = known.additional([&map, &shared].into_iter()).unwrap();
        assert_eq!(bytes, map.retained_bytes());
        known.extend(delta);
        assert_eq!(known.additional([&map].into_iter()).unwrap().1, 0);
        assert_eq!(
            known
                .additional([&separately_allocated, &separately_allocated].into_iter())
                .unwrap()
                .1,
            separately_allocated.retained_bytes()
        );
    }

    #[test]
    fn phase_reveal_bootstrap_counts_shared_tables_once_and_rejects_before_clone() {
        let map = phase_map();
        let source = source(&map, 1000);
        let bytes = map.retained_bytes();
        let frame = source.frame().clone();
        let publication = source.publication_context();
        assert!(matches!(
            source.create_prefix_projection_with_resource_budget(bytes - 1),
            Err(PrefixError::ResourceBudgetExceeded {
                retained_bytes: 0,
                additional_bytes,
                limit_bytes,
            }) if additional_bytes == bytes && limit_bytes == bytes - 1
        ));
        assert_eq!(source.frame(), &frame);
        assert_eq!(source.publication_context(), publication);
        let (publisher, reader, receiver) = source
            .create_prefix_projection_with_resource_budget(bytes)
            .unwrap();
        assert_eq!(publisher.extracted_resource_bytes(), bytes);
        assert_eq!(receiver.retained_resource_bytes(), bytes);
        assert_eq!(reader.frame().time, 0.0);
        assert_eq!(reader.frame().reveal(0), 0.0);
        assert_eq!(reader.frame().reveal(1), 0.0);
    }

    #[test]
    fn phase_reveal_prefix_budget_rejection_is_atomic_and_adoption_preserves_the_held_frame() {
        use super::super::resource_budget::ResourceBudget;

        let first = phase_map();
        let mut source = source(&first, 1000);
        let bytes = first.retained_bytes();
        let (mut limited, _, _) = source
            .create_prefix_projection_with_resource_budget(2 * bytes - 1)
            .unwrap();
        let (mut publisher, mut reader, mut receiver) = source
            .create_prefix_projection_with_resource_budget(2 * bytes)
            .unwrap();
        receiver.prepare_seek(&mut reader, 0.25).unwrap();
        reader.seek(0.25).unwrap();
        reader.take_frame_changes();
        let displayed = reader.frame().clone();
        let displayed_context = reader.publication_context();

        let second = Arc::new(RevealPhaseMap::new(vec![0.0, 0.37, 0.8, 1.0]).unwrap());
        source.advance_to(1.5).unwrap();
        source
            .apply_execution_patch(&ExecutionPatch::AddTrack(definition(
                &second, 3, 1, 1.5, true,
            )))
            .unwrap();
        source.advance_to(2.5).unwrap();
        let source_frame = source.frame().clone();
        let source_context = source.publication_context();
        assert!(matches!(
            source.prepare_prefix_update(&mut limited),
            Err(PrefixError::ResourceBudgetExceeded { additional_bytes, .. })
                if additional_bytes == second.retained_bytes()
        ));
        assert_eq!(limited.frontier(), 1.0);
        assert_eq!(limited.extracted_resource_bytes(), bytes);
        assert_eq!(source.frame(), &source_frame);
        assert_eq!(source.publication_context(), source_context);

        let batch = source.prepare_prefix_update(&mut publisher).unwrap();
        assert_eq!(batch.resource_bytes(), second.retained_bytes());
        assert!(batch.stats().payloads >= second.len());
        receiver.resource_budget = ResourceBudget::new(2 * bytes - 1)
            .with_additional(bytes)
            .unwrap();
        assert!(matches!(
            receiver.preflight_append(&reader, &batch),
            Err(PrefixError::ResourceBudgetExceeded { .. })
        ));
        assert_eq!(receiver.frontier(), 1.0);
        assert_eq!(receiver.retained_resource_bytes(), bytes);
        assert_eq!(reader.frame(), &displayed);
        assert_eq!(reader.publication_context(), displayed_context);
        receiver.resource_budget = ResourceBudget::new(2 * bytes)
            .with_additional(bytes)
            .unwrap();
        receiver.append(&mut reader, batch).unwrap();
        assert_eq!(receiver.retained_resource_bytes(), 2 * bytes);
        assert_eq!(reader.frame(), &displayed);
        assert_eq!(reader.publication_context(), displayed_context);
        assert!(reader.take_frame_changes().object_indices().is_empty());

        for time in [0.13, 0.5, 1.2, 1.5, 2.0, 0.25] {
            receiver.prepare_seek(&mut reader, time).unwrap();
            reader.seek(time).unwrap();
            let expected = if time < 1.5 {
                first.sample(time.min(1.0))
            } else {
                second.sample(1.0 - (time - 1.5))
            };
            assert_eq!(reader.frame().reveal(0), expected as f32);
            assert_eq!(reader.frame().reveal(1), first.sample(time.min(1.0)) as f32);
        }
        source.seal_replay().unwrap();
        let terminal = source.prepare_prefix_finish(&mut publisher).unwrap();
        assert_eq!(terminal.resource_bytes(), 0);
        let displayed = reader.frame().clone();
        receiver.append(&mut reader, terminal).unwrap();
        assert_eq!(reader.frame(), &displayed);
        receiver.prepare_seek(&mut reader, 2.5).unwrap();
        reader.seek(2.5).unwrap();
        assert_eq!(reader.frame().reveal(0), 0.0);
        assert_eq!(reader.frame().reveal(1), 1.0);
    }

    #[test]
    fn phase_reveal_key_budget_failure_preserves_forward_execution_and_denies_history() {
        let map = phase_map();
        let mut source = source(&map, map.len() - 1);
        source
            .apply_execution_patch(&ExecutionPatch::AddTrack(definition(&map, 3, 1, 1.0, true)))
            .unwrap();
        assert!(!source.replay_retention_valid());
        source.advance_to(1.5).unwrap();
        assert_eq!(source.frame().reveal(0), map.sample(0.5) as f32);
        assert_eq!(source.seal_replay(), Err(ReplayError::RetentionLimit));
        assert!(matches!(
            source.create_prefix_projection(),
            Err(PrefixError::Replay(ReplayError::RetentionLimit))
        ));
    }
}
