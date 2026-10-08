//! Actual public target declarations lowered by the same compiled activation
//! functions used by ExecutionSession. Host bootstrap remains separately gated;
//! no test-only session, renderer, clock or serialization path is introduced.
use super::*;
use crate::{effects::Pixels, Color, DeclaredAnimation};
use noon_compile::{
    lower_prepared_semantic_animation_composition, lower_semantic_affine_animation_tracks,
    lower_semantic_animation_schedule, CompiledScene, EffectiveAnimationProperties,
    ExecutionMutationTransaction, ExecutionPatch, SemanticAnimationCompletion,
    SemanticExecutionIndex, SemanticExecutionReachability,
};
use noon_core::{
    AnimationOptions, ObjectId, Property, RateFunction, SemanticAnimationCompositionKind,
    SemanticAnimationIntent, SemanticNodeCreation, TrackId, TrackValues,
};
use noon_runtime::SceneInstance;

fn fixture() -> (Scene, Mobject) {
    let mut scene = Scene::new();
    let mut source = scene.circle(0.4).unwrap();
    source.disable_stroke().unwrap();
    source.set_fill(1.0, 1.0, 1.0, 1.0).unwrap();
    scene.add(&source).unwrap();
    scene
        .set_glow(
            &source,
            GlowUpdate::default()
                .radius(Pixels(3.25))
                .color(Color::RED)
                .intensity(0.4000000000000123),
        )
        .unwrap();
    (scene, source)
}
fn options() -> AnimationOptions {
    AnimationOptions::new()
        .run_time(1.0)
        .rate_func(RateFunction::Linear)
}
fn runtime(scene: &Scene) -> (SceneInstance, SemanticExecutionIndex) {
    let mut index = SemanticExecutionIndex::new();
    let store = scene.integration_store().borrow();
    let projection = index.lower_root(&store, scene.root()).unwrap();
    (
        SceneInstance::new(CompiledScene::from_semantic_projection(&projection).unwrap()),
        index,
    )
}
fn effective(runtime: &SceneInstance, object: ObjectId) -> EffectiveAnimationProperties {
    let frame = runtime.frame();
    let index = runtime.frame_index_for_object(object).unwrap();
    let row = &frame.objects[index];
    EffectiveAnimationProperties {
        glow: row.glow.as_deref().copied(),
        z_index: row.z_index,
        transform: row.transform,
        style: row.style,
        appearance: row.appearance,
        reveal: frame.reveal(index),
        world_transform: row.world_transform(),
        camera_profile: row.camera_profile(),
    }
}
fn lower(
    scene: &Scene,
    runtime: &SceneInstance,
    index: &SemanticExecutionIndex,
    declaration: &DeclaredAnimation,
) -> Result<
    noon_compile::SemanticAffineAnimationTrackProjection,
    noon_compile::SemanticAffineAnimationTrackError,
> {
    let store = scene.integration_store().borrow();
    let schedule = lower_semantic_animation_schedule(
        &store,
        index,
        declaration.node_id(),
        runtime.frame().time,
        AnimationOptions::new(),
    )
    .unwrap();
    lower_semantic_affine_animation_tracks(&store, &schedule, |id| Some(effective(runtime, id)))
}
fn glow(runtime: &SceneInstance, object: ObjectId) -> Glow {
    effective(runtime, object).glow.unwrap().definition
}

#[test]
fn declaration_activates_motion_and_glow_from_exact_effective_values() {
    let (scene, source) = fixture();
    let original = source.get_effect("glow").unwrap().node_id();
    let mut target = source.target_editor().unwrap();
    target.shift(2.0, 0.0).unwrap();
    target
        .set_glow(
            GlowUpdate::default()
                .intensity(1.4)
                .radius(Pixels(6.5))
                .color(Color::BLUE),
        )
        .unwrap();
    let declaration = scene
        .declare_transform_to(&source, &target, options())
        .unwrap();
    let (mut runtime, index) = runtime(&scene);
    let object = index.execution_object_id(source.node_id()).unwrap();
    let before = scene.revision();
    let projection = lower(&scene, &runtime, &index, &declaration).unwrap();
    assert_eq!(projection.tracks().len(), 4);
    let patches = projection
        .tracks()
        .iter()
        .enumerate()
        .map(|(i, t)| {
            if let TrackValues::Glow { attachment, .. } = t.values {
                assert_eq!(attachment, original);
            }
            ExecutionPatch::AddTrack(t.with_track_id(TrackId::new(i as u64 + 1)).unwrap())
        })
        .collect::<Vec<_>>();
    runtime
        .apply_execution_transaction(&ExecutionMutationTransaction::from_mutations(patches))
        .unwrap();
    runtime.evaluate(0.5).unwrap();
    let actual = glow(&runtime, object);
    assert_eq!(
        actual.intensity(),
        0.4000000000000123 + (1.4 - 0.4000000000000123) * 0.5
    );
    assert_eq!(actual.radius(), Pixels(4.875).into());
    assert_eq!(effective(&runtime, object).transform.translation.x, 1.0);
    let midpoint = effective(&runtime, object);
    runtime.seek(1.0).unwrap();
    assert_eq!(glow(&runtime, object).intensity(), 1.4);
    runtime.seek(0.5).unwrap();
    assert_eq!(effective(&runtime, object), midpoint);
    assert_eq!(scene.revision(), before);
    assert!(
        scene.execution_session().is_err(),
        "host orchestration is not admitted by a lowerer test"
    );
}

#[test]
fn disjoint_glow_targets_share_ordinary_composition_and_conflicting_parameters_reject() {
    let (scene, source) = fixture();
    let mut intensity = source.target_editor().unwrap();
    intensity
        .set_glow(GlowUpdate::default().intensity(1.4))
        .unwrap();
    let mut color = source.target_editor().unwrap();
    color
        .set_glow(GlowUpdate::default().color(Color::BLUE))
        .unwrap();
    let a = scene
        .declare_transform_to(&source, &intensity, options())
        .unwrap();
    let b = scene
        .declare_transform_to(&source, &color, options())
        .unwrap();
    let both = scene
        .declare_animation(
            SemanticAnimationIntent::Composition {
                kind: SemanticAnimationCompositionKind::Parallel,
                children: vec![a.node_id(), b.node_id()],
            },
            AnimationOptions::new(),
        )
        .unwrap();
    let (runtime, index) = runtime(&scene);
    let projection = lower(&scene, &runtime, &index, &both).unwrap();
    assert_eq!(
        projection
            .tracks()
            .iter()
            .map(|t| t.property)
            .collect::<Vec<_>>(),
        [Property::GlowIntensity, Property::GlowColor]
    );
    let duplicate = scene
        .declare_transform_to(&source, &intensity, options())
        .unwrap();
    let conflict = scene
        .declare_animation(
            SemanticAnimationIntent::Composition {
                kind: SemanticAnimationCompositionKind::Parallel,
                children: vec![a.node_id(), duplicate.node_id()],
            },
            AnimationOptions::new(),
        )
        .unwrap();
    let before = scene.revision();
    assert!(matches!(
        lower(&scene, &runtime, &index, &conflict),
        Err(
            noon_compile::SemanticAffineAnimationTrackError::GlowDriverConflict {
                property: Property::GlowIntensity,
                ..
            }
        )
    ));
    assert_eq!(scene.revision(), before);
}

#[test]
fn stale_source_or_target_generation_never_rebinds_by_name() {
    for replace_target in [false, true] {
        let (scene, source) = fixture();
        let mut target = source.target_editor().unwrap();
        target
            .set_glow(GlowUpdate::default().intensity(1.4))
            .unwrap();
        let declaration = scene
            .declare_transform_to(&source, &target, options())
            .unwrap();
        let receiver = if replace_target {
            &mut target
        } else {
            &mut source.clone()
        };
        let old = receiver.get_effect("glow").unwrap().node_id();
        receiver.remove_glow().unwrap();
        receiver
            .set_glow(GlowUpdate::default().intensity(1.4))
            .unwrap();
        assert_ne!(old, receiver.get_effect("glow").unwrap().node_id());
        let (runtime, index) = runtime(&scene);
        assert!(matches!(
            lower(&scene, &runtime, &index, &declaration),
            Err(
                noon_compile::SemanticAffineAnimationTrackError::GlowTarget {
                    error: noon_compile::GlowTargetLoweringError::StaleAttachment,
                    ..
                }
            )
        ));
    }
}

#[test]
fn prepared_sequence_captures_prior_parameter_endpoint_and_can_return_to_original() {
    let (scene, source) = fixture();
    let mut first = source.target_editor().unwrap();
    first
        .set_glow(GlowUpdate::default().intensity(1.4))
        .unwrap();
    let original = source.target_editor().unwrap();
    let a = scene
        .declare_transform_to(&source, &first, options())
        .unwrap();
    let b = scene
        .declare_transform_to(&source, &original, options())
        .unwrap();
    let root = scene
        .declare_animation(
            SemanticAnimationIntent::Composition {
                kind: SemanticAnimationCompositionKind::Sequence,
                children: vec![a.node_id(), b.node_id()],
            },
            AnimationOptions::new(),
        )
        .unwrap();
    let (mut runtime, index) = runtime(&scene);
    let object = index.execution_object_id(source.node_id()).unwrap();
    let mut store = scene.integration_store().borrow_mut();
    let before = store.scene_revision();
    let prepared = SemanticMutationTransaction::new()
        .prepare(&mut store)
        .unwrap();
    let plan = lower_prepared_semantic_animation_composition(
        &prepared,
        &index,
        root.node_id(),
        0.0,
        AnimationOptions::new(),
        |id| Some(effective(&runtime, id)),
    )
    .unwrap();
    assert_eq!(plan.tracks().len(), 2);
    assert!(matches!(plan.tracks()[1].values,TrackValues::Glow{
        from:noon_core::GlowTrackValue::Intensity(1.4),to:noon_core::GlowTrackValue::Intensity(v),..
    } if v == 0.4000000000000123));
    let definitions = plan
        .tracks()
        .iter()
        .enumerate()
        .map(|(i, t)| {
            ExecutionPatch::AddTrack(t.with_track_id(TrackId::new(i as u64 + 1)).unwrap())
        })
        .collect::<Vec<_>>();
    drop(prepared);
    assert_eq!(store.scene_revision(), before);
    runtime
        .apply_execution_transaction(&ExecutionMutationTransaction::from_mutations(definitions))
        .unwrap();
    runtime.seek(1.5).unwrap();
    assert_eq!(
        glow(&runtime, object).intensity(),
        1.4 + (0.4000000000000123 - 1.4) * 0.5
    );
}

#[test]
fn returning_and_reversed_completion_persist_the_actual_f64_endpoint() {
    for (rate, reverse, expected) in [
        (RateFunction::ThereAndBack, false, 0.4000000000000123),
        (RateFunction::Linear, true, 0.4000000000000123),
        (RateFunction::ThereAndBack, true, 0.4000000000000123),
    ] {
        let (scene, source) = fixture();
        let mut target = source.target_editor().unwrap();
        target
            .set_glow(GlowUpdate::default().intensity(1.4))
            .unwrap();
        let opts = options().rate_func(rate).reverse_rate_function(reverse);
        let declaration = scene.declare_transform_to(&source, &target, opts).unwrap();
        let (runtime, index) = runtime(&scene);
        let plan = lower(&scene, &runtime, &index, &declaration).unwrap();
        assert_eq!(plan.tracks().len(), 1);
        assert!(
            matches!(plan.tracks()[0].completion,SemanticAnimationCompletion::Glow {
            value:noon_core::GlowTrackValue::Intensity(v),..
        } if v==expected)
        );
    }
}

#[test]
fn pending_effect_target_is_inert_and_prepared_identity_matches_its_commit() {
    let (scene, source) = fixture();
    let (runtime, index) = runtime(&scene);
    let mut store = scene.integration_store().borrow_mut();
    let before = store.scene_revision();
    let count = store.len();
    let reachability = SemanticExecutionReachability::from_root(&store, scene.root()).unwrap();
    let state = store
        .semantic_object_state_checked(source.node_id())
        .unwrap()
        .clone();
    let mut tx = SemanticMutationTransaction::new();
    let target = tx.create_node(SemanticNodeCreation::object(state));
    let animation = tx.create_transform_animation(source.node_id(), target, options());
    // Deliberately add the effect after the animation: final preflight meaning,
    // not intermediate commit ordering, determines the frozen correspondence.
    let effect = tx.create_effect(
        target,
        "glow",
        Glow::new(
            GlowUpdate::default()
                .radius(Pixels(3.25))
                .color(Color::RED)
                .intensity(1.4),
        )
        .unwrap(),
    );
    let prepared = tx.prepare(&mut store).unwrap();
    let planned_effect = prepared.planned_node_id(effect).unwrap();
    assert_eq!(
        prepared
            .transform_effect_snapshot(animation)
            .unwrap()
            .unwrap()
            .target
            .as_ref(),
        [planned_effect]
    );
    assert_eq!(prepared.store().len(), count);
    let activation = lower_prepared_semantic_animation_composition(
        &prepared,
        &index,
        animation,
        0.0,
        AnimationOptions::new(),
        |id| Some(effective(&runtime, id)),
    )
    .unwrap();
    assert_eq!(activation.tracks().len(), 1);
    let publication =
        noon_compile::prepare_semantic_publication(&prepared, &index, &reachability).unwrap();
    assert_eq!(publication.possible_entry_count(), 0);
    assert!(publication.value_transaction().mutations().is_empty());
    assert_eq!(prepared.store().scene_revision(), before);
    let result = prepared.commit();
    assert_eq!(result.resolve(effect), Some(planned_effect));
    assert_eq!(
        store
            .semantic_animation_state(result.resolve(animation).unwrap())
            .unwrap()
            .transform_effect_snapshot()
            .unwrap()
            .target
            .as_ref(),
        [planned_effect]
    );
    assert_eq!(
        glow(
            &runtime,
            index.execution_object_id(source.node_id()).unwrap()
        )
        .intensity(),
        0.4000000000000123
    );
}

#[test]
fn effect_target_enrollment_and_unsupported_schema_reject_without_publication() {
    for discrete in [0, 1, 2] {
        let (scene, source) = fixture();
        let mut target = source.target_editor().unwrap();
        match discrete {
            0 => target
                .set_glow(GlowUpdate::default().source(noon_core::GlowSource::Silhouette))
                .unwrap(),
            1 => target.set_glow(GlowUpdate::default().radius(0.2)).unwrap(),
            _ => target
                .add_effect(Glow::default(), "second")
                .map(|_| ())
                .unwrap(),
        };
        let declaration = scene
            .declare_transform_to(&source, &target, options())
            .unwrap();
        let (runtime, index) = runtime(&scene);
        let before = scene.revision();
        assert!(lower(&scene, &runtime, &index, &declaration).is_err());
        assert_eq!(scene.revision(), before);
    }
    let (scene, source) = fixture();
    let (_, index) = runtime(&scene);
    let mut store = scene.integration_store().borrow_mut();
    let before = store.scene_revision();
    let reachability = SemanticExecutionReachability::from_root(&store, scene.root()).unwrap();
    let mut tx = SemanticMutationTransaction::new();
    let target = tx.create_node(SemanticNodeCreation::object(
        store
            .semantic_object_state_checked(source.node_id())
            .unwrap()
            .clone(),
    ));
    tx.create_effect(target, "glow", Glow::default());
    tx.add_member(scene.root(), target);
    let prepared = tx.prepare(&mut store).unwrap();
    assert!(noon_compile::prepare_semantic_publication(&prepared, &index, &reachability).is_err());
    drop(prepared);
    assert_eq!(store.scene_revision(), before);
}

#[test]
fn host_guard_precedes_profile_projection_even_for_unsupported_stacks() {
    let (mut scene, source) = fixture();
    scene
        .add_effect(&source, Glow::default(), "second")
        .unwrap();
    let mut index = SemanticExecutionIndex::new();
    let store = scene.integration_store().borrow();
    let error =
        noon_compile::lower_semantic_execution_root(&store, scene.root(), &mut index).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("effect declarations cannot execute"),
        "{error}"
    );
    assert!(index.is_empty());
    assert!(index
        .lower_root(&store, scene.root())
        .unwrap_err()
        .to_string()
        .contains("requires one attachment"));
    assert!(index.is_empty());
}
