//! M2a diagnostic control for #2211, not a production incremental player.
//!
//! Copy the existing execution projection ONCE at a completed prefix, then keep
//! the sole Semantic Scene with the producer. The reader has no source handles
//! and uses the ordinary Rust replay evaluator. This deliberately O(plan + saved
//! history) bootstrap is an isolation oracle, NOT a per-segment publication
//! strategy, startup benchmark, or permission to change the viewer default.
//! A later sparse incremental implementation must pass these witnesses without
//! repeating this clone. This module is compiled only for tests.

use super::{ExecutionSession, ReplayError, ReplayLimits};
use crate::{
    AnimationOptions, ExecutionSegment, FadeEndpoint, FadeTranslation, Mobject, RateFunction, Scene,
};
use noon_core::{SemanticFadeDirection, SemanticVec3};
use noon_runtime::{EvaluationError, FrameState};
use std::sync::mpsc;
use std::time::Duration;

#[derive(Debug)]
struct PrefixProbe {
    reader: ExecutionSession,
}

impl PrefixProbe {
    fn capture(
        producer: &ExecutionSession,
        source_segment: Option<ExecutionSegment>,
    ) -> Result<Self, ReplayError> {
        // The actual source owner must supply its current continuation. A quiet
        // wait is a value boundary, deliberately not a session-owned no-op track.
        // None is used only at this harness's known no-continuation boundaries.
        if source_segment.is_some_and(|segment| !producer.segment_state(segment).is_complete()) {
            return Err(ReplayError::Incomplete);
        }
        // Clone discards pending continuation tokens. Check the ORIGINAL owner
        // before any copy; checking only the reader can admit an unfinished play.
        producer.ensure_replay_completion()?;
        let mut reader = producer.clone();
        reader.seal_replay()?;
        Ok(Self { reader })
    }

    fn sample(&mut self, time: f64) -> Result<FrameState, EvaluationError> {
        self.reader.seek(time)?;
        Ok(snapshot(&self.reader))
    }
}

fn options(duration: f64) -> AnimationOptions {
    AnimationOptions::new()
        .run_time(duration)
        .rate_func(RateFunction::Linear)
}

fn recording(scene: &Scene, limits: ReplayLimits) -> ExecutionSession {
    let mut producer = scene.execution_session().unwrap();
    producer.begin_replay_retention(limits).unwrap();
    producer
}

fn recorded_scene(limits: ReplayLimits) -> (Scene, Mobject, ExecutionSession) {
    let mut scene = Scene::new();
    let shape = scene.square(1.0).unwrap();
    scene.add(&shape).unwrap();
    let producer = recording(&scene, limits);
    (scene, shape, producer)
}

// First-pass observations in actual painter order, not all pinned physical rows.
// Use the observation boundary of the finite_replay_history integration test.
fn snapshot(session: &ExecutionSession) -> FrameState {
    let source = session.frame();
    let mut result = source.clone();
    let indices: Vec<_> = session
        .painter_order()
        .iter()
        .map(|&i| i as usize)
        .collect();
    result.objects = indices.iter().map(|&i| source.objects[i].clone()).collect();
    result.presences = indices.iter().map(|&i| source.presences[i]).collect();
    result.reveals = indices.iter().map(|&i| source.reveals[i]).collect();
    result.morphs = indices.iter().map(|&i| source.morphs[i]).collect();
    result.render_geometries = indices
        .iter()
        .map(|&i| source.render_geometries[i].clone())
        .collect();
    result.render_transforms = indices
        .iter()
        .map(|&i| source.render_transforms[i])
        .collect();
    result.family_animations = indices
        .iter()
        .map(|&i| source.family_animations[i])
        .collect();
    result.family_animation_plan_indices = indices
        .iter()
        .map(|&i| source.family_animation_plan_indices[i])
        .collect();
    result
}

#[test]
fn completed_prefix_reader_survives_future_morph_membership_and_producer_drop() {
    let mut scene = Scene::new();
    let shape = scene.square(1.0).unwrap();
    let mut circle = scene.circle(0.6).unwrap();
    circle.shift(2.0, 0.0).unwrap();
    let rectangle = scene.rectangle(2.0, 0.4).unwrap();
    scene.add(&shape).unwrap();
    // All detached targets are declared before bootstrapping this explicit
    // integration session; later changes use scene.live(&mut producer).
    let mut producer = recording(&scene, ReplayLimits::default());
    let segment = scene
        .live(&mut producer)
        .declare_and_activate_transform_to(&shape, &circle, options(1.0))
        .unwrap();
    let mut expected = Vec::new();
    for time in [0.25, 0.75] {
        producer.advance_to(time).unwrap();
        expected.push(snapshot(&producer));
    }
    {
        let mut live = scene.live(&mut producer);
        live.advance_segment_to(segment, segment.end_time())
            .unwrap();
        live.complete_segment(segment).unwrap();
    }
    producer.advance_to(1.25).unwrap();
    expected.push(snapshot(&producer));
    let mut prefix = PrefixProbe::capture(&producer, Some(segment)).unwrap();
    assert_ne!(
        prefix.reader.runtime_identity(),
        producer.runtime_identity()
    );
    assert!(!producer.replay_is_sealed());

    // Real OS-thread ownership with deterministic bounded interleaving. This is
    // an isolation witness, not a wall-time concurrency/contention measurement.
    let (request_tx, request_rx) = mpsc::sync_channel::<()>(1);
    let (ack_tx, ack_rx) = mpsc::sync_channel::<()>(1);
    let worker = std::thread::spawn(move || {
        let mut rounds = 0;
        while request_rx.recv_timeout(Duration::from_secs(20)).is_ok() {
            for index in [2, 0, 1, 0, 2, 1] {
                let reference = &expected[index];
                assert_eq!(prefix.sample(reference.time).unwrap(), *reference);
            }
            ack_tx.send(()).unwrap();
            rounds += 1;
        }
        rounds
    });
    let observe = || {
        request_tx.send(()).unwrap();
        ack_rx.recv_timeout(Duration::from_secs(20)).unwrap();
    };
    observe();

    let segment = scene
        .live(&mut producer)
        .declare_and_activate_transform_to(&shape, &rectangle, options(0.75))
        .unwrap();
    {
        let mut live = scene.live(&mut producer);
        live.advance_segment_to(segment, segment.end_time())
            .unwrap();
        live.complete_segment(segment).unwrap();
    }
    let frontier = snapshot(&producer);
    let revision = scene.integration_store().borrow().scene_revision();
    observe();
    assert_eq!(snapshot(&producer), frontier, "reader rewound the producer");
    assert_eq!(
        scene.integration_store().borrow().scene_revision(),
        revision
    );

    for direction in [SemanticFadeDirection::Out, SemanticFadeDirection::In] {
        let segment = scene
            .live(&mut producer)
            .declare_and_activate_fade_with_endpoint(
                &shape,
                direction,
                FadeEndpoint::new(
                    0.25,
                    FadeTranslation::Shift(SemanticVec3::new(1.0, 0.0, 0.0)),
                ),
                options(0.5),
            )
            .unwrap();
        {
            let mut live = scene.live(&mut producer);
            live.advance_segment_to(segment, segment.end_time())
                .unwrap();
            live.complete_segment(segment).unwrap();
        }
        let frontier = snapshot(&producer);
        observe();
        assert_eq!(snapshot(&producer), frontier);
    }
    producer.discard_replay_retention();
    scene.live(&mut producer).shift(&shape, 9.0, 1.0).unwrap();
    observe();
    drop(producer);
    drop(shape);
    drop(circle);
    drop(rectangle);
    drop(scene);
    observe();
    drop(request_tx);
    assert_eq!(worker.join().unwrap(), 6);
}

#[test]
fn source_admission_rejects_partial_animation_and_uncompleted_endpoint() {
    let mut scene = Scene::new();
    let shape = scene.square(1.0).unwrap();
    let target = scene.circle(0.5).unwrap();
    scene.add(&shape).unwrap();
    let mut producer = recording(&scene, ReplayLimits::default());
    let segment = scene
        .live(&mut producer)
        .declare_and_activate_transform_to(&shape, &target, options(1.0))
        .unwrap();
    producer.advance_to(0.5).unwrap();
    let before = snapshot(&producer);
    assert_eq!(
        PrefixProbe::capture(&producer, Some(segment)).unwrap_err(),
        ReplayError::Incomplete
    );
    // Even an omitted source receipt must not bypass session-owned release work.
    assert_eq!(
        PrefixProbe::capture(&producer, None).unwrap_err(),
        ReplayError::Incomplete
    );
    assert_eq!(snapshot(&producer), before);
    {
        let mut live = scene.live(&mut producer);
        live.advance_segment_to(segment, segment.end_time())
            .unwrap();
    }
    assert_eq!(
        PrefixProbe::capture(&producer, Some(segment)).unwrap_err(),
        ReplayError::Incomplete
    );
    scene.live(&mut producer).complete_segment(segment).unwrap();
    assert!(PrefixProbe::capture(&producer, Some(segment)).is_ok());
    assert!(!producer.replay_is_sealed());
}

#[test]
fn source_admission_does_not_mistake_a_quiet_wait_for_completion() {
    let (_scene, _shape, mut producer) = recorded_scene(ReplayLimits::default());
    let segment = producer.wait_segment(1.0).unwrap();
    producer
        .advance_segment_to_callback_barrier(segment, 0.5)
        .unwrap();
    assert!(!producer.segment_state(segment).is_complete());
    assert_eq!(
        PrefixProbe::capture(&producer, Some(segment)).unwrap_err(),
        ReplayError::Incomplete
    );
    producer
        .advance_segment_to_callback_barrier(segment, segment.end_time())
        .unwrap();
    assert!(producer.segment_state(segment).is_complete());
    assert!(PrefixProbe::capture(&producer, Some(segment)).is_ok());
}

#[test]
fn unavailable_or_invalid_time_preserves_the_last_reader_frame() {
    let (_scene, _shape, mut producer) = recorded_scene(ReplayLimits::default());
    producer.advance_to(1.0).unwrap();
    let mut prefix = PrefixProbe::capture(&producer, None).unwrap();
    let before = prefix.sample(0.5).unwrap();
    producer.advance_to(10.0).unwrap();
    for time in [-1.0, 1.0001, 5.0, f64::NAN, f64::INFINITY] {
        assert!(prefix.sample(time).is_err(), "unexpectedly admitted {time}");
        assert_eq!(snapshot(&prefix.reader), before);
        assert_eq!(producer.frame().time, 10.0);
    }
    assert_eq!(prefix.sample(1.0).unwrap().time, 1.0);
}

#[test]
fn a_reader_cannot_commit_an_authored_edit_or_rewind_the_semantic_store() {
    let (scene, shape, mut producer) = recorded_scene(ReplayLimits::default());
    producer.advance_to(1.0).unwrap();
    let mut prefix = PrefixProbe::capture(&producer, None).unwrap();
    let before = prefix.sample(0.5).unwrap();
    producer.advance_to(2.0).unwrap();
    scene.live(&mut producer).shift(&shape, 5.0, 0.0).unwrap();
    let revision = scene.integration_store().borrow().scene_revision();
    let frontier = snapshot(&producer);
    assert!(scene
        .live(&mut prefix.reader)
        .shift(&shape, 1.0, 0.0)
        .is_err());
    assert_eq!(
        scene.integration_store().borrow().scene_revision(),
        revision
    );
    assert_eq!(snapshot(&producer), frontier);
    assert_eq!(snapshot(&prefix.reader), before);
}

#[test]
fn closed_prefix_endpoint_is_revision_specific_not_an_append_watermark() {
    let (scene, shape, mut producer) = recorded_scene(ReplayLimits::default());
    producer.advance_to(0.5).unwrap();
    scene.live(&mut producer).shift(&shape, 1.0, 0.0).unwrap();
    producer.advance_to(1.0).unwrap();
    let mut prefix = PrefixProbe::capture(&producer, None).unwrap();
    let old_endpoint = prefix.sample(1.0).unwrap();
    let old_interior = prefix.sample(0.75).unwrap();

    // Source may issue another operation at the same completed endpoint. A
    // growing prefix needs half-open time or an intra-timestamp revision policy.
    scene.live(&mut producer).shift(&shape, 4.0, 0.0).unwrap();
    producer.advance_to(2.0).unwrap();
    producer.seal_replay().unwrap();
    producer.seek(1.0).unwrap();
    assert_ne!(snapshot(&producer), old_endpoint);
    producer.seek(0.75).unwrap();
    assert_eq!(snapshot(&producer), old_interior);
    assert_eq!(prefix.sample(1.0).unwrap(), old_endpoint);
}

#[test]
fn later_retention_failure_does_not_destroy_an_already_captured_prefix() {
    let (scene, shape, mut producer) = recorded_scene(ReplayLimits {
        revisions: 4,
        payloads: 1024,
    });
    producer.advance_to(1.0).unwrap();
    let mut prefix = PrefixProbe::capture(&producer, None).unwrap();
    let before = prefix.sample(0.5).unwrap();
    for step in 0..16 {
        producer.advance_to(2.0 + f64::from(step)).unwrap();
        scene.live(&mut producer).shift(&shape, 1.0, 0.0).unwrap();
    }
    assert_eq!(producer.seal_replay(), Err(ReplayError::RetentionLimit));
    assert_eq!(prefix.sample(0.5).unwrap(), before);
    producer.discard_replay_retention();
    scene.live(&mut producer).shift(&shape, 1.0, 0.0).unwrap();
    assert_eq!(prefix.sample(0.5).unwrap(), before);
}

#[test]
fn no_recording_cannot_be_certified_by_cloning_current_state() {
    let mut scene = Scene::new();
    let shape = scene.square(1.0).unwrap();
    scene.add(&shape).unwrap();
    let producer = scene.execution_session().unwrap();
    assert_eq!(
        PrefixProbe::capture(&producer, None).unwrap_err(),
        ReplayError::NotRecording
    );
    assert!(!producer.replay_scope_active());
}
