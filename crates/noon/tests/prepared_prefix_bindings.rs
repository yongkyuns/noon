#![cfg(feature = "prepared-prefix")]
//! Authored click declarations travel through the same retained object history as
//! the displayed scene. No producer-current binding map or supplied action options
//! are available to the reader when these historical actions are invoked.

mod raster_support;

use noon::integration::{NativeInputModifiers, NativePointerInputKind, NativePointerPosition};
use noon::{
    AnimationOptions, ExecutionSession, IndicateOptions, Mobject, PrefixPublisher, PrefixReader,
    Scene,
};
use noon_core::{
    Color, NativePointerId, NativePointerInput, ObjectId, SemanticClickIndicate,
    SemanticMutationTransaction, Vec2,
};
use noon_runtime::{FrameState, ReplayLimits};

const A: SemanticClickIndicate = SemanticClickIndicate::new(1.25, Color::YELLOW, 0.5);
const B: SemanticClickIndicate = SemanticClickIndicate::new(1.75, Color::RED, 1.0);

fn options(binding: SemanticClickIndicate) -> (IndicateOptions, AnimationOptions) {
    (
        IndicateOptions::new(binding.scale_factor(), binding.color()),
        AnimationOptions::new().run_time(binding.run_time()),
    )
}

fn recorded_scene(
    binding: Option<SemanticClickIndicate>,
) -> (Scene, Mobject, ExecutionSession, ObjectId) {
    let mut scene = Scene::new();
    let mut target = scene.circle(0.65).unwrap();
    target.set_fill(0.15, 0.4, 0.9, 1.0).unwrap();
    target.shift(-1.0, 0.0).unwrap();
    scene.add(&target).unwrap();
    if let Some(binding) = binding {
        let (indication, animation) = options(binding);
        scene
            .on_click_indicate(&target, indication, animation)
            .unwrap();
    }
    let mut source = scene.execution_session().unwrap();
    let object = source.execution_object_id(target.node_id()).unwrap();
    source
        .begin_replay_retention(ReplayLimits {
            revisions: 128,
            payloads: 2048,
        })
        .unwrap();
    (scene, target, source, object)
}

fn fixture(
    binding: Option<SemanticClickIndicate>,
) -> (
    Scene,
    Mobject,
    ExecutionSession,
    ObjectId,
    PrefixPublisher,
    PrefixReader,
) {
    let (scene, target, mut source, object) = recorded_scene(binding);
    let boundary = source.wait_segment(1.0).unwrap();
    source.advance_to(boundary.end_time()).unwrap();
    let (publisher, mut reader) = source.create_prepared_prefix(boundary).unwrap();
    reader.seek(0.5).unwrap();
    (scene, target, source, object, publisher, reader)
}

fn bind(
    scene: &Scene,
    source: &mut ExecutionSession,
    target: &Mobject,
    binding: Option<SemanticClickIndicate>,
) {
    if let Some(binding) = binding {
        let (indication, animation) = options(binding);
        scene
            .live(source)
            .on_click_indicate(target, indication, animation)
            .unwrap();
    } else {
        // The facade has no clear convenience method. Use the public semantic
        // transaction boundary, never mutate the store behind its live session.
        let mut transaction = SemanticMutationTransaction::new();
        transaction.set_click_indicate(target.node_id(), None);
        source
            .apply_semantic_transaction(&mut scene.integration_store().borrow_mut(), transaction)
            .unwrap();
    }
}

fn append_interval(
    source: &mut ExecutionSession,
    publisher: &mut PrefixPublisher,
    reader: &mut PrefixReader,
) -> noon_runtime::PrefixBatchStats {
    let boundary = source.wait_segment(1.0).unwrap();
    source.advance_to(boundary.end_time()).unwrap();
    let batch = source.publish_prepared_prefix(boundary, publisher).unwrap();
    reader.append(batch).unwrap()
}

fn midpoint(frame: &FrameState, object: ObjectId, binding: SemanticClickIndicate) {
    let row = frame.objects.iter().find(|row| row.id == object).unwrap();
    assert_eq!(
        row.transform.scale,
        Vec2::new(binding.scale_factor() as f32, binding.scale_factor() as f32)
    );
    assert_eq!(row.style.fill, Some(binding.color()));
    assert_eq!(row.transform.translation, Vec2::new(-1.0, 0.0));
}

fn exercise(
    reader: &mut PrefixReader,
    object: ObjectId,
    expected: Option<SemanticClickIndicate>,
    wall_time: f64,
) {
    let baseline = reader.frame().clone();
    let history = reader.replay_stats();
    let context = reader.take_renderer_publication().context();
    let prepared = reader.prepare_bound_click_indicate(object).unwrap();
    match expected {
        None => assert!(prepared.is_none(), "this historical interval is unbound"),
        Some(binding) => {
            reader.start_transient_animation(prepared.unwrap()).unwrap();
            reader.advance_interactions(wall_time).unwrap();
            reader
                .advance_interactions(wall_time + binding.run_time() * 0.5)
                .unwrap();
            midpoint(reader.frame(), object, binding);
            assert_eq!(reader.frame().time, baseline.time);
            assert!(reader.interactions_active());
            reader
                .advance_interactions(wall_time + binding.run_time())
                .unwrap();
        }
    }
    assert!(!reader.interactions_active());
    assert_eq!(reader.frame(), &baseline);
    assert_eq!(reader.replay_stats(), history);
    let restored = reader.take_renderer_publication().context();
    assert_eq!(restored.scene_revision(), context.scene_revision());
    assert_eq!(restored.execution_revision(), context.execution_revision());
}

#[test]
fn metadata_only_binding_intervals_survive_appends_rewinds_and_source_destruction() {
    let (scene, target, mut source, object, mut publisher, mut reader) = fixture(None);
    let initial = reader.frame().clone();
    exercise(&mut reader, object, None, 0.0);
    for binding in [Some(A), Some(B), None] {
        let source_frame = source.frame().clone();
        bind(&scene, &mut source, &target, binding);
        assert_eq!(
            source.frame(),
            &source_frame,
            "authoring a click declaration must not animate or restyle the target"
        );
        let stats = append_interval(&mut source, &mut publisher, &mut reader);
        assert_eq!(
            stats.row_payloads, 1,
            "one changed binding is one retained row"
        );
        assert_eq!(
            reader.frame(),
            &initial,
            "append must not leak future bindings"
        );
        exercise(&mut reader, object, None, 0.0);
    }
    assert_eq!(target.state().unwrap().click_indicate(), None);
    assert_eq!(reader.frontier(), 4.0);
    let retained = reader.replay_stats();
    drop(target);
    drop(source);
    drop(scene);
    drop(publisher);

    for (index, (time, binding)) in [
        (2.5, Some(B)),
        (1.5, Some(A)),
        (3.5, None),
        (0.5, None),
        (1.0, Some(A)),
        (2.0, Some(B)),
        (3.0, None),
        (1.5, Some(A)),
    ]
    .into_iter()
    .enumerate()
    {
        reader.seek(time).unwrap();
        assert_eq!(reader.frame().objects, initial.objects);
        exercise(&mut reader, object, binding, 10.0 * index as f64);
        assert_eq!(
            reader.replay_stats().revisions_retained,
            retained.revisions_retained
        );
        assert_eq!(
            reader.replay_stats().payloads_retained,
            retained.payloads_retained
        );
    }
}

#[test]
fn appended_binding_changes_preserve_active_effect_until_displayed_revision_changes() {
    let (scene, target, mut source, object, mut publisher, mut reader) = fixture(Some(A));
    let mut baseline = reader.frame().clone();
    let prepared = reader
        .prepare_bound_click_indicate(object)
        .unwrap()
        .unwrap();
    reader.start_transient_animation(prepared).unwrap();
    reader.advance_interactions(100.0).unwrap();
    reader.advance_interactions(100.125).unwrap();
    let active = reader.frame().clone();
    let context = reader.take_renderer_publication().context();

    bind(&scene, &mut source, &target, Some(B));
    append_interval(&mut source, &mut publisher, &mut reader);
    assert_eq!(reader.frame(), &active);
    assert_eq!(reader.take_renderer_publication().context(), context);
    assert!(reader.interactions_active());
    assert!(reader
        .prepare_bound_click_indicate(object)
        .unwrap()
        .is_none());
    reader.advance_to(0.75).unwrap();
    reader.advance_interactions(100.25).unwrap();
    midpoint(reader.frame(), object, A);
    reader.advance_interactions(100.5).unwrap();
    assert!(
        !reader.interactions_active(),
        "the old duration still owns completion"
    );
    baseline.time = 0.75;
    assert_eq!(reader.frame(), &baseline);

    let stale = reader
        .prepare_bound_click_indicate(object)
        .unwrap()
        .unwrap();
    reader.start_transient_animation(stale.clone()).unwrap();
    reader.advance_interactions(110.0).unwrap();
    reader.advance_interactions(110.25).unwrap();
    reader.advance_to(1.5).unwrap();
    reader.advance_interactions(110.375).unwrap();
    assert!(!reader.interactions_active());
    baseline.time = 1.5;
    assert_eq!(reader.frame(), &baseline);
    let context = reader.take_renderer_publication().context();
    assert!(reader.start_transient_animation(stale).is_err());
    assert_eq!(reader.frame(), &baseline);
    assert_eq!(reader.take_renderer_publication().context(), context);
    exercise(&mut reader, object, Some(B), 120.0);

    // Explicit rewind cancels B and resolves A again, despite the producer being
    // newer. A prepared action also survives delivery of another future batch.
    let prepared = reader
        .prepare_bound_click_indicate(object)
        .unwrap()
        .unwrap();
    reader.start_transient_animation(prepared).unwrap();
    reader.advance_interactions(130.0).unwrap();
    reader.advance_interactions(130.5).unwrap();
    midpoint(reader.frame(), object, B);
    reader.seek(0.5).unwrap();
    assert!(!reader.interactions_active());
    baseline.time = 0.5;
    assert_eq!(reader.frame(), &baseline);
    let prepared = reader
        .prepare_bound_click_indicate(object)
        .unwrap()
        .unwrap();
    bind(&scene, &mut source, &target, None);
    append_interval(&mut source, &mut publisher, &mut reader);
    reader.start_transient_animation(prepared).unwrap();
    reader.advance_interactions(140.0).unwrap();
    reader.advance_interactions(140.25).unwrap();
    midpoint(reader.frame(), object, A);
    reader.advance_interactions(140.5).unwrap();
    assert_eq!(reader.frame(), &baseline);
    reader.advance_to(2.5).unwrap();
    exercise(&mut reader, object, None, 150.0);
}

#[test]
fn detached_binding_update_is_admitted_only_on_reentry_and_old_history_outlives_deletion() {
    let (scene, target, mut source, object, mut publisher, mut reader) = fixture(Some(A));
    scene.live(&mut source).remove(&target).unwrap();
    append_interval(&mut source, &mut publisher, &mut reader);
    bind(&scene, &mut source, &target, Some(B));
    assert_eq!(
        append_interval(&mut source, &mut publisher, &mut reader).row_payloads,
        0,
        "detached authoring does not create a displayed execution row"
    );
    scene.live(&mut source).add(&target).unwrap();
    let reentry = append_interval(&mut source, &mut publisher, &mut reader);
    assert_eq!(
        reentry.new_object_slots, 0,
        "reentry preserves execution identity"
    );

    let mut deletion = SemanticMutationTransaction::new();
    deletion.remove_node(target.node_id());
    source
        .apply_semantic_transaction(&mut scene.integration_store().borrow_mut(), deletion)
        .unwrap();
    assert_eq!(source.execution_object_id(target.node_id()), None);
    append_interval(&mut source, &mut publisher, &mut reader);
    drop(target);
    drop(source);
    drop(scene);
    drop(publisher);

    for (index, (time, binding)) in [
        (3.5, Some(B)),
        (0.5, Some(A)),
        (1.5, None),
        (2.5, None),
        (4.5, None),
        (3.5, Some(B)),
        (0.5, Some(A)),
    ]
    .into_iter()
    .enumerate()
    {
        reader.seek(time).unwrap();
        if binding.is_some() {
            exercise(&mut reader, object, binding, 10.0 * index as f64);
        } else {
            let frame = reader.frame().clone();
            let context = reader.take_renderer_publication().context();
            assert!(reader.prepare_bound_click_indicate(object).is_err());
            assert_eq!(reader.frame(), &frame);
            assert_eq!(reader.take_renderer_publication().context(), context);
            assert!(!reader.interactions_active());
        }
    }
    let frame = reader.frame().clone();
    let context = reader.take_renderer_publication().context();
    assert!(reader
        .prepare_bound_click_indicate(ObjectId::new(u64::MAX))
        .is_err());
    assert_eq!(reader.frame(), &frame);
    assert_eq!(reader.take_renderer_publication().context(), context);
    assert!(!reader.interactions_active());
}

fn click(session: &mut ExecutionSession, sequence: u64) {
    let position =
        NativePointerPosition::new(Vec2::new(-1.0, 0.0), Vec2::new(100.0, 100.0)).unwrap();
    for (sequence, kind) in [
        (
            sequence,
            NativePointerInputKind::Press {
                position,
                button: 0,
            },
        ),
        (
            sequence + 1,
            NativePointerInputKind::Release {
                position,
                button: 0,
            },
        ),
    ] {
        let token = session.native_pointer_input_token().unwrap();
        let input = NativePointerInput::new(
            sequence,
            token.pointer(),
            token.context(),
            NativeInputModifiers::default(),
            kind,
        );
        session.submit_native_pointer_input(&token, input).unwrap();
    }
}

#[test]
fn sealed_session_pointer_interest_and_dispatch_follow_displayed_binding_history() {
    let (scene, target, mut source, object) = recorded_scene(None);
    for binding in [None, Some(A), Some(B), None] {
        bind(&scene, &mut source, &target, binding);
        let boundary = source.wait_segment(1.0).unwrap();
        source.advance_to(boundary.end_time()).unwrap();
    }
    assert_eq!(target.state().unwrap().click_indicate(), None);
    source.seal_replay().unwrap();
    source
        .configure_native_pointer_input(
            NativePointerId {
                source: 71,
                pointer: 2,
            },
            1,
        )
        .unwrap();
    drop(target);
    drop(scene);

    for (index, (time, binding)) in [
        (0.5, None),
        (1.5, Some(A)),
        (2.5, Some(B)),
        (3.5, None),
        (1.5, Some(A)),
    ]
    .into_iter()
    .enumerate()
    {
        source.seek(time).unwrap();
        assert_eq!(source.has_native_pointer_subscribers(), binding.is_some());
        let frame = source.frame().clone();
        let history = source.replay_stats();
        click(&mut source, 2 * index as u64);
        if let Some(binding) = binding {
            assert!(source.interactions_active());
            let wall_time = 10.0 * index as f64;
            source.advance_interactions(wall_time).unwrap();
            source
                .advance_interactions(wall_time + binding.run_time() * 0.5)
                .unwrap();
            midpoint(source.frame(), object, binding);
            assert_eq!(source.frame().time, time);
            source
                .advance_interactions(wall_time + binding.run_time())
                .unwrap();
        }
        assert!(!source.interactions_active());
        assert_eq!(source.frame(), &frame);
        assert_eq!(source.replay_stats(), history);
    }
}

fn changed_pixels(actual: &[u8], expected: &[u8]) -> usize {
    assert_eq!(actual.len(), expected.len());
    actual
        .as_chunks::<4>()
        .0
        .iter()
        .zip(expected.as_chunks::<4>().0)
        .filter(|(actual, expected)| *actual != *expected)
        .count()
}

#[test]
#[ignore = "requires actual Vulkan raster; explicitly invoked by focused qualification"]
fn historical_binding_pixels_differ_on_the_same_baseline_and_restore_after_source_drop() {
    pollster::block_on(async {
        let (scene, target, mut source, object) = recorded_scene(None);
        let initial = source.wait_segment(1.0).unwrap();
        let mut oracle = raster_support::Raster::new().await;
        source.advance_to(0.5).unwrap();
        let reference = oracle.capture(&source.take_renderer_publication());
        assert!(
            reference
                .as_chunks::<4>()
                .0
                .iter()
                .filter(|pixel| pixel[2] > 32)
                .count()
                > 128,
            "the first-pass baseline must contain a visible filled target"
        );
        source.advance_to(initial.end_time()).unwrap();
        let (mut publisher, mut reader) = source.create_prepared_prefix(initial).unwrap();
        for binding in [Some(A), Some(B), None] {
            bind(&scene, &mut source, &target, binding);
            assert_eq!(
                oracle.capture(&source.take_renderer_publication()),
                reference,
                "changing only the declaration must preserve first-pass pixels"
            );
            append_interval(&mut source, &mut publisher, &mut reader);
        }
        drop(target);
        drop(source);
        drop(scene);
        drop(publisher);
        drop(oracle);

        let mut retained = raster_support::Raster::new().await;
        let mut midpoints = Vec::new();
        for (index, (time, binding)) in [(1.5, A), (2.5, B)].into_iter().enumerate() {
            reader.seek(time).unwrap();
            assert_eq!(
                retained.capture(&reader.take_renderer_publication()),
                reference
            );
            let prepared = reader
                .prepare_bound_click_indicate(object)
                .unwrap()
                .unwrap();
            reader.start_transient_animation(prepared).unwrap();
            let wall_time = 10.0 * index as f64;
            reader.advance_interactions(wall_time).unwrap();
            reader
                .advance_interactions(wall_time + binding.run_time() * 0.5)
                .unwrap();
            let pixels = retained.capture(&reader.take_renderer_publication());
            assert!(changed_pixels(&pixels, &reference) > 128);
            assert_eq!(reader.frame().time, time);
            midpoints.push(pixels);
            reader
                .advance_interactions(wall_time + binding.run_time())
                .unwrap();
            assert!(!reader.interactions_active());
            assert_eq!(
                retained.capture(&reader.take_renderer_publication()),
                reference
            );
        }
        let binding_difference = changed_pixels(&midpoints[0], &midpoints[1]);
        assert!(
            binding_difference > 128,
            "different past declarations must visibly differ"
        );
        for time in [0.5, 3.5] {
            reader.seek(time).unwrap();
            assert!(reader
                .prepare_bound_click_indicate(object)
                .unwrap()
                .is_none());
            assert_eq!(
                retained.capture(&reader.take_renderer_publication()),
                reference
            );
        }

        drop(retained);
        let mut fresh = raster_support::Raster::new().await;
        reader.seek(1.5).unwrap();
        assert_eq!(
            fresh.capture(&reader.take_renderer_publication()),
            reference
        );
        let prepared = reader
            .prepare_bound_click_indicate(object)
            .unwrap()
            .unwrap();
        reader.start_transient_animation(prepared).unwrap();
        reader.advance_interactions(30.0).unwrap();
        reader.advance_interactions(30.25).unwrap();
        assert_eq!(
            fresh.capture(&reader.take_renderer_publication()),
            midpoints[0]
        );
        reader.seek(2.5).unwrap();
        assert!(!reader.interactions_active());
        assert_eq!(
            fresh.capture(&reader.take_renderer_publication()),
            reference
        );
        let prepared = reader
            .prepare_bound_click_indicate(object)
            .unwrap()
            .unwrap();
        reader.start_transient_animation(prepared).unwrap();
        reader.advance_interactions(40.0).unwrap();
        reader.advance_interactions(40.5).unwrap();
        assert_eq!(
            fresh.capture(&reader.take_renderer_publication()),
            midpoints[1]
        );
        reader.advance_interactions(41.0).unwrap();
        assert_eq!(
            fresh.capture(&reader.take_renderer_publication()),
            reference
        );
        eprintln!(
            "PREFIX_BINDING_RASTER: distinct historical actions ({binding_difference} different midpoint pixels), identical first-pass baseline, source destruction, exact restoration, seek cancellation, fresh renderer"
        );
    });
}
