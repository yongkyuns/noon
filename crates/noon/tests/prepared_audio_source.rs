//! Fixed soundtracks through the public headless source/reader boundary.
//! These tests query plans only: they do not allocate or qualify an audio device.
#![cfg(feature = "prepared-prefix")]

use noon::{
    AnimationOptions, AudioClipOptions, AudioResource, ContinuationStep, LiveContinuation,
    LiveSession, LiveSessionError, Mobject, PrefixReader, PreparedSource, PreparedSourceError,
    PreparedSourceStatus, RateFunction, Scene,
};
use noon_core::{AudioClipDefinition, SemanticMutationTransaction, SemanticNodeId};
use noon_runtime::{AudioGainEnvelope, PrefixError, ReplayError, ReplayLimits};
use std::{
    cell::Cell,
    rc::Rc,
    sync::{Arc, Weak},
};

struct Script {
    target: Mobject,
    calls: Rc<Cell<usize>>,
    drops: Rc<Cell<usize>>,
    fail_after_play: bool,
}

impl Drop for Script {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
    }
}

impl LiveContinuation for Script {
    type Error = LiveSessionError;

    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, Self::Error> {
        let step = self.calls.get();
        self.calls.set(step + 1);
        match step {
            0 => {
                let target = live.target_editor(&self.target)?;
                live.set_translation(&target, 2.0, 0.0)?;
                Ok(ContinuationStep::Await(
                    live.declare_and_activate_transform_to(
                        &self.target,
                        &target,
                        AnimationOptions::new()
                            .run_time(1.0)
                            .rate_func(RateFunction::Linear),
                    )?,
                ))
            }
            1 => {
                // Completion, not presentation, makes the next source getter valid.
                assert_eq!(live.effective(&self.target)?.transform.translation.x, 2.0);
                live.wait_segment(if self.fail_after_play { -1.0 } else { 2.0 })
                    .map(ContinuationStep::Await)
            }
            2 => {
                live.set_fill(&self.target, 0.0, 1.0, 0.0, 1.0)?;
                Ok(ContinuationStep::Finished)
            }
            _ => panic!("completed or failed source must never run again"),
        }
    }
}

fn scene_and_script(fail_after_play: bool) -> (Scene, Script, Weak<[f32]>) {
    let mut scene = Scene::new();
    let mut target = scene.circle(0.5).unwrap();
    target.set_fill(0.0, 0.0, 1.0, 1.0).unwrap();
    scene.add(&target).unwrap();
    let samples: Arc<[f32]> = vec![0.125; 8_000].into();
    let weak = Arc::downgrade(&samples);
    scene
        .audio(
            AudioResource::from_interleaved(8_000, 1, samples, 32_000).unwrap(),
            AudioClipOptions {
                start_time: 0.25,
                source_start_frame: 1_000,
                source_end_frame: Some(7_000),
                duration: Some(8.0),
                looped: true,
                gain: 0.5,
                fade_in: 0.5,
                fade_out: 1.0,
            },
        )
        .unwrap();
    let script = Script {
        target,
        calls: Rc::new(Cell::new(0)),
        drops: Rc::new(Cell::new(0)),
        fail_after_play,
    };
    (scene, script, weak)
}

struct Fixture {
    source: PreparedSource<Script>,
    calls: Rc<Cell<usize>>,
    drops: Rc<Cell<usize>>,
    pcm: Weak<[f32]>,
}

fn fixture(fail: bool) -> Fixture {
    let (scene, script, pcm) = scene_and_script(fail);
    let calls = script.calls.clone();
    let drops = script.drops.clone();
    let source = scene
        .into_prepared_source(script, ReplayLimits::default())
        .unwrap();
    assert_eq!(calls.get(), 0, "construction cannot invoke source code");
    Fixture {
        source,
        calls,
        drops,
        pcm,
    }
}

fn deliver(source: &mut PreparedSource<Script>, reader: &mut PrefixReader) {
    let batch = source.pending_batch().expect("one retained suffix");
    reader
        .reserve_object_capacity(batch.required_object_capacity())
        .unwrap();
    reader.preflight_append(batch).unwrap();
    reader.append(source.take_batch().unwrap()).unwrap();
}

#[derive(Debug, PartialEq)]
struct ClipObservation {
    clip: SemanticNodeId,
    pcm: *const f32,
    delay: f64,
    duration: f64,
    offset: f64,
    gains: AudioGainEnvelope,
}

fn observe(reader: &PrefixReader) -> Vec<ClipObservation> {
    let publication = reader.publication_context();
    let result = reader
        .audio_playback()
        .unwrap()
        .map(|clip| ClipObservation {
            clip: clip.clip,
            pcm: clip.pcm.samples().as_ptr(),
            delay: clip.start_delay,
            duration: clip.duration,
            offset: clip.source_offset,
            gains: clip.gain_points,
        })
        .collect();
    assert_eq!(
        reader.publication_context(),
        publication,
        "query is read-only"
    );
    result
}

#[test]
fn soundtrack_prepares_once_and_only_delivery_extends_reader_audio() {
    let mut f = fixture(false);
    assert_eq!(
        f.source.prepare_next().unwrap(),
        PreparedSourceStatus::ReaderReady
    );
    for _ in 0..4 {
        assert_eq!(
            f.source.prepare_next().unwrap(),
            PreparedSourceStatus::ReaderReady
        );
    }
    assert_eq!(f.calls.get(), 1);
    let mut reader = f.source.take_reader().unwrap();
    assert!(reader.has_audio());
    reader.seek(0.5).unwrap();
    assert_eq!(reader.frame().objects[0].transform.translation.x, 1.0);
    let frame = reader.frame().clone();
    let publication = reader.publication_context();
    let sound = observe(&reader);
    assert_eq!(sound.len(), 1);
    assert_eq!(sound[0].pcm, f.pcm.upgrade().unwrap().as_ptr());
    assert_eq!(sound[0].offset, 0.375);
    assert_eq!(sound[0].duration, 0.5);
    assert_eq!(sound[0].gains.points()[0].gain, 0.25);
    assert_eq!(sound[0].gains.points().last().unwrap().gain, 0.5);

    assert_eq!(
        f.source.prepare_next().unwrap(),
        PreparedSourceStatus::BatchReady
    );
    for _ in 0..4 {
        assert_eq!(
            f.source.prepare_next().unwrap(),
            PreparedSourceStatus::BatchReady
        );
    }
    assert_eq!(f.calls.get(), 2);
    assert_eq!(
        observe(&reader),
        sound,
        "undelivered preparation cannot extend sound"
    );
    deliver(&mut f.source, &mut reader);
    assert_eq!(reader.frame(), &frame);
    assert_eq!(reader.publication_context(), publication);
    assert_eq!(reader.frontier(), 3.0);
    let extended = observe(&reader);
    assert_eq!(extended[0].duration, 2.5);
    assert_eq!(extended[0].offset, sound[0].offset);
    assert_eq!(extended[0].pcm, sound[0].pcm);
    assert_eq!(extended[0].gains.points()[0], sound[0].gains.points()[0]);

    assert_eq!(
        f.source.prepare_next().unwrap(),
        PreparedSourceStatus::BatchReady
    );
    assert!(f.source.pending_batch().unwrap().is_terminal());
    assert_eq!(f.drops.get(), 1);
    assert_eq!(reader.end_time(), None, "source completion is not delivery");
    deliver(&mut f.source, &mut reader);
    assert_eq!(reader.end_time(), Some(3.0));
    assert_eq!(
        f.source.prepare_next().unwrap(),
        PreparedSourceStatus::Finished
    );
    assert_eq!(f.calls.get(), 3);
    assert_eq!(reader.frame(), &frame);
    reader.seek(3.0).unwrap();
    assert!(
        observe(&reader).is_empty(),
        "completed source clips longer music"
    );
    reader.seek(0.5).unwrap();
    assert_eq!(reader.frame(), &frame);
    assert_eq!(observe(&reader), extended);
    drop(reader);
    assert!(
        f.pcm.upgrade().is_none(),
        "last reader releases the PCM closure"
    );
}

#[test]
fn cancellation_drops_pending_sound_horizon_without_revoking_delivered_history() {
    let mut f = fixture(false);
    f.source.prepare_next().unwrap();
    let mut reader = f.source.take_reader().unwrap();
    reader.seek(0.5).unwrap();
    let sound = observe(&reader);
    f.source.prepare_next().unwrap();
    f.source.cancel();
    f.source.cancel();
    assert!(f.source.pending_batch().is_none());
    assert_eq!(
        f.source.prepare_next().unwrap(),
        PreparedSourceStatus::Cancelled
    );
    assert_eq!(f.calls.get(), 2);
    assert_eq!(f.drops.get(), 1);
    assert_eq!(reader.frontier(), 1.0);
    assert_eq!(reader.end_time(), None);
    assert!(reader.seek(1.0).is_err());
    assert_eq!(observe(&reader), sound);
    drop(reader);
    assert!(f.pcm.upgrade().is_none());
}

#[test]
fn source_failure_keeps_existing_audio_history_and_a_fresh_run_can_prepare() {
    let mut f = fixture(true);
    f.source.prepare_next().unwrap();
    let mut reader = f.source.take_reader().unwrap();
    reader.seek(0.5).unwrap();
    let sound = observe(&reader);
    assert!(matches!(
        f.source.prepare_next(),
        Err(PreparedSourceError::Program(_))
    ));
    for _ in 0..4 {
        assert_eq!(
            f.source.prepare_next().unwrap(),
            PreparedSourceStatus::Failed
        );
    }
    assert_eq!(f.calls.get(), 2);
    assert_eq!(f.drops.get(), 1);
    assert_eq!(reader.end_time(), None);
    assert_eq!(observe(&reader), sound);
    let mut recovery = fixture(false);
    assert_eq!(
        recovery.source.prepare_next().unwrap(),
        PreparedSourceStatus::ReaderReady
    );
    assert_eq!(recovery.calls.get(), 1);
    assert_eq!(
        observe(&reader),
        sound,
        "a replacement cannot mutate the old reader"
    );
}

#[test]
fn cancelling_before_preparation_never_calls_source_and_releases_audio() {
    let mut f = fixture(false);
    f.source.cancel();
    assert_eq!(
        f.source.prepare_next().unwrap(),
        PreparedSourceStatus::Cancelled
    );
    assert_eq!(f.calls.get(), 0);
    assert_eq!(f.drops.get(), 1);
    assert!(f.pcm.upgrade().is_none());
}

#[test]
fn realtime_live_source_still_rejects_audio_before_invoking_continuation() {
    let (scene, script, pcm) = scene_and_script(false);
    let calls = script.calls.clone();
    let drops = script.drops.clone();
    assert!(matches!(
        scene.into_live_program(script),
        Err(noon_compile::SemanticExecutionLoweringError::AudioExecutionUnavailable { .. })
    ));
    assert_eq!(calls.get(), 0);
    assert_eq!(drops.get(), 1);
    assert!(pcm.upgrade().is_none());
}

#[test]
fn mixed_root_and_nested_audio_stays_rejected_before_preparation() {
    let (scene, script, _) = scene_and_script(false);
    let calls = script.calls.clone();
    let drops = script.drops.clone();
    {
        let mut store = scene.integration_store().borrow_mut();
        let nested = store.insert_family();
        store.add_member(scene.root(), nested).unwrap();
        let resource = store
            .insert_audio_resource(
                AudioResource::from_interleaved(8_000, 1, vec![0.0; 8_000], 32_000).unwrap(),
            )
            .unwrap();
        let definition = AudioClipDefinition::new(
            resource,
            store.audio_resources().get(resource).unwrap(),
            AudioClipOptions::default(),
        )
        .unwrap();
        let mut declaration = SemanticMutationTransaction::new();
        declaration.create_audio_clip(nested, definition);
        declaration.apply(&mut store).unwrap();
    }
    assert!(matches!(
        scene.into_prepared_source(script, ReplayLimits::default()),
        Err(PreparedSourceError::Prefix(PrefixError::Replay(
            ReplayError::UnsupportedDomain
        )))
    ));
    assert_eq!(calls.get(), 0);
    assert_eq!(drops.get(), 1);
}
