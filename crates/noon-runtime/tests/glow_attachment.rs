//! Structural attachment projection, not public Scene or browser qualification.
use std::sync::Arc;

use noon_compile::{
    CompilePatchError, CompiledChannelKey, CompiledGlow, CompiledObject, CompiledResources,
    CompiledScene, ExecutionMutationTransaction, ExecutionPatch,
};
use noon_core::{
    Color, GeometryRef, Glow, GlowSource, GlowUpdate, ObjectId, Pixels, Property, RateFunction,
    SemanticNodeId, Style, TrackDefinition, TrackId, TrackTiming, TrackValues, Transform2D, Vec2,
};
use noon_runtime::{FrameChanges, ReplayLimits, SceneInstance};

fn attachment(generation: u32) -> Arc<CompiledGlow> {
    Arc::new(CompiledGlow {
        attachment: SemanticNodeId::new(7, generation),
        definition: Glow::new(
            GlowUpdate::default()
                .radius(Pixels(3.25))
                .color(Color::RED)
                .intensity(0.4),
        )
        .unwrap(),
    })
}
fn source(glow: Option<Arc<CompiledGlow>>) -> CompiledObject {
    let mut row = CompiledObject::new(
        ObjectId::new(1),
        GeometryRef::circle(0.4),
        Transform2D::IDENTITY,
        Style::default(),
    );
    row.glow = glow;
    row
}
fn change(expected: Option<SemanticNodeId>, glow: Option<Arc<CompiledGlow>>) -> ExecutionPatch {
    ExecutionPatch::SetGlowAttachment {
        object: ObjectId::new(1),
        expected,
        glow,
    }
}
fn channels(glow: &CompiledGlow, start: f64, offset: u64) -> Vec<TrackDefinition> {
    glow.parameter_channels(
        GlowUpdate::default()
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
fn scene(glow: Option<Arc<CompiledGlow>>, tracks: &[TrackDefinition]) -> CompiledScene {
    CompiledScene::compile_objects(vec![source(glow)], tracks).unwrap()
}
fn tx(patches: impl IntoIterator<Item = ExecutionPatch>) -> ExecutionMutationTransaction {
    ExecutionMutationTransaction::from_mutations(patches)
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
        assert_eq!(rt.frame().objects[0].glow, next);
        let mut row = rt.frame().objects[0].clone();
        row.glow = None;
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
    assert!(!compiled.objects()[0].dynamic.glow);
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
            rt.frame().objects[0].glow.is_none(),
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
    schema.definition = GlowUpdate::default()
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
            CompilePatchError::InvalidGlowAttachmentChange {
                object: ObjectId::new(1)
            }
        );
        let edits = tx([
            ExecutionPatch::SetStyle {
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
    old_value.definition = GlowUpdate::default()
        .intensity(1.0)
        .apply_to(a.definition)
        .unwrap();
    let mut new_value = *b;
    new_value.definition = GlowUpdate::default()
        .intensity(1.7)
        .apply_to(b.definition)
        .unwrap();
    let mut rt = SceneInstance::new(scene(Some(a.clone()), &[]));
    rt.apply_execution_transaction(&tx([
        ExecutionPatch::SetGlow {
            object: ObjectId::new(1),
            glow: Arc::new(old_value),
        },
        change(Some(a.attachment), Some(b.clone())),
        ExecutionPatch::SetGlow {
            object: ObjectId::new(1),
            glow: Arc::new(new_value),
        },
    ]))
    .unwrap();
    assert_eq!(rt.frame().objects[0].glow.as_deref(), Some(&new_value));
    let before = rt.frame().clone();
    assert!(rt
        .apply_execution_transaction(&tx([
            change(Some(b.attachment), None),
            change(None, Some(attachment(5))),
            ExecutionPatch::SetGlow {
                object: ObjectId::new(1),
                glow: a
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
        rt.frame().objects[0].glow.as_ref().unwrap().attachment,
        b.attachment
    );
    assert!(
        (rt.frame().objects[0]
            .glow
            .as_ref()
            .unwrap()
            .definition
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
    assert!(rt.frame().objects[0].glow.is_none());
    assert!(
        (rt.frame().objects[1]
            .glow
            .as_ref()
            .unwrap()
            .definition
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
