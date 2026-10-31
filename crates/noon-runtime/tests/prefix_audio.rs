//! Device-neutral audio plans in raw retained readers; no host/output admission.
#![cfg(feature = "prepared-prefix")]
mod prefix_support;

use noon_compile::{
    lower_semantic_execution_root, CompiledAudioClip, CompiledObject, ExecutionPatch,
    SemanticExecutionIndex,
};
use noon_core::{
    AudioClipDefinition, AudioClipOptions, AudioResource, GeometryRef, ObjectId,
    SemanticMutationTransaction, SemanticStore, Style, Transform2D, Vec2,
};
use noon_runtime::{PrefixError, ReplayLimits, SceneInstance};
use prefix_support::PrefixHarness;
use std::sync::{Arc, Weak};

struct Fixture {
    source: SceneInstance,
    pcm: Weak<[f32]>,
    resource_bytes: usize,
}

fn fixture(clips: usize) -> Fixture {
    let mut store = SemanticStore::new();
    let root = store.insert_family();
    let samples: Arc<[f32]> = vec![0.125; 8_000].into();
    let weak = Arc::downgrade(&samples);
    let pcm = AudioResource::from_interleaved(8_000, 1, samples, 32_000).unwrap();
    let bytes = pcm.retained_bytes() + clips * std::mem::size_of::<CompiledAudioClip>();
    let resource = store.insert_audio_resource(pcm).unwrap();
    let definition = AudioClipDefinition::new(
        resource,
        store.audio_resources().get(resource).unwrap(),
        AudioClipOptions {
            start_time: 0.25,
            source_start_frame: 1_000,
            source_end_frame: Some(7_000),
            duration: Some(6.0),
            looped: true,
            gain: 0.5,
            fade_in: 0.5,
            fade_out: 1.0,
        },
    )
    .unwrap();
    let mut transaction = SemanticMutationTransaction::new();
    for _ in 0..clips {
        transaction.create_audio_clip(root, definition);
    }
    transaction.apply(&mut store).unwrap();
    let lowered =
        lower_semantic_execution_root(&store, root, &mut SemanticExecutionIndex::new()).unwrap();
    let mut source = SceneInstance::from_semantic_execution(lowered);
    // A visual object exercises real saved row revisions, not an audio-only clone.
    source
        .apply_execution_patch(&ExecutionPatch::CreateObject(object()))
        .unwrap();
    source
        .begin_replay_retention(ReplayLimits {
            revisions: 32,
            payloads: 256,
        })
        .unwrap();
    source.advance_to(1.0).unwrap();
    Fixture {
        source,
        pcm: weak,
        resource_bytes: bytes,
    }
}

fn object() -> CompiledObject {
    CompiledObject::new(
        ObjectId::new(100),
        GeometryRef::circle(0.5),
        Transform2D::IDENTITY,
        Style::default(),
    )
}

fn observe(
    runtime: &SceneInstance,
) -> Vec<(
    noon_core::SemanticNodeId,
    f64,
    f64,
    f64,
    noon_runtime::AudioGainEnvelope,
)> {
    runtime
        .audio_playback()
        .unwrap()
        .map(|clip| {
            (
                clip.clip,
                clip.start_delay,
                clip.source_offset,
                clip.duration,
                clip.gain_points,
            )
        })
        .collect()
}

fn check_audio(reader: &PrefixHarness, pcm: &Weak<[f32]>, count: usize) {
    let time = reader.frame().time;
    let publication = reader.runtime.publication_context();
    let samples = pcm.upgrade().expect("reader must retain its immutable PCM");
    let clips = reader.runtime.audio_playback().unwrap().collect::<Vec<_>>();
    if time >= 6.25 {
        assert!(clips.is_empty());
        return;
    }
    assert_eq!(clips.len(), count);
    let elapsed = (time - 0.25).max(0.0);
    let gain = if elapsed < 0.5 {
        elapsed
    } else if elapsed > 5.0 {
        0.5 * (6.0 - elapsed)
    } else {
        0.5
    };
    for clip in clips {
        assert_eq!(
            clip.pcm.samples().as_ptr(),
            samples.as_ptr(),
            "PCM must remain shared"
        );
        assert!((clip.start_delay - (0.25 - time).max(0.0)).abs() < 1e-12);
        assert!((clip.source_offset - (0.125 + elapsed.rem_euclid(0.75))).abs() < 1e-12);
        assert!((clip.duration - (6.0 - elapsed)).abs() < 1e-12);
        assert!((clip.gain_points.points()[0].gain - gain).abs() < 1e-12);
    }
    assert_eq!(
        reader.runtime.publication_context(),
        publication,
        "audio queries must be read-only"
    );
}

fn bootstrap(f: &Fixture) -> (noon_runtime::PrefixPublisher, PrefixHarness) {
    let (publisher, runtime, receiver) = f
        .source
        .create_prefix_projection_with_resource_budget(f.resource_bytes)
        .unwrap();
    (publisher, PrefixHarness { runtime, receiver })
}

#[test]
fn audio_bootstrap_charges_pcm_and_every_clip_before_cloning_and_can_retry() {
    for clips in [1, 128] {
        let f = fixture(clips);
        let frame = f.source.frame().clone();
        let publication = f.source.publication_context();
        let strong = f.pcm.strong_count();
        for limit in [0, 32_000, f.resource_bytes - 1] {
            assert!(
                matches!(f.source.create_prefix_projection_with_resource_budget(limit),
                Err(PrefixError::ResourceBudgetExceeded { retained_bytes: 0, additional_bytes, limit_bytes })
                if additional_bytes == f.resource_bytes && limit_bytes == limit)
            );
            assert_eq!(f.source.frame(), &frame);
            assert_eq!(f.source.publication_context(), publication);
            assert_eq!(
                f.pcm.strong_count(),
                strong,
                "failed admission must not retain PCM"
            );
        }
        let (publisher, reader) = bootstrap(&f);
        assert_eq!(publisher.extracted_resource_bytes(), f.resource_bytes);
        assert_eq!(reader.receiver.retained_resource_bytes(), f.resource_bytes);
        assert_eq!(reader.frame().time, 0.0);
        assert_eq!(f.source.frame(), &frame);
        check_audio(&reader, &f.pcm, clips);
    }
}

#[test]
fn preparing_and_delivering_visual_revisions_does_not_move_reader_audio_time() {
    let mut f = fixture(1);
    let (mut publisher, mut reader) = bootstrap(&f);
    reader.seek(0.5).unwrap();
    reader.runtime.take_frame_changes();
    let frame = reader.frame().clone();
    let publication = reader.runtime.publication_context();
    let sound = observe(&reader.runtime);
    f.source
        .apply_execution_patch(&ExecutionPatch::SetTransform {
            object: ObjectId::new(100),
            transform: Transform2D {
                translation: Vec2::new(3.0, 0.0),
                ..Transform2D::IDENTITY
            },
        })
        .unwrap();
    f.source.advance_to(3.0).unwrap();
    let batch = f.source.prepare_prefix_update(&mut publisher).unwrap();
    assert_eq!(batch.stats().revisions, 1);
    assert_eq!(batch.stats().resource_payloads, 0);
    assert_eq!(
        batch.resource_bytes(),
        0,
        "fixed audio is charged only at bootstrap"
    );
    assert_eq!(publisher.extracted_resource_bytes(), f.resource_bytes);
    reader.append(batch).unwrap();
    assert_eq!(reader.frame(), &frame);
    assert_eq!(reader.runtime.publication_context(), publication);
    assert!(reader.runtime.take_frame_changes().is_empty());
    assert_eq!(observe(&reader.runtime), sound);
    reader.seek(2.5).unwrap();
    assert_eq!(reader.frame().objects[0].transform.translation.x, 3.0);
    check_audio(&reader, &f.pcm, 1);
    reader.seek(0.5).unwrap();
    assert_eq!(reader.frame().objects[0].transform.translation.x, 0.0);
    assert_eq!(observe(&reader.runtime), sound);
}

#[test]
fn open_frontier_stays_unavailable_until_terminal_adoption_and_terminal_audio_is_silent() {
    let mut f = fixture(1);
    let (mut publisher, mut reader) = bootstrap(&f);
    reader.seek(0.5).unwrap();
    let frame = reader.frame().clone();
    let sound = observe(&reader.runtime);
    for time in [1.0, 2.0] {
        assert!(matches!(
            reader.seek(time),
            Err(PrefixError::OutsideRange { .. })
        ));
        assert_eq!(reader.frame(), &frame);
        assert_eq!(observe(&reader.runtime), sound);
    }
    f.source.advance_to(6.25).unwrap();
    f.source.seal_replay().unwrap();
    let terminal = f.source.prepare_prefix_finish(&mut publisher).unwrap();
    assert!(terminal.is_terminal());
    assert_eq!(terminal.resource_bytes(), 0);
    assert_eq!(reader.end_time(), None);
    assert!(
        reader.seek(6.25).is_err(),
        "producer completion is not delivery"
    );
    reader.append(terminal).unwrap();
    assert_eq!(reader.frame(), &frame, "terminal adoption is not a seek");
    assert_eq!(observe(&reader.runtime), sound);
    assert_eq!(reader.end_time(), Some(6.25));
    reader.seek(6.25).unwrap();
    check_audio(&reader, &f.pcm, 1);
    assert!(reader.advance_to(6.5).is_err());
    for time in [6.0, 5.5, 3.5, 0.0, 0.5] {
        reader.seek(time).unwrap();
        check_audio(&reader, &f.pcm, 1);
        assert_eq!(reader.receiver.retained_resource_bytes(), f.resource_bytes);
    }
}

#[test]
fn dropped_source_and_publisher_leave_delivered_audio_valid_until_reader_release() {
    let mut f = fixture(1);
    let (mut publisher, mut reader) = bootstrap(&f);
    f.source.advance_to(4.0).unwrap();
    reader
        .append(f.source.prepare_prefix_update(&mut publisher).unwrap())
        .unwrap();
    let pcm = f.pcm.clone();
    drop(publisher);
    drop(f);
    assert!(pcm.upgrade().is_some());
    for time in [3.5, 0.0, 1.5] {
        reader.seek(time).unwrap();
        check_audio(&reader, &pcm, 1);
    }
    assert_eq!(
        reader.end_time(),
        None,
        "source release must not invent a terminal boundary"
    );
    assert!(reader.seek(4.0).is_err());
    drop(reader);
    assert!(
        pcm.upgrade().is_none(),
        "last reader must release the audio closure"
    );
}

#[test]
fn foreign_audio_batch_is_rejected_without_changing_frame_phase_or_resource_charge() {
    let mut first = fixture(1);
    let mut other = fixture(2);
    let (mut publisher, mut reader) = bootstrap(&first);
    let (mut foreign, _) = bootstrap(&other);
    reader.seek(0.5).unwrap();
    let frame = reader.frame().clone();
    let sound = observe(&reader.runtime);
    other.source.advance_to(2.0).unwrap();
    assert!(reader
        .append(other.source.prepare_prefix_update(&mut foreign).unwrap())
        .is_err());
    assert_eq!(reader.frame(), &frame);
    assert_eq!(observe(&reader.runtime), sound);
    assert_eq!(
        reader.receiver.retained_resource_bytes(),
        first.resource_bytes
    );
    first.source.advance_to(2.0).unwrap();
    reader
        .append(first.source.prepare_prefix_update(&mut publisher).unwrap())
        .unwrap();
    reader.seek(1.5).unwrap();
    check_audio(&reader, &first.pcm, 1);
}
