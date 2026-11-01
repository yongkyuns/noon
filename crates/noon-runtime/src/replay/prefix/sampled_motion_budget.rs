//! Allocation identities already pinned by promised history; no resource owner.

use std::{collections::BTreeSet, sync::Arc};

use noon_core::SampledMotionPlan;

use super::{PrefixError, ReplayError};

#[derive(Debug, Default)]
pub(super) struct SampledMotionPayloads {
    plans: BTreeSet<usize>,
    data: BTreeSet<usize>,
}

impl SampledMotionPayloads {
    /// Only new suffix tracks are visited. An allocation is charged once even
    /// when Position/Rotation or several activations share the immutable table.
    /// The source/reader history pins admitted Arcs for this stream's lifetime.
    pub(super) fn additional<'a>(
        &self,
        plans: impl Iterator<Item = &'a Arc<SampledMotionPlan>>,
    ) -> Result<(Self, usize), PrefixError> {
        let mut delta = Self::default();
        let mut bytes = 0usize;
        for plan in plans {
            let key = Arc::as_ptr(plan) as usize;
            if !self.plans.contains(&key) && delta.plans.insert(key) {
                bytes = bytes
                    .checked_add(std::mem::size_of::<SampledMotionPlan>())
                    .ok_or(ReplayError::RetentionLimit)?;
            }
            let key = Arc::as_ptr(plan.motion()) as usize;
            if !self.data.contains(&key) && delta.data.insert(key) {
                bytes = bytes
                    .checked_add(plan.motion().retained_bytes())
                    .ok_or(ReplayError::RetentionLimit)?;
            }
        }
        Ok((delta, bytes))
    }

    pub(super) fn extend(&mut self, delta: Self) {
        self.plans.extend(delta.plans);
        self.data.extend(delta.data);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ReplayLimits, SceneInstance};
    use noon_compile::{CompiledObject, CompiledScene, ExecutionPatch};
    use noon_core::{
        CompositionTimeMap, GeometryRef, ObjectId, Property, RateFunction, SampledMotionData,
        SemanticVec3, Style, TrackDefinition, TrackId, TrackTiming, TrackValues, Transform2D, Vec2,
    };

    fn data() -> Arc<SampledMotionData> {
        Arc::new(
            SampledMotionData::new(
                vec![0.1, 0.3, 1.0],
                vec![
                    SemanticVec3::new(1.0, 0.0, 0.0),
                    SemanticVec3::new(2.0, 1.0, 0.0),
                    SemanticVec3::new(3.0, 0.0, 0.0),
                ],
                vec![0.5, -2.0, -8.0],
            )
            .unwrap(),
        )
    }
    fn plan(data: Arc<SampledMotionData>) -> Arc<SampledMotionPlan> {
        Arc::new(SampledMotionPlan::new(data, Transform2D::IDENTITY, Vec2::ZERO).unwrap())
    }
    fn definitions(plan: &Arc<SampledMotionPlan>, first: u64, start: f64) -> [TrackDefinition; 2] {
        [
            (
                Property::Position,
                TrackValues::SampledPosition { plan: plan.clone() },
            ),
            (
                Property::Rotation,
                TrackValues::SampledRotation { plan: plan.clone() },
            ),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, (property, values))| TrackDefinition {
            id: TrackId::new(first + index as u64),
            object: ObjectId::new(1),
            property,
            values,
            timing: TrackTiming::new(start, 1.0, RateFunction::Linear),
            time_map: CompositionTimeMap::identity(),
        })
        .collect::<Vec<_>>()
        .try_into()
        .unwrap()
    }
    fn source(plan: &Arc<SampledMotionPlan>) -> SceneInstance {
        let object = CompiledObject::new(
            ObjectId::new(1),
            GeometryRef::circle(0.5),
            Transform2D::IDENTITY,
            Style::default(),
        );
        let compiled =
            CompiledScene::compile_objects(vec![object], &definitions(plan, 1, 0.0)).unwrap();
        let mut source = SceneInstance::new(compiled);
        source
            .begin_replay_retention(ReplayLimits {
                revisions: 32,
                payloads: 1000,
            })
            .unwrap();
        source.advance_to(1.0).unwrap();
        source
    }

    #[test]
    fn sampled_motion_resource_accounting_deduplicates_shared_allocations_not_equal_values() {
        let data = data();
        let first = plan(data.clone());
        let second = plan(data.clone());
        let decoded_equivalent = plan(Arc::new((*data).clone()));
        let mut known = SampledMotionPayloads::default();
        let (delta, bytes) = known
            .additional([&first, &first, &second].into_iter())
            .unwrap();
        assert_eq!(
            bytes,
            2 * std::mem::size_of::<SampledMotionPlan>() + data.retained_bytes()
        );
        known.extend(delta);
        assert_eq!(
            known.additional([&first, &second].into_iter()).unwrap().1,
            0
        );
        let (_, bytes) = known.additional([&decoded_equivalent].into_iter()).unwrap();
        assert_eq!(
            bytes,
            std::mem::size_of::<SampledMotionPlan>() + data.retained_bytes()
        );
    }

    #[test]
    fn sampled_motion_bootstrap_charges_shared_tables_before_clone() {
        let first = plan(data());
        let source = source(&first);
        let bytes = std::mem::size_of::<SampledMotionPlan>() + first.motion().retained_bytes();
        let frame = source.frame().clone();
        let publication = source.publication_context();
        assert!(
            matches!(source.create_prefix_projection_with_resource_budget(bytes - 1), Err(PrefixError::ResourceBudgetExceeded { additional_bytes, .. }) if additional_bytes == bytes)
        );
        assert_eq!(source.frame(), &frame);
        assert_eq!(source.publication_context(), publication);
        let (publisher, reader, receiver) = source
            .create_prefix_projection_with_resource_budget(bytes)
            .unwrap();
        assert_eq!(publisher.extracted_resource_bytes(), bytes);
        assert_eq!(receiver.retained_resource_bytes(), bytes);
        assert_eq!(reader.frame().time, 0.0);
    }

    #[test]
    fn sampled_motion_suffix_rejection_is_atomic_and_valid_delivery_preserves_displayed_time() {
        let first = plan(data());
        let mut source = source(&first);
        let bytes = std::mem::size_of::<SampledMotionPlan>() + first.motion().retained_bytes();
        let (mut limited, _, _) = source
            .create_prefix_projection_with_resource_budget(2 * bytes - 1)
            .unwrap();
        let (mut publisher, mut reader, mut receiver) = source
            .create_prefix_projection_with_resource_budget(2 * bytes)
            .unwrap();
        let second = plan(data());
        for definition in definitions(&second, 3, 1.0) {
            source
                .apply_execution_patch(&ExecutionPatch::AddTrack(definition))
                .unwrap();
        }
        source.advance_to(2.0).unwrap();
        let source_frame = source.frame().clone();
        assert!(matches!(
            source.prepare_prefix_update(&mut limited),
            Err(PrefixError::ResourceBudgetExceeded { .. })
        ));
        assert_eq!(limited.frontier(), 1.0);
        assert_eq!(limited.extracted_resource_bytes(), bytes);
        assert_eq!(source.frame(), &source_frame);
        let batch = source.prepare_prefix_update(&mut publisher).unwrap();
        assert_eq!(batch.resource_bytes(), bytes);
        assert!(batch.stats().payloads >= 2 * second.motion().len());
        receiver.append(&mut reader, batch).unwrap();
        assert_eq!(reader.frame().time, 0.0);
        assert_eq!(receiver.retained_resource_bytes(), 2 * bytes);
        for time in [0.1, 0.7, 1.1, 1.7, 0.2] {
            receiver.prepare_seek(&mut reader, time).unwrap();
            reader.seek(time).unwrap();
            let phase = if time < 1.0 { time } else { time - 1.0 };
            let expected = if time < 1.0 {
                first.sample(phase)
            } else {
                second.sample(phase)
            };
            let pose = reader.frame().objects[0].transform;
            assert_eq!((pose.translation, pose.rotation), expected);
        }
    }
}
