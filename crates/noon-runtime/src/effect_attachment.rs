//! Attachment lifetime on the existing execution publication and scheduler.
use std::sync::Arc;

use noon_compile::{
    CompilePatchError, CompiledChannelKey, CompiledEffect, CompiledEffectStack, ExecutionPatch,
};
use noon_core::ObjectId;

use crate::{EvaluationStats, RuntimePatchStats, SceneInstance};

impl SceneInstance {
    pub(crate) fn apply_effect_stack_patch(
        &mut self,
        patch: &ExecutionPatch,
        object: ObjectId,
        mut next: Option<Arc<CompiledEffectStack>>,
    ) -> Result<(), CompilePatchError> {
        let index = self
            .compiled
            .object_index(object)
            .ok_or(CompilePatchError::UnknownObject(object))?;
        let previous = self.compiled.objects()[index as usize].effects.clone();
        self.compiled.preflight_execution_transaction(
            &noon_compile::ExecutionMutationTransaction::from_mutations([patch.clone()]),
        )?;
        // Prepare from the old coherent publication. Surviving bindings borrow
        // the same channels and keep disjoint effective values across topology.
        next = next.map(|stack| self.prepare_effective_effect_values(index as usize, &stack));
        // Compiler validation completes before either authority is changed.
        let compiled = self.compiled.apply_execution_patch_with_stats(patch)?;
        let mut stats = RuntimePatchStats {
            track_locators_removed: compiled.track_locators_removed,
            ..RuntimePatchStats::default()
        };
        for &attachment in previous
            .as_ref()
            .map_or(&[][..], |stack| stack.attachments())
        {
            if next
                .as_ref()
                .is_some_and(|stack| stack.get(attachment).is_some())
            {
                continue;
            }
            for property in CompiledEffect::PARAMETER_PROPERTIES {
                let channel = CompiledChannelKey::effect(index, property, attachment);
                if self.groups.remove(&channel).is_some() {
                    let retired = self.timeline_scheduler.relower_channel(channel, &[]);
                    stats.channels_relowered += retired.groups_relowered;
                    stats.scheduler_events_removed += retired.events_removed;
                    stats.scheduler_events_inserted += retired.events_inserted;
                }
            }
        }
        let index = index as usize;
        self.frame.objects[index].effects = next;
        // Halo coverage is renderer-derived. Semantic geometry, spatial queries,
        // object motion, painter order and all unrelated rows stay untouched.
        self.mark_nonspatial_changed(index);
        self.last_stats = EvaluationStats::default();
        self.last_patch_stats = stats;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use noon_compile::{CompiledObject, CompiledScene, ExecutionMutationTransaction};
    use noon_core::{
        Blur, EffectTrackEndBehavior, EffectTrackValue, EffectUpdate, GeometryRef, Property,
        RateFunction, SemanticNodeId, Style, TrackDefinition, TrackId, TrackTiming, TrackValues,
        Transform2D, Vec2,
    };

    fn blur(slot: u32, radius: f64) -> CompiledEffect {
        CompiledEffect {
            attachment: SemanticNodeId::new(slot, 2),
            definition: Blur::new(EffectUpdate::default().radius(radius))
                .unwrap()
                .into(),
        }
    }

    fn stack(effects: impl IntoIterator<Item = CompiledEffect>) -> Arc<CompiledEffectStack> {
        Arc::new(CompiledEffectStack::new(effects).unwrap())
    }

    fn runtime(effects: Arc<CompiledEffectStack>, tracks: &[TrackDefinition]) -> SceneInstance {
        let mut object = CompiledObject::new(
            ObjectId::new(0),
            GeometryRef::circle(1.0),
            Transform2D::IDENTITY,
            Style::default(),
        );
        object.effects = Some(effects);
        SceneInstance::new(CompiledScene::compile_objects(vec![object], tracks).unwrap())
    }

    fn radius_track(id: u64, effect: CompiledEffect, radius: f64) -> TrackDefinition {
        let (property, values) = effect
            .parameter_channels(EffectUpdate::default().radius(radius))
            .unwrap()
            .remove(0);
        TrackDefinition {
            id: TrackId::new(id),
            object: ObjectId::new(0),
            property,
            values,
            timing: TrackTiming::new(0.0, 2.0, RateFunction::Linear),
            time_map: Default::default(),
        }
    }

    fn radius(runtime: &SceneInstance, id: SemanticNodeId) -> noon_core::EffectRadius {
        runtime.frame().objects[0]
            .effects
            .as_ref()
            .unwrap()
            .get(id)
            .unwrap()
            .definition
            .radius()
    }

    #[test]
    fn same_kind_channels_seek_independently_and_retire_only_removed_generation() {
        let a = blur(10, 1.0);
        let b = blur(11, 2.0);
        let tracks = [
            radius_track(1, a, 5.0),
            radius_track(2, b, 10.0),
            TrackDefinition {
                id: TrackId::new(3),
                object: ObjectId::new(0),
                property: Property::Position,
                values: TrackValues::Vec2 {
                    from: Vec2::ZERO,
                    to: Vec2::ONE,
                },
                timing: TrackTiming::new(0.0, 2.0, RateFunction::Linear),
                time_map: Default::default(),
            },
        ];
        let mut runtime = runtime(stack([a, b]), &tracks);
        runtime.seek(1.0).unwrap();
        assert_eq!(
            radius(&runtime, a.attachment),
            noon_core::EffectRadius::Scene(3.0)
        );
        assert_eq!(
            radius(&runtime, b.attachment),
            noon_core::EffectRadius::Scene(6.0)
        );
        let expected = runtime.frame().objects[0].clone();
        runtime.seek(1.75).unwrap();
        runtime.seek(1.0).unwrap();
        assert_eq!(runtime.frame().objects[0], expected);
        runtime.take_frame_changes();
        runtime.take_spatial_changes();
        runtime.take_nonspatial_changes();
        let survivor_address =
            runtime.compiled.track(TrackId::new(2)).unwrap() as *const _ as usize;
        runtime
            .apply_execution_patch(&ExecutionPatch::SetEffectStack {
                object: ObjectId::new(0),
                expected: Box::new([a.attachment, b.attachment]),
                effects: Some(stack([b])),
            })
            .unwrap();
        assert_eq!(
            radius(&runtime, b.attachment),
            noon_core::EffectRadius::Scene(6.0)
        );
        assert_eq!(runtime.frame().objects[0].transform, expected.transform);
        assert_eq!(runtime.compiled.track_count(), 2);
        assert!(runtime.compiled.track(TrackId::new(1)).is_none());
        assert_eq!(
            runtime.compiled.track(TrackId::new(2)).unwrap() as *const _ as usize,
            survivor_address
        );
        assert!(runtime.compiled.objects()[0].dynamic.effect);
        assert_eq!(
            runtime.take_frame_changes(),
            crate::FrameChanges::objects(vec![0])
        );
        assert_eq!(
            runtime.take_nonspatial_changes(),
            crate::FrameChanges::objects(vec![0]),
            "consuming renderer dirtiness must preserve pointer damage"
        );
        assert!(runtime.take_spatial_changes().is_empty());
        runtime.seek(1.5).unwrap();
        assert_eq!(
            radius(&runtime, b.attachment),
            noon_core::EffectRadius::Scene(8.0)
        );
        let before = runtime.frame().clone();
        let context = runtime.publication_context();
        assert!(runtime
            .apply_execution_patch(&ExecutionPatch::SetEffectStack {
                object: ObjectId::new(0),
                expected: Box::new([a.attachment, b.attachment]),
                effects: None,
            })
            .is_err());
        assert_eq!(runtime.frame(), &before);
        assert_eq!(runtime.publication_context(), context);
    }

    #[test]
    fn prepared_value_columns_share_the_submitted_allocation_and_reject_late_stale_write_atomically(
    ) {
        let a = blur(10, 1.0);
        let b = blur(11, 2.0);
        let mut runtime = runtime(stack([a, b]), &[]);
        let edit = |first: CompiledEffect, second: CompiledEffect, first_radius, second_radius| {
            ExecutionPatch::SetEffects {
                object: ObjectId::new(0),
                effects: stack([
                    CompiledEffect {
                        definition: EffectUpdate::default()
                            .radius(first_radius)
                            .apply_to(first.definition)
                            .unwrap(),
                        ..first
                    },
                    CompiledEffect {
                        definition: EffectUpdate::default()
                            .radius(second_radius)
                            .apply_to(second.definition)
                            .unwrap(),
                        ..second
                    },
                ]),
            }
        };
        let final_patch = edit(a, b, 5.0, 4.0);
        let ExecutionPatch::SetEffects {
            effects: submitted, ..
        } = &final_patch
        else {
            unreachable!()
        };
        let submitted = Arc::clone(submitted);
        let transaction = ExecutionMutationTransaction::from_mutations([
            edit(a, b, 3.0, 2.0),
            edit(a, b, 3.0, 4.0),
            final_patch,
        ]);
        runtime.take_frame_changes();
        runtime.take_spatial_changes();
        runtime.take_nonspatial_changes();
        let before = runtime.publication_context();
        let prepared = runtime
            .prepare_authored_value_publication(
                &transaction,
                Default::default(),
                before,
                before.scene_revision().checked_next().unwrap(),
            )
            .unwrap()
            .unwrap();
        assert_eq!(radius(&runtime, a.attachment), a.definition.radius());
        runtime.commit_prepared_authored_value_publication(prepared);
        assert!(
            Arc::ptr_eq(
                runtime.frame().objects[0].effects.as_ref().unwrap(),
                &submitted
            ),
            "prepared values must retain the exact submitted column allocation"
        );
        assert!(
            Arc::ptr_eq(
                runtime.frame().objects[0].effects.as_ref().unwrap(),
                runtime.compiled.objects()[0].effects.as_ref().unwrap()
            ),
            "undriven prepared values must share the compiled immutable column"
        );
        assert_eq!(
            radius(&runtime, a.attachment),
            noon_core::EffectRadius::Scene(5.0)
        );
        assert_eq!(
            radius(&runtime, b.attachment),
            noon_core::EffectRadius::Scene(4.0)
        );
        assert_eq!(
            runtime.take_frame_changes(),
            crate::FrameChanges::objects(vec![0])
        );
        assert_eq!(
            runtime.take_nonspatial_changes(),
            crate::FrameChanges::objects(vec![0]),
            "prepared ordered values must retain damage after renderer consumption"
        );
        assert!(runtime.take_spatial_changes().is_empty());
        let context = runtime.publication_context();
        let frame = runtime.frame().clone();
        let stale = CompiledEffect {
            attachment: SemanticNodeId::new(b.attachment.slot(), b.attachment.generation() + 1),
            ..b
        };
        assert!(runtime
            .prepare_authored_value_publication(
                &ExecutionMutationTransaction::from_mutations([
                    edit(a, b, 6.0, 4.0),
                    edit(a, stale, 6.0, 7.0)
                ]),
                Default::default(),
                context,
                context.scene_revision().checked_next().unwrap()
            )
            .is_err());
        assert_eq!(runtime.frame(), &frame);
        assert_eq!(runtime.publication_context(), context);
    }

    #[test]
    fn prepared_effect_values_equal_to_presented_frame_adopt_submitted_column_without_damage() {
        let a = blur(10, 1.0);
        let b = blur(11, 2.0);
        let mut runtime = runtime(
            stack([a, b]),
            &[radius_track(1, a, 5.0), radius_track(2, b, 10.0)],
        );
        runtime.seek(1.0).unwrap();
        let submitted = stack([blur(10, 3.0), blur(11, 6.0)]);
        let presented = Arc::clone(runtime.frame().objects[0].effects.as_ref().unwrap());
        assert_eq!(presented, submitted);
        assert!(!Arc::ptr_eq(&presented, &submitted));
        runtime.take_frame_changes();
        runtime.take_spatial_changes();
        runtime.take_nonspatial_changes();

        // The authored radii change, but their active tracks already present the
        // new values. Publish the submitted column without dirtying that picture.
        let before = runtime.publication_context();
        let prepared = runtime
            .prepare_authored_value_publication(
                &ExecutionMutationTransaction::from_mutations([ExecutionPatch::SetEffects {
                    object: ObjectId::new(0),
                    effects: Arc::clone(&submitted),
                }]),
                Default::default(),
                before,
                before.scene_revision().checked_next().unwrap(),
            )
            .unwrap()
            .unwrap();
        runtime.commit_prepared_authored_value_publication(prepared);

        assert!(Arc::ptr_eq(
            runtime.compiled.objects()[0].effects.as_ref().unwrap(),
            &submitted
        ));
        assert!(Arc::ptr_eq(
            runtime.frame().objects[0].effects.as_ref().unwrap(),
            &submitted
        ));
        assert!(runtime.take_frame_changes().is_empty());
        assert!(runtime.take_nonspatial_changes().is_empty());
        assert!(runtime.take_spatial_changes().is_empty());
    }

    #[test]
    fn input_pulse_preserves_sibling_track_and_survives_unrelated_stack_append() {
        let a = CompiledEffect {
            attachment: SemanticNodeId::new(10, 2),
            definition: noon_core::Glow::new(EffectUpdate::default().intensity(0.4))
                .unwrap()
                .into(),
        };
        let b = blur(11, 2.0);
        let c = blur(12, 3.0);
        let mut runtime = runtime(stack([a, b]), &[radius_track(1, b, 10.0)]);
        runtime.seek(1.0).unwrap();
        let prepared = runtime
            .prepare_click_glow_pulse(
                ObjectId::new(0),
                noon_core::SemanticClickGlowPulse::new(2.0, 1.0, RateFunction::ThereAndBack, false),
                a,
                false,
                runtime.publication_context(),
            )
            .unwrap()
            .unwrap();
        runtime.start_transient_animation(prepared).unwrap();
        runtime.advance_interactions(0.0).unwrap();
        runtime.advance_interactions(0.5).unwrap();
        runtime
            .apply_execution_patch(&ExecutionPatch::SetEffectStack {
                object: ObjectId::new(0),
                expected: Box::new([a.attachment, b.attachment]),
                effects: Some(stack([a, b, c])),
            })
            .unwrap();
        assert_eq!(
            runtime.frame().objects[0]
                .effects
                .as_ref()
                .unwrap()
                .get(a.attachment)
                .unwrap()
                .definition
                .as_glow()
                .unwrap()
                .intensity(),
            2.0
        );
        assert_eq!(
            radius(&runtime, b.attachment),
            noon_core::EffectRadius::Scene(6.0)
        );
        runtime.advance_interactions(1.0).unwrap();
        assert!(!runtime.interactions_active());
        assert_eq!(
            runtime.frame().objects[0]
                .effects
                .as_ref()
                .unwrap()
                .get(a.attachment)
                .unwrap()
                .definition
                .as_glow()
                .unwrap()
                .intensity(),
            0.4
        );
        assert_eq!(
            radius(&runtime, b.attachment),
            noon_core::EffectRadius::Scene(6.0)
        );
        // A later exact-attachment timeline driver retains normal precedence.
        runtime
            .apply_execution_patch(&ExecutionPatch::AddTrack(TrackDefinition {
                id: TrackId::new(3),
                object: ObjectId::new(0),
                property: Property::EffectIntensity,
                values: TrackValues::Effect {
                    attachment: a.attachment,
                    from: EffectTrackValue::Intensity(0.4),
                    to: EffectTrackValue::Intensity(1.0),
                    end_behavior: EffectTrackEndBehavior::Sample,
                },
                timing: TrackTiming::new(0.0, 2.0, RateFunction::Linear),
                time_map: Default::default(),
            }))
            .unwrap();
        let current = runtime.frame().objects[0]
            .effects
            .as_ref()
            .unwrap()
            .get(a.attachment)
            .unwrap();
        assert!(runtime
            .prepare_click_glow_pulse(
                ObjectId::new(0),
                noon_core::SemanticClickGlowPulse::new(2.0, 1.0, RateFunction::ThereAndBack, false),
                current,
                false,
                runtime.publication_context()
            )
            .unwrap()
            .is_none());
    }
}
