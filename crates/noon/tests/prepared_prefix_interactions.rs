#![cfg(feature = "prepared-prefix")]
//! Public Scene -> prepared prefix qualification for the existing typed Indicate.
//! These are reader-local presentation effects, not pointer dispatch or retained
//! authored bindings. The source and the displayed historical interval may differ.

mod raster_support;

use noon::{ExecutionSession, Mobject, PrefixPublisher, PrefixReader, Scene};
use noon_core::{Color, ObjectId, SemanticClickIndicate};
use noon_runtime::{FrameState, ReplayLimits};

fn recorded_scene() -> (Scene, Mobject, ExecutionSession, ObjectId) {
    let mut scene = Scene::new();
    let mut target = scene.circle(0.65).unwrap();
    target.set_fill(0.15, 0.4, 0.9, 1.0).unwrap();
    target.shift(-1.0, 0.0).unwrap();
    scene.add(&target).unwrap();
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

fn fixture() -> (
    Scene,
    Mobject,
    ExecutionSession,
    ObjectId,
    PrefixPublisher,
    PrefixReader,
) {
    let (scene, target, mut source, object) = recorded_scene();
    let boundary = source.wait_segment(1.0).unwrap();
    source.advance_to(boundary.end_time()).unwrap();
    let (publisher, mut reader) = source.create_prepared_prefix(boundary).unwrap();
    reader.seek(0.5).unwrap();
    (scene, target, source, object, publisher, reader)
}

fn indicate() -> SemanticClickIndicate {
    SemanticClickIndicate::new(1.4, Color::YELLOW, 1.0)
}

fn start(reader: &mut PrefixReader, object: ObjectId, wall_time: f64) {
    let prepared = reader
        .prepare_click_indicate(object, indicate())
        .unwrap()
        .expect("the displayed historical target must admit Indicate");
    reader.start_transient_animation(prepared).unwrap();
    reader.advance_interactions(wall_time).unwrap();
    assert!(reader.interactions_active());
}

fn at_time(frame: &FrameState, time: f64) -> FrameState {
    let mut frame = frame.clone();
    frame.time = time;
    frame
}

#[test]
fn historical_effect_survives_append_and_forward_playback_without_editing_source() {
    let (scene, target, mut source, object, mut publisher, mut reader) = fixture();
    let baseline = reader.frame().clone();
    let source_before = source.frame().clone();
    let history = reader.replay_stats();
    let context = reader.take_renderer_publication().context();
    start(&mut reader, object, 10.0);
    reader.advance_interactions(10.5).unwrap();
    let middle = reader.frame().clone();
    let middle_context = reader.take_renderer_publication().context();
    assert_ne!(
        middle, baseline,
        "the effect must change its displayed target"
    );
    assert_eq!(middle.time, 0.5);
    assert_eq!(reader.frontier(), 1.0);
    assert_eq!(reader.replay_stats(), history);
    assert_eq!(source.frame(), &source_before);
    assert_eq!(middle_context.scene_revision(), context.scene_revision());
    assert_eq!(
        middle_context.execution_revision(),
        context.execution_revision()
    );
    assert_ne!(middle_context.frame_epoch(), context.frame_epoch());
    assert!(
        reader
            .prepare_click_indicate(object, indicate())
            .unwrap()
            .is_none(),
        "an active effect must not capture its enlarged midpoint again"
    );

    // New source revisions belong to the future. Receiving them must not replace
    // the current display, cancel its effect, or reuse the producer's new values.
    scene.live(&mut source).shift(&target, 2.0, 0.0).unwrap();
    scene
        .live(&mut source)
        .set_fill(&target, 0.9, 0.1, 0.2, 1.0)
        .unwrap();
    let boundary = source.wait_segment(1.0).unwrap();
    source.advance_to(1.5).unwrap();
    let later_reference = source.frame().clone();
    source.advance_to(boundary.end_time()).unwrap();
    reader
        .append(
            source
                .publish_prepared_prefix(boundary, &mut publisher)
                .unwrap(),
        )
        .unwrap();
    assert_eq!(reader.frontier(), 2.0);
    assert_eq!(reader.frame(), &middle);
    assert_eq!(
        reader.take_renderer_publication().context(),
        middle_context,
        "append must not publish a different effective frame"
    );
    assert!(reader.interactions_active());
    let appended = reader.replay_stats();
    assert!(appended.revisions_retained > history.revisions_retained);
    let source_ahead = source.frame().clone();
    let source_state = target.state().unwrap();

    reader.advance_to(0.75).unwrap();
    assert!(reader.interactions_active());
    assert_eq!(reader.frame(), &at_time(&middle, 0.75));
    reader.advance_interactions(11.0).unwrap();
    assert!(!reader.interactions_active());
    assert_eq!(reader.frame(), &at_time(&baseline, 0.75));
    assert_eq!(reader.replay_stats(), appended);
    assert_eq!(source.frame(), &source_ahead);
    assert_eq!(target.state().unwrap(), source_state);

    // An actual crossed authored revision takes precedence over a captured
    // baseline. The following tick must not restore the old blue fill/position.
    start(&mut reader, object, 20.0);
    reader.advance_interactions(20.5).unwrap();
    reader.advance_to(1.5).unwrap();
    reader.advance_interactions(20.75).unwrap();
    assert!(!reader.interactions_active());
    assert_eq!(reader.frame(), &later_reference);
    assert_eq!(source.frame(), &source_ahead);
    assert_eq!(target.state().unwrap(), source_state);
}

#[test]
fn historical_target_remains_interactive_after_source_removal_and_destruction() {
    let (scene, target, mut source, object, mut publisher, mut reader) = fixture();
    let baseline = reader.frame().clone();
    scene.live(&mut source).remove(&target).unwrap();
    assert!(!scene.live(&mut source).contains(&target).unwrap());
    let boundary = source.wait_segment(1.0).unwrap();
    source.advance_to(boundary.end_time()).unwrap();
    reader
        .append(
            source
                .publish_prepared_prefix(boundary, &mut publisher)
                .unwrap(),
        )
        .unwrap();

    // Admission is against the displayed past, even though the producer has
    // already removed this target. The running effect owns no producer borrow.
    start(&mut reader, object, 30.0);
    reader.advance_interactions(30.5).unwrap();
    assert_ne!(reader.frame(), &baseline);
    drop(target);
    drop(source);
    drop(scene);
    drop(publisher);
    reader.advance_interactions(31.0).unwrap();
    assert!(!reader.interactions_active());
    assert_eq!(reader.frame(), &baseline);

    // The same execution identity is not actionable in its removed interval.
    reader.advance_to(1.5).unwrap();
    let removed = reader.frame().clone();
    let context = reader.take_renderer_publication().context();
    assert!(reader.prepare_click_indicate(object, indicate()).is_err());
    assert_eq!(reader.frame(), &removed);
    assert_eq!(reader.take_renderer_publication().context(), context);
    assert!(!reader.interactions_active());
    reader.seek(0.5).unwrap();
    start(&mut reader, object, 40.0);
    reader.advance_interactions(40.5).unwrap();
    assert_ne!(reader.frame(), &baseline);
    reader.advance_interactions(41.0).unwrap();
    assert_eq!(reader.frame(), &baseline);
}

#[test]
fn invalid_navigation_and_ticks_preserve_effect_but_explicit_seek_cancels_it() {
    let (_, _, _, object, _, mut reader) = fixture();
    let baseline = reader.frame().clone();
    start(&mut reader, object, 50.0);
    reader.advance_interactions(50.5).unwrap();
    let middle = reader.frame().clone();
    let context = reader.take_renderer_publication().context();
    let history = reader.replay_stats();
    for time in [f64::NAN, f64::INFINITY, -1.0, 1.0, 0.25] {
        assert!(reader.advance_to(time).is_err());
        assert_eq!(reader.frame(), &middle);
        assert_eq!(reader.take_renderer_publication().context(), context);
        assert_eq!(reader.replay_stats(), history);
        assert!(reader.interactions_active());
    }
    for time in [f64::NAN, f64::INFINITY, -1.0, 1.0] {
        assert!(reader.seek(time).is_err());
        assert_eq!(reader.frame(), &middle);
        assert_eq!(reader.take_renderer_publication().context(), context);
        assert_eq!(reader.replay_stats(), history);
        assert!(reader.interactions_active());
    }
    for time in [f64::NAN, f64::INFINITY, -1.0, 50.25] {
        assert!(reader.advance_interactions(time).is_err());
        assert_eq!(reader.frame(), &middle);
        assert_eq!(reader.take_renderer_publication().context(), context);
        assert_eq!(reader.replay_stats(), history);
        assert!(reader.interactions_active());
    }

    reader.seek(0.5).unwrap();
    assert!(!reader.interactions_active());
    assert_eq!(reader.frame(), &baseline);
    assert_eq!(reader.replay_stats(), history);
    let restored = reader.take_renderer_publication().context();
    assert_eq!(restored.scene_revision(), context.scene_revision());
    assert_eq!(restored.execution_revision(), context.execution_revision());
    assert_ne!(restored.frame_epoch(), context.frame_epoch());
    // A cancelled session effect is not resurrected by replay or later ticks.
    reader.advance_interactions(51.0).unwrap();
    reader.seek(0.25).unwrap();
    reader.advance_to(0.5).unwrap();
    assert_eq!(reader.frame(), &baseline);
    assert!(!reader.interactions_active());
}

#[test]
#[ignore = "requires actual Vulkan raster; explicitly invoked by focused qualification"]
fn prefix_indicate_pixels_change_and_restore_after_appends_source_drop_and_seek() {
    pollster::block_on(async {
        let (scene, target, mut source, object) = recorded_scene();
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
            "the first-pass reference must contain a visible filled target"
        );
        source.advance_to(initial.end_time()).unwrap();
        let (mut publisher, mut reader) = source.create_prepared_prefix(initial).unwrap();
        reader.seek(0.5).unwrap();
        let mut retained = raster_support::Raster::new().await;
        assert_eq!(
            retained.capture(&reader.take_renderer_publication()),
            reference
        );
        let baseline = reader.frame().clone();
        start(&mut reader, object, 60.0);
        reader.advance_interactions(60.5).unwrap();
        let middle = retained.capture(&reader.take_renderer_publication());
        let changed_pixels = middle
            .as_chunks::<4>()
            .0
            .iter()
            .zip(reference.as_chunks::<4>().0)
            .filter(|(actual, expected)| *actual != *expected)
            .count();
        assert!(
            changed_pixels > 128,
            "Indicate midpoint must visibly change pixels"
        );
        assert_eq!(
            reader.frame().time,
            0.5,
            "effects must not advance authored time"
        );

        // Publish a changed future, then a removal, while the historical effect
        // remains active. Neither append may alter the currently presented pixels.
        scene.live(&mut source).shift(&target, 2.0, 0.0).unwrap();
        scene
            .live(&mut source)
            .set_fill(&target, 0.9, 0.1, 0.2, 1.0)
            .unwrap();
        for removed in [false, true] {
            if removed {
                scene.live(&mut source).remove(&target).unwrap();
            }
            let boundary = source.wait_segment(1.0).unwrap();
            source.advance_to(boundary.end_time()).unwrap();
            reader
                .append(
                    source
                        .publish_prepared_prefix(boundary, &mut publisher)
                        .unwrap(),
                )
                .unwrap();
            assert!(reader.interactions_active());
            assert_eq!(
                retained.capture(&reader.take_renderer_publication()),
                middle
            );
            assert_eq!(reader.frame().time, 0.5);
        }
        let history = reader.replay_stats();
        drop(target);
        drop(source);
        drop(scene);
        drop(publisher);
        drop(oracle);

        reader.advance_to(0.75).unwrap();
        assert!(reader.interactions_active());
        assert_eq!(
            retained.capture(&reader.take_renderer_publication()),
            middle
        );
        reader.advance_interactions(61.0).unwrap();
        assert!(!reader.interactions_active());
        assert_eq!(reader.frame(), &at_time(&baseline, 0.75));
        assert_eq!(
            retained.capture(&reader.take_renderer_publication()),
            reference
        );
        assert_eq!(reader.replay_stats(), history);

        reader.seek(0.5).unwrap();
        start(&mut reader, object, 70.0);
        reader.advance_interactions(70.5).unwrap();
        assert_eq!(
            retained.capture(&reader.take_renderer_publication()),
            middle
        );
        reader.seek(0.5).unwrap();
        assert!(!reader.interactions_active());
        assert_eq!(
            retained.capture(&reader.take_renderer_publication()),
            reference
        );

        // Exact restoration must not rely on the old renderer's retained buffers.
        drop(retained);
        let mut fresh = raster_support::Raster::new().await;
        assert_eq!(
            fresh.capture(&reader.take_renderer_publication()),
            reference
        );
        start(&mut reader, object, 80.0);
        reader.advance_interactions(80.5).unwrap();
        assert_eq!(fresh.capture(&reader.take_renderer_publication()), middle);
        reader.advance_interactions(81.0).unwrap();
        assert_eq!(
            fresh.capture(&reader.take_renderer_publication()),
            reference
        );
        eprintln!(
            "PREFIX_INTERACTION_RASTER: visible midpoint ({changed_pixels} changed pixels), exact first-pass restoration, two appends, forward playback, source destruction, seek cancellation, fresh renderer"
        );
    });
}
