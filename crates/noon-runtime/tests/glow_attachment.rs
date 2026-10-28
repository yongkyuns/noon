//! Structural attachment projection, not public Scene or browser qualification.
use std::sync::Arc;

use noon_compile::{
    CompilePatchError, CompiledChannelKey, CompiledEffect, CompiledObject, CompiledResources,
    CompiledScene, ExecutionMutationTransaction, ExecutionPatch,
};
use noon_core::{
    Color, EffectUpdate, GeometryRef, Glow, GlowSource, ObjectId, Pixels, Property, RateFunction,
    SemanticNodeId, Style, TrackDefinition, TrackId, TrackTiming, TrackValues, Transform2D, Vec2,
};
use noon_runtime::{FrameChanges, ReplayLimits, SceneInstance};

fn attachment(generation: u32) -> Arc<CompiledEffect> {
    Arc::new(CompiledEffect {
        attachment: SemanticNodeId::new(7, generation),
        definition: (Glow::new(
            EffectUpdate::default()
                .radius(Pixels(3.25))
                .color(Color::RED)
                .intensity(0.4),
        )
        .unwrap())
        .into(),
    })
}
fn source(effect: Option<Arc<CompiledEffect>>) -> CompiledObject {
    let mut row = CompiledObject::new(
        ObjectId::new(1),
        GeometryRef::circle(0.4),
        Transform2D::IDENTITY,
        Style::default(),
    );
    row.effects = effect.map(|effect| stack(*effect));
    row
}
fn change(expected: Option<SemanticNodeId>, effect: Option<Arc<CompiledEffect>>) -> ExecutionPatch {
    ExecutionPatch::SetEffectStack {
        object: ObjectId::new(1),
        expected: expected.into_iter().collect(),
        effects: effect.map(|effect| stack(*effect)),
    }
}
fn stack(effect: CompiledEffect) -> Arc<noon_compile::CompiledEffectStack> {
    Arc::new(noon_compile::CompiledEffectStack::new([effect]).unwrap())
}
fn only(stack: &noon_compile::CompiledEffectStack) -> CompiledEffect {
    assert_eq!(stack.len(), 1);
    stack.iter().next().unwrap()
}
fn channels(effect: &CompiledEffect, start: f64, offset: u64) -> Vec<TrackDefinition> {
    effect
        .parameter_channels(
            EffectUpdate::default()
                .radius(Pixels(6.5))
                .color(Color::BLUE)
                .intensity(2.0),
        )
        .unwrap()
        .into_iter()
        .enumerate()
        .map(|(i, (property, values))| TrackDefinition {
            id: TrackId::new(offset + i as u64),
            object: ObjectId::new(1),
            property,
            values,
            timing: TrackTiming::new(start, 1.0, RateFunction::Linear),
            time_map: Default::default(),
        })
        .collect()
}
fn motion() -> TrackDefinition {
    TrackDefinition {
        id: TrackId::new(100),
        object: ObjectId::new(1),
        property: Property::Position,
        values: TrackValues::Vec2 {
            from: Vec2::ZERO,
            to: Vec2::new(2.0, 1.0),
        },
        timing: TrackTiming::new(0.0, 2.0, RateFunction::Linear),
        time_map: Default::default(),
    }
}
fn scene(effect: Option<Arc<CompiledEffect>>, tracks: &[TrackDefinition]) -> CompiledScene {
    CompiledScene::compile_objects(vec![source(effect)], tracks).unwrap()
}
fn tx(patches: impl IntoIterator<Item = ExecutionPatch>) -> ExecutionMutationTransaction {
    ExecutionMutationTransaction::from_mutations(patches)
}

#[test]
fn parameter_channels_require_exact_attachment_addresses_for_every_finite_kind() {
    use noon_core::{Blur, EffectDefinition, Shadow};
    for (definition, update, properties) in [
        (
            EffectDefinition::Glow(Glow::new(EffectUpdate::default().radius(Pixels(2.0))).unwrap()),
            EffectUpdate::default()
                .color(Color::BLUE)
                .radius(Pixels(4.0))
                .intensity(2.0),
            vec![
                Property::EffectColor,
                Property::EffectRadius,
                Property::EffectIntensity,
            ],
        ),
        (
            EffectDefinition::Blur(Blur::new(EffectUpdate::default().radius(Pixels(2.0))).unwrap()),
            EffectUpdate::default().radius(Pixels(4.0)),
            vec![Property::EffectRadius],
        ),
        (
            EffectDefinition::Shadow(
                Shadow::new(
                    EffectUpdate::default()
                        .radius(Pixels(2.0))
                        .offset(noon_core::EffectOffset::Pixels([0.0, 0.0])),
                )
                .unwrap(),
            ),
            EffectUpdate::default()
                .color(Color::RED)
                .radius(Pixels(4.0))
                .offset(noon_core::EffectOffset::Pixels([2.0, -1.0])),
            vec![
                Property::EffectColor,
                Property::EffectRadius,
                Property::EffectOffset,
            ],
        ),
    ] {
        let effect = Arc::new(CompiledEffect {
            attachment: SemanticNodeId::new(7, 3),
            definition,
        });
        let tracks: Vec<_> = effect
            .parameter_channels(update)
            .unwrap()
            .into_iter()
            .enumerate()
            .map(|(id, (property, values))| TrackDefinition {
                id: TrackId::new(id as u64),
                object: ObjectId::new(1),
                property,
                values,
                timing: TrackTiming::new(0.0, 1.0, RateFunction::Linear),
                time_map: Default::default(),
            })
            .collect();
        let compiled = scene(Some(effect.clone()), &tracks);
        for property in properties {
            let key = CompiledChannelKey::effect(0, property, effect.attachment);
            assert_eq!(
                compiled
                    .channels_for_object_property(0, property)
                    .collect::<Vec<_>>(),
                vec![key]
            );
            let channel = compiled.channel_tracks(key);
            assert_eq!(channel.len(), 1);
            assert_eq!(compiled.channel_for_track(channel[0].id), Some(key));
            assert_eq!(compiled.track(channel[0].id).unwrap().channel(), key);
            assert!(compiled
                .channel_tracks(CompiledChannelKey::new(0, property))
                .is_empty());
            for foreign in [SemanticNodeId::new(7, 4), SemanticNodeId::new(8, 3)] {
                assert!(compiled
                    .channel_tracks(CompiledChannelKey::effect(0, property, foreign))
                    .is_empty());
            }
            assert!(compiled
                .channels_for_object_property(1, property)
                .next()
                .is_none());
        }
        let mut runtime = SceneInstance::new(compiled);
        runtime.advance_to(0.5).unwrap();
        let midpoint = runtime.frame().clone();
        runtime.advance_to(1.0).unwrap();
        assert_eq!(
            only(runtime.frame().objects[0].effects.as_ref().unwrap()).definition,
            update.apply_to(definition).unwrap()
        );
        runtime.seek(0.5).unwrap();
        assert_eq!(runtime.frame(), &midpoint);
    }
}

#[test]
fn compaction_relocates_channels_without_replacing_attachment_identity() {
    let a = attachment(3);
    let tracks = channels(&a, 0.0, 1);
    let mut prefix = source(None);
    prefix.id = ObjectId::new(99);
    let compiled =
        CompiledScene::compile_objects(vec![prefix, source(Some(a.clone()))], &tracks).unwrap();
    let mut runtime = SceneInstance::new(compiled);
    runtime.advance_to(0.25).unwrap();
    let effective = runtime.frame().objects[1].clone();
    runtime
        .apply_execution_patch(&ExecutionPatch::RemoveObject(ObjectId::new(99)))
        .unwrap();
    let stats = runtime.reclaim_retired_object_slots().unwrap();
    assert_eq!(stats.compiled.track_rows_reindexed, 3);
    assert_eq!(runtime.frame().objects.len(), 1);
    assert_eq!(runtime.frame().objects[0], effective);
    runtime.advance_to(0.5).unwrap();
    assert!(
        (only(runtime.frame().objects[0].effects.as_ref().unwrap())
            .definition
            .as_glow()
            .unwrap()
            .intensity()
            - 1.2)
            .abs()
            < 1e-12
    );
    runtime.take_frame_changes();
    runtime.advance_to(0.5).unwrap();
    assert!(runtime.frame_changes().is_empty());
    runtime.seek(0.25).unwrap();
    assert_eq!(runtime.frame().objects[0], effective);
    runtime
        .apply_execution_patch(&change(Some(a.attachment), None))
        .unwrap();
    assert_eq!(runtime.last_patch_stats().track_locators_removed, 3);
    runtime.advance_to(1.0).unwrap();
    assert!(runtime.frame().objects[0].effects.is_none());
    runtime.take_frame_changes();
    runtime.advance_to(1.5).unwrap();
    assert_eq!(runtime.last_stats().groups_evaluated, 0);
    assert!(runtime.frame_changes().is_empty());
}

#[test]
fn staged_object_retirement_releases_effect_and_priority_track_ids_atomically() {
    let a = attachment(3);
    let b = attachment(4);
    let priority = TrackDefinition {
        id: TrackId::new(50),
        object: ObjectId::new(1),
        property: Property::ZIndex,
        values: TrackValues::ZIndex { from: 0.0, to: 2.0 },
        timing: TrackTiming::instant(0.0),
        time_map: Default::default(),
    };
    let mut edits = vec![change(None, Some(a.clone()))];
    edits.extend(
        channels(&a, 0.0, 1)
            .into_iter()
            .map(ExecutionPatch::AddTrack),
    );
    edits.push(ExecutionPatch::AddTrack(priority.clone()));
    edits.push(ExecutionPatch::RemoveObject(ObjectId::new(1)));
    edits.push(ExecutionPatch::CreateObject(source(Some(b.clone()))));
    edits.extend(
        channels(&b, 0.0, 1)
            .into_iter()
            .map(ExecutionPatch::AddTrack),
    );
    edits.push(ExecutionPatch::AddTrack(priority));
    let transaction = tx(edits);
    let mut sequential = scene(None, &[]);
    for patch in transaction.mutations() {
        sequential.apply_execution_patch(patch).unwrap();
    }
    assert_eq!(sequential.track_count(), 4);
    let mut runtime = SceneInstance::new(scene(None, &[]));
    runtime.apply_execution_transaction(&transaction).unwrap();
    runtime.advance_to(0.5).unwrap();
    assert_eq!(runtime.frame().objects[0].z_index, 2.0);
    let current = only(runtime.frame().objects[0].effects.as_ref().unwrap());
    assert_eq!(current.attachment, b.attachment);
    assert!((current.definition.as_glow().unwrap().intensity() - 1.2).abs() < 1e-12);
}

#[test]
fn attach_remove_and_new_generation_preserve_the_object_and_only_dirty_rendering() {
    let mut rt = SceneInstance::new(scene(None, &[]));
    let a = attachment(3);
    let b = attachment(4);
    let original = rt.frame().objects[0].clone();
    let bounds = rt.effective_object_bounds(ObjectId::new(1));
    for (expected, next) in [
        (None, Some(a.clone())),
        (Some(a.attachment), None),
        (None, Some(b.clone())),
    ] {
        rt.take_frame_changes();
        rt.take_spatial_changes();
        let before = rt.publication_context();
        let patch = change(expected, next.clone());
        assert!(
            rt.prepare_authored_value_publication(
                &tx([patch.clone()]),
                Default::default(),
                before,
                before.scene_revision().checked_next().unwrap()
            )
            .unwrap()
            .is_none(),
            "topology cannot use the existing-value fast lane"
        );
        rt.apply_authored_execution_transaction(
            &tx([patch]),
            CompiledResources::default(),
            before,
            before.scene_revision().checked_next().unwrap(),
        )
        .unwrap();
        assert_eq!(
            rt.frame().objects[0]
                .effects
                .as_ref()
                .map(|stack| only(stack)),
            next.as_deref().copied()
        );
        let mut row = rt.frame().objects[0].clone();
        row.effects = None;
        assert_eq!(row, original);
        assert_eq!(rt.frame().objects.len(), 1);
        assert_eq!(rt.painter_order(), &[0]);
        assert_eq!(rt.effective_object_bounds(ObjectId::new(1)), bounds);
        assert_eq!(rt.frame_changes(), &FrameChanges::objects(vec![0]));
        assert!(rt.take_spatial_changes().is_empty());
        assert_eq!(
            rt.publication_context().execution_revision(),
            before.execution_revision().checked_next().unwrap()
        );
        assert_eq!(rt.last_patch_stats().full_group_rebuilds, 0);
        assert_eq!(rt.last_patch_stats().full_seeks, 0);
    }
}

#[test]
fn removal_retires_only_glow_tracks_and_keeps_motion_and_other_attachments() {
    let a = attachment(3);
    let mut tracks = channels(&a, 0.0, 1);
    tracks.push(motion());
    let mut compiled = scene(Some(a.clone()), &tracks);
    let stats = compiled
        .apply_execution_patch_with_stats(&change(Some(a.attachment), None))
        .unwrap();
    assert_eq!(compiled.track_count(), 1);
    assert_eq!(
        compiled.track(TrackId::new(100)).unwrap().values,
        motion().values
    );
    assert_eq!(stats.track_locators_removed, 3);
    assert_eq!(
        stats.dynamic_tracks_inspected, 0,
        "do not scan motion when retiring glow"
    );
    assert!(!compiled.objects()[0].dynamic.effect);
    assert!(compiled.objects()[0].dynamic.position);
    let mut rt = SceneInstance::new(scene(Some(a.clone()), &tracks));
    rt.advance_to(0.25).unwrap();
    let before = rt.frame().objects[0].transform;
    rt.take_frame_changes();
    rt.take_spatial_changes();
    rt.apply_execution_patch(&change(Some(a.attachment), None))
        .unwrap();
    assert_eq!(rt.last_patch_stats().track_locators_removed, 3);
    assert!(rt.last_patch_stats().scheduler_events_removed > 0);
    assert_eq!(rt.last_patch_stats().scheduler_events_inserted, 0);
    assert_eq!(rt.frame().objects[0].transform, before);
    assert!(rt.take_spatial_changes().is_empty());
    for time in [0.5, 1.0, 1.5, 2.0, 0.25] {
        rt.evaluate(time).unwrap();
        assert!(
            rt.frame().objects[0].effects.is_none(),
            "retired drivers cannot recreate a halo"
        );
        assert!((rt.frame().objects[0].transform.translation.x as f64 - time).abs() < 1e-6);
    }
}

#[test]
fn stale_attachment_changes_and_same_identity_schema_edits_fail_without_a_prefix() {
    let a = attachment(3);
    let b = attachment(4);
    let mut rt = SceneInstance::new(scene(Some(a.clone()), &channels(&a, 0.0, 1)));
    rt.begin_replay_retention(ReplayLimits::default()).unwrap();
    rt.advance_to(0.25).unwrap();
    rt.take_frame_changes();
    rt.take_spatial_changes();
    let frame = rt.frame().clone();
    let context = rt.publication_context();
    let mut schema = *a;
    schema.definition = EffectUpdate::default()
        .source(GlowSource::Silhouette)
        .apply_to(a.definition)
        .unwrap();
    for patch in [
        change(None, Some(b.clone())),
        change(Some(b.attachment), None),
        change(Some(a.attachment), Some(a.clone())),
        change(Some(a.attachment), Some(Arc::new(schema))),
    ] {
        assert_eq!(
            rt.apply_execution_patch(&patch).unwrap_err(),
            CompilePatchError::InvalidEffectAttachmentChange {
                object: ObjectId::new(1)
            }
        );
        let edits = tx([
            ExecutionPatch::SetStyle {
                fill_paint: None,
                object: ObjectId::new(1),
                style: Style {
                    fill: Some(Color::BLUE),
                    ..Style::default()
                },
            },
            patch,
        ]);
        assert!(rt.apply_execution_transaction(&edits).is_err());
        assert_eq!(rt.frame(), &frame);
        assert_eq!(rt.publication_context(), context);
        assert!(rt.frame_changes().is_empty());
        assert!(rt.take_spatial_changes().is_empty());
        assert_eq!(rt.replay_stats().revisions_retained, 0);
    }
}

#[test]
fn topology_barrier_keeps_value_writes_bound_to_their_own_generation() {
    let a = attachment(3);
    let b = attachment(4);
    let mut old_value = *a;
    old_value.definition = EffectUpdate::default()
        .intensity(1.0)
        .apply_to(a.definition)
        .unwrap();
    let mut new_value = *b;
    new_value.definition = EffectUpdate::default()
        .intensity(1.7)
        .apply_to(b.definition)
        .unwrap();
    let mut rt = SceneInstance::new(scene(Some(a.clone()), &[]));
    rt.apply_execution_transaction(&tx([
        ExecutionPatch::SetEffects {
            object: ObjectId::new(1),
            effects: stack(old_value),
        },
        change(Some(a.attachment), Some(b.clone())),
        ExecutionPatch::SetEffects {
            object: ObjectId::new(1),
            effects: stack(new_value),
        },
    ]))
    .unwrap();
    assert_eq!(
        rt.frame().objects[0]
            .effects
            .as_ref()
            .map(|stack| only(stack)),
        Some(new_value)
    );
    let before = rt.frame().clone();
    assert!(rt
        .apply_execution_transaction(&tx([
            change(Some(b.attachment), None),
            change(None, Some(attachment(5))),
            ExecutionPatch::SetEffects {
                object: ObjectId::new(1),
                effects: stack(*a)
            },
        ]))
        .is_err());
    assert_eq!(rt.frame(), &before);
}

#[test]
fn preflight_accounts_for_staged_drivers_and_rejects_old_generation_tracks() {
    let a = attachment(3);
    let b = attachment(4);
    let old = channels(&a, 0.0, 1);
    let new = channels(&b, 0.0, 1);
    let mut rt = SceneInstance::new(scene(None, &[]));
    let mut edits = vec![change(None, Some(a.clone()))];
    edits.extend(old.iter().cloned().map(ExecutionPatch::AddTrack));
    edits.push(change(Some(a.attachment), Some(b.clone())));
    // Track IDs are compiler input, not generated here: retirement releases only
    // the old channel's locators, so a valid incoming declaration is admissible.
    edits.extend(new.iter().cloned().map(ExecutionPatch::AddTrack));
    rt.apply_execution_transaction(&tx(edits)).unwrap();
    rt.advance_to(0.5).unwrap();
    assert_eq!(
        only(rt.frame().objects[0].effects.as_ref().unwrap()).attachment,
        b.attachment
    );
    assert!(
        (only(rt.frame().objects[0].effects.as_ref().unwrap())
            .definition
            .as_glow()
            .unwrap()
            .intensity()
            - 1.2)
            .abs()
            < 1e-12
    );
    let before = rt.frame().clone();
    let stale = tx([
        change(Some(b.attachment), Some(attachment(5))),
        ExecutionPatch::AddTrack(old[0].clone()),
    ]);
    assert!(rt.apply_execution_transaction(&stale).is_err());
    assert_eq!(rt.frame(), &before);
}

#[test]
fn replacing_one_attachment_does_not_retire_another_objects_moved_track() {
    let a = attachment(3);
    let b = attachment(4);
    let mut second = source(Some(b.clone()));
    second.id = ObjectId::new(2);
    let old = channels(&a, 0.0, 1)[2].clone();
    let mut moved = channels(&b, 0.0, 1)[2].clone();
    moved.object = second.id;
    let mut rt = SceneInstance::new(
        CompiledScene::compile_objects(vec![source(Some(a.clone())), second], &[old]).unwrap(),
    );
    rt.apply_execution_transaction(&tx([
        ExecutionPatch::ReplaceTrack(moved),
        change(Some(a.attachment), None),
    ]))
    .unwrap();
    rt.advance_to(0.5).unwrap();
    assert!(rt.frame().objects[0].effects.is_none());
    assert!(
        (only(rt.frame().objects[1].effects.as_ref().unwrap())
            .definition
            .as_glow()
            .unwrap()
            .intensity()
            - 1.2)
            .abs()
            < 1e-12
    );
}

#[test]
fn replay_restores_attachment_generations_and_their_exact_retired_drivers() {
    let a = attachment(3);
    let b = attachment(4);
    let mut tracks = channels(&a, 0.0, 1);
    tracks.push(motion());
    let mut rt = SceneInstance::new(scene(Some(a.clone()), &tracks));
    rt.begin_replay_retention(ReplayLimits::default()).unwrap();
    rt.advance_to(0.25).unwrap();
    let first = rt.frame().clone();
    rt.advance_to(0.4).unwrap();
    rt.apply_execution_patch(&change(Some(a.attachment), None))
        .unwrap();
    rt.advance_to(0.5).unwrap();
    let removed = rt.frame().clone();
    rt.advance_to(0.7).unwrap();
    rt.apply_execution_patch(&change(None, Some(b.clone())))
        .unwrap();
    for track in channels(&b, 0.7, 10) {
        rt.apply_execution_patch(&ExecutionPatch::AddTrack(track))
            .unwrap();
    }
    rt.advance_to(1.2).unwrap();
    let reattached = rt.frame().clone();
    rt.advance_to(2.0).unwrap();
    rt.seal_replay().unwrap();
    assert!(rt.replay_retention_valid());
    let retained = rt.replay_stats().payloads_retained;
    for _ in 0..3 {
        for checkpoint in [&first, &removed, &reattached] {
            rt.seek(checkpoint.time).unwrap();
            assert_eq!(rt.frame(), checkpoint);
        }
        rt.seek(0.25).unwrap();
        rt.advance_to(0.5).unwrap();
        assert_eq!(rt.frame(), &removed);
        rt.advance_to(1.2).unwrap();
        assert_eq!(rt.frame(), &reattached);
        assert_eq!(rt.replay_stats().payloads_retained, retained);
    }
}

#[test]
fn topology_replay_budget_is_bounded_by_the_retired_channels_not_the_scene() {
    let a = attachment(3);
    let mut tracks = channels(&a, 0.0, 1);
    tracks.push(motion());
    let mut objects = vec![source(Some(a.clone()))];
    for id in 2..=4097 {
        let mut row = source(None);
        row.id = ObjectId::new(id);
        objects.push(row);
    }
    let mut compiled = CompiledScene::compile_objects(objects, &tracks).unwrap();
    let patch = change(Some(a.attachment), None);
    let inverse = compiled.prepare_replay_revision(&patch).unwrap();
    assert_eq!(
        inverse.retention_cost(),
        7,
        "one row, three channels, three tracks"
    );
    let stats = compiled
        .preflight_execution_transaction(&tx([patch.clone()]))
        .unwrap();
    assert!(stats.track_metadata_visits <= 6);
    let mut rt = SceneInstance::new(compiled.clone());
    rt.take_frame_changes();
    rt.take_spatial_changes();
    let tail = rt.frame().objects[1..].to_vec();
    rt.apply_execution_patch(&patch).unwrap();
    assert_eq!(&rt.frame().objects[1..], tail.as_slice());
    assert_eq!(rt.frame_changes(), &FrameChanges::objects(vec![0]));
    assert!(rt.take_spatial_changes().is_empty());
    assert_eq!(rt.last_patch_stats().objects_recomputed, 0);
    let stats = compiled.apply_execution_patch_with_stats(&patch).unwrap();
    assert_eq!(stats.dynamic_tracks_inspected, 0);
    assert_eq!(stats.unrelated_track_slots_shifted, 0);
    assert_eq!(
        compiled
            .channel_tracks(CompiledChannelKey::new(0, Property::Position))
            .len(),
        1
    );
}

#[test]
fn absent_to_absent_is_exactly_idle_but_a_stale_removal_is_not_a_noop() {
    let mut rt = SceneInstance::new(scene(None, &[]));
    rt.take_frame_changes();
    rt.take_spatial_changes();
    let context = rt.publication_context();
    rt.apply_execution_patch(&change(None, None)).unwrap();
    assert_eq!(rt.publication_context(), context);
    assert!(rt.frame_changes().is_empty());
    assert!(rt.take_spatial_changes().is_empty());
    assert!(rt
        .apply_execution_patch(&change(Some(attachment(3).attachment), None))
        .is_err());
    assert_eq!(rt.publication_context(), context);
}
