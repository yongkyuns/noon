//! Completed history uses one original reader, exact samples and immutable PCM.
#![cfg(feature = "prepared-prefix")]

use std::{
    cell::Cell,
    rc::Rc,
    sync::{Arc, Weak},
};

use noon::integration::{
    CompletedPrefixExport, CompletedPrefixExportError, ExportFrameOptions, ExportFramePolicy,
    ExportFramePolicyError, ExportFramePolicyStatus, ExportSampleKind, ExportStop, FrameRate,
    MAX_AUDIO_MIX_BLOCK_FRAMES,
};
use noon::{
    AnimationOptions, AudioClipOptions, AudioResource, ContinuationStep, LiveContinuation,
    LiveSession, LiveSessionError, Mobject, PrefixError, PrefixReader, PreparedSource,
    PreparedSourceStatus, RateFunction, Scene,
};

struct Source {
    target: Mobject,
    calls: Rc<Cell<usize>>,
    drops: Rc<Cell<usize>>,
}
impl Drop for Source {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
    }
}
impl LiveContinuation for Source {
    type Error = LiveSessionError;
    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, Self::Error> {
        let step = self.calls.get();
        self.calls.set(step + 1);
        match step {
            0 => {
                let target = live.target_editor(&self.target)?;
                live.set_translation(&target, 2.0, 0.0)?;
                live.declare_and_activate_transform_to(
                    &self.target,
                    &target,
                    AnimationOptions::new()
                        .run_time(1.0)
                        .rate_func(RateFunction::Linear),
                )
                .map(ContinuationStep::Await)
            }
            1 => live.wait_segment(2.0).map(ContinuationStep::Await),
            2 => {
                live.set_fill(&self.target, 0.0, 1.0, 0.0, 1.0)?;
                Ok(ContinuationStep::Finished)
            }
            _ => panic!("export cannot invoke source again"),
        }
    }
}

struct Fixture {
    source: PreparedSource<Source>,
    calls: Rc<Cell<usize>>,
    drops: Rc<Cell<usize>>,
    pcm: Weak<[f32]>,
}
fn fixture() -> Fixture {
    fixture_with_clip(AudioClipOptions {
        start_time: 0.25,
        source_start_frame: 1000,
        source_end_frame: Some(7000),
        duration: Some(8.0),
        looped: true,
        gain: 0.5,
        fade_in: 0.5,
        fade_out: 1.0,
    })
}
fn fixture_with_clip(clip: AudioClipOptions) -> Fixture {
    let mut scene = Scene::new();
    let mut target = scene.circle(0.5).unwrap();
    target.set_fill(0.0, 0.0, 1.0, 1.0).unwrap();
    scene.add(&target).unwrap();
    let pcm: Arc<[f32]> = (0..8000)
        .map(|frame| frame as f32 / 32_000.0)
        .collect::<Vec<_>>()
        .into();
    let weak = Arc::downgrade(&pcm);
    scene
        .audio(
            AudioResource::from_interleaved(8000, 1, pcm, 32_000).unwrap(),
            clip,
        )
        .unwrap();
    let calls = Rc::new(Cell::new(0));
    let drops = Rc::new(Cell::new(0));
    let source = scene
        .into_prepared_source(
            Source {
                target,
                calls: calls.clone(),
                drops: drops.clone(),
            },
            Default::default(),
        )
        .unwrap();
    Fixture {
        source,
        calls,
        drops,
        pcm: weak,
    }
}
fn deliver(source: &mut PreparedSource<Source>, reader: &mut PrefixReader) {
    let batch = source.pending_batch().unwrap();
    reader
        .reserve_object_capacity(batch.required_object_capacity())
        .unwrap();
    reader.preflight_append(batch).unwrap();
    reader.append(source.take_batch().unwrap()).unwrap();
}
fn complete(fixture: &mut Fixture) -> PrefixReader {
    assert_eq!(
        fixture.source.prepare_next().unwrap(),
        PreparedSourceStatus::ReaderReady
    );
    let mut reader = fixture.source.take_reader().unwrap();
    while fixture.source.prepare_next().unwrap() == PreparedSourceStatus::BatchReady {
        deliver(&mut fixture.source, &mut reader);
    }
    assert_eq!(fixture.source.status(), PreparedSourceStatus::Finished);
    assert_eq!(reader.end_time(), Some(3.0));
    reader
}
fn options() -> ExportFrameOptions {
    ExportFrameOptions {
        frame_rate: FrameRate::new(4, 1).unwrap(),
        start_frame: 0,
        stop: ExportStop::SourceEnd,
        max_frames: 32,
        max_transitions_per_sample: 64,
        final_hold_seconds: 0.0,
    }
}

#[test]
fn producer_completion_without_terminal_adoption_does_not_admit_export() {
    let mut fixture = fixture();
    fixture.source.prepare_next().unwrap();
    let mut reader = fixture.source.take_reader().unwrap();
    for _ in 0..2 {
        assert!(matches!(
            CompletedPrefixExport::new(&reader, options()),
            Err(CompletedPrefixExportError::Reader(PrefixError::NotReady))
        ));
        fixture.source.prepare_next().unwrap();
        if !fixture.source.pending_batch().unwrap().is_terminal() {
            deliver(&mut fixture.source, &mut reader);
        }
    }
    assert_eq!(
        fixture.drops.get(),
        1,
        "source is gone but its terminal batch is pending"
    );
    assert!(matches!(
        CompletedPrefixExport::new(&reader, options()),
        Err(CompletedPrefixExportError::Reader(PrefixError::NotReady))
    ));
    deliver(&mut fixture.source, &mut reader);
    assert!(CompletedPrefixExport::new(&reader, options()).is_ok());
    reader.seek(0.5).unwrap();
    assert!(matches!(
        CompletedPrefixExport::new(&reader, options()),
        Err(CompletedPrefixExportError::NotAtStart)
    ));
}

#[test]
fn exact_history_samples_survive_source_drop_and_cannot_skip_acknowledgement() {
    let mut fixture = fixture();
    let mut reader = complete(&mut fixture);
    let runtime = reader.runtime_identity();
    let calls = fixture.calls.clone();
    let weak = fixture.pcm.clone();
    assert_eq!(fixture.drops.get(), 1);
    drop(fixture);
    let mut export = CompletedPrefixExport::new(&reader, options()).unwrap();
    let owner = export.owner();
    let mut frames = 0;
    let mut execution_changes = 0;
    let mut previous = owner;
    loop {
        match export.advance(&mut reader).unwrap() {
            ExportFramePolicyStatus::SampleReady(sample) => {
                assert_eq!(
                    export.advance(&mut reader).unwrap(),
                    ExportFramePolicyStatus::SampleReady(sample)
                );
                assert_eq!(reader.runtime_identity(), runtime);
                assert_eq!(sample.observation.publication, reader.publication_context());
                assert_eq!(
                    reader.frame().objects[0].transform.translation.x,
                    (reader.frame().time * 2.0).min(2.0) as f32
                );
                assert_eq!(
                    reader.frame().objects[0].style.fill,
                    Some(if reader.frame().time == 3.0 {
                        noon_core::Color::rgb(0.0, 1.0, 0.0)
                    } else {
                        noon_core::Color::rgb(0.0, 0.0, 1.0)
                    }),
                    "the final same-time source edit belongs to the terminal sample"
                );
                let current = reader.publication_context();
                assert_eq!(current.scene_revision(), owner.scene_revision());
                execution_changes +=
                    usize::from(current.execution_revision() != previous.execution_revision());
                previous = current;
                let mut wrong = sample;
                wrong.observation.published_time += 0.01;
                assert!(export.acknowledge_sample(&reader, wrong).is_err());
                assert_eq!(
                    export.status(&reader).unwrap(),
                    ExportFramePolicyStatus::SampleReady(sample)
                );
                assert_eq!(reader.take_renderer_publication().context(), current);
                frames += u64::from(sample.kind == ExportSampleKind::Output);
                export.acknowledge_sample(&reader, sample).unwrap();
            }
            ExportFramePolicyStatus::Complete(summary) => {
                assert_eq!(summary, export.planned());
                assert_eq!(summary.frames, 12);
                break;
            }
            ExportFramePolicyStatus::NeedsSample(_) => panic!("advance must settle"),
        }
    }
    assert_eq!(frames, 12);
    assert!(
        execution_changes >= 2,
        "samples must expose actual exchanged history revisions"
    );
    assert_eq!(calls.get(), 3, "sampling never resumes source");
    assert!(weak.upgrade().is_some());
    drop(reader);
    assert!(
        weak.upgrade().is_none(),
        "export metadata must not retain the reader's PCM"
    );
    drop(export);
}

#[test]
fn foreign_reader_external_seek_and_cancellation_require_a_new_export() {
    let mut a = fixture();
    let mut b = fixture();
    let mut reader = complete(&mut a);
    let other = complete(&mut b);
    let mut export = CompletedPrefixExport::new(&reader, options()).unwrap();
    assert!(matches!(
        export.status(&other),
        Err(CompletedPrefixExportError::OwnerChanged)
    ));
    let sample = match export.advance(&mut reader).unwrap() {
        ExportFramePolicyStatus::SampleReady(sample) => sample,
        _ => unreachable!(),
    };
    reader.seek(0.5).unwrap();
    reader.seek(0.0).unwrap();
    assert!(matches!(
        export.acknowledge_sample(&reader, sample),
        Err(CompletedPrefixExportError::OwnerChanged)
    ));
    export.cancel();
    let mut restarted = CompletedPrefixExport::new(&reader, options()).unwrap();
    assert!(matches!(
        restarted.advance(&mut reader).unwrap(),
        ExportFramePolicyStatus::SampleReady(_)
    ));
    restarted.cancel();
    assert!(matches!(
        restarted.status(&reader),
        Err(CompletedPrefixExportError::Sampling(
            ExportFramePolicyError::Inactive
        ))
    ));
    let mut unchanged = vec![0.75; 8];
    assert!(restarted
        .mix_audio_block(&reader, 0, &mut unchanged)
        .is_err());
    assert_eq!(unchanged, [0.75; 8]);
    assert_eq!(a.calls.get(), 3);
}

fn mix(export: &CompletedPrefixExport, reader: &PrefixReader, block: usize) -> Vec<f32> {
    let grid = export.audio_grid().unwrap();
    let mut samples = vec![0.0; grid.frames() as usize * 2];
    let mut first = 0;
    while first < grid.frames() {
        let count = (grid.frames() - first).min(block as u64) as usize;
        export
            .mix_audio_block(
                reader,
                first,
                &mut samples[first as usize * 2..(first as usize + count) * 2],
            )
            .unwrap();
        first += count as u64;
    }
    samples
}

fn assert_same_pcm(left: &[f32], right: &[f32]) {
    assert_eq!(left.len(), right.len());
    let mismatch = left
        .iter()
        .zip(right)
        .enumerate()
        .find(|(_, (left, right))| left.to_bits() != right.to_bits())
        .map(|(sample, (left, right))| (sample, *left, *right));
    assert_eq!(
        mismatch, None,
        "PCM coordinates must preserve every sample bit"
    );
}

#[test]
fn pcm_is_block_independent_keeps_absolute_crop_phase_and_silences_terminal_hold() {
    let mut fixture = fixture();
    let reader = complete(&mut fixture);
    let publication = reader.publication_context();
    let mut options = options();
    options.final_hold_seconds = 0.5;
    let export = CompletedPrefixExport::new(&reader, options).unwrap();
    let full = mix(&export, &reader, MAX_AUDIO_MIX_BLOCK_FRAMES);
    assert_same_pcm(&full, &mix(&export, &reader, 997));
    assert!(full[..12_000 * 2].iter().all(|sample| *sample == 0.0));
    assert!(full[48_000..144_000 * 2]
        .iter()
        .any(|sample| *sample != 0.0));
    assert!(full[144_000 * 2..].iter().all(|sample| *sample == 0.0));
    assert_eq!(export.audio_grid().unwrap().frames(), 168_000);
    assert_eq!(export.audio_grid().unwrap().mixed_frames(), 144_000);
    options.start_frame = 2;
    let cropped = CompletedPrefixExport::new(&reader, options).unwrap();
    assert_same_pcm(&mix(&cropped, &reader, 137), &full[24_000 * 2..]);
    options.start_frame = 12;
    let hold_only = CompletedPrefixExport::new(&reader, options).unwrap();
    assert!(mix(&hold_only, &reader, 4096)
        .iter()
        .all(|sample| *sample == 0.0));
    assert_eq!(reader.frame().time, 0.0);
    assert_eq!(reader.publication_context(), publication);
    let mut output = [0.75; 8];
    assert!(export
        .mix_audio_block(&reader, u64::MAX, &mut output)
        .is_err());
    assert_eq!(output, [0.75; 8]);
    assert!(export
        .mix_audio_block(&reader, 0, &mut output[..3])
        .is_err());
    assert_eq!(output, [0.75; 8]);
}

#[test]
fn preparation_limit_is_rational_and_includes_cropped_prefix() {
    let mut config = options();
    config.frame_rate = FrameRate::new(30_000, 1001).unwrap();
    config.start_frame = 700;
    config.max_frames = 1039;
    let policy = ExportFramePolicy::new(config).unwrap();
    assert_eq!(
        policy.source_preparation_limit().unwrap(),
        1039.0 * 1001.0 / 30_000.0
    );
    let mut fixture = fixture();
    let reader = complete(&mut fixture);
    config = options();
    config.max_frames = 11;
    config.stop = ExportStop::FrameCount(1);
    assert!(matches!(
        CompletedPrefixExport::new(&reader, config),
        Err(CompletedPrefixExportError::Sampling(
            ExportFramePolicyError::FrameLimit
        ))
    ));
}

#[test]
fn callback_and_causal_sources_cannot_create_a_completed_export_reader() {
    for callback in [false, true] {
        let mut scene = Scene::new();
        let target = scene.circle(0.5).unwrap();
        scene.add(&target).unwrap();
        if callback {
            let mut transaction = noon_core::SemanticMutationTransaction::new();
            transaction.add_updater(
                target.node_id(),
                noon_core::HostCallbackId::new(9),
                0.0,
                None,
            );
            transaction
                .apply(&mut scene.integration_store().borrow_mut())
                .unwrap();
        } else {
            let signal = scene.control_signal("opacity", 1.0).unwrap();
            scene.bind_opacity(&target, &signal).unwrap();
        }
        let calls = Rc::new(Cell::new(0));
        let drops = Rc::new(Cell::new(0));
        let result = scene.into_prepared_source(
            Source {
                target,
                calls: calls.clone(),
                drops: drops.clone(),
            },
            Default::default(),
        );
        assert!(result.is_err());
        assert_eq!(calls.get(), 0);
        assert_eq!(drops.get(), 1);
    }
}

#[test]
fn inspection_navigation_cannot_change_an_admitted_export_view() {
    use noon_core::Vec2;
    let mut fixture = fixture();
    let mut reader = complete(&mut fixture);
    let export = CompletedPrefixExport::new(&reader, options()).unwrap();
    let view = reader
        .inspection_pointer_view(0, Vec2::new(200.0, 200.0))
        .unwrap();
    let displayed = reader.capture_pointer_frame(view).unwrap();
    assert!(reader
        .zoom_inspection_view(&displayed, view, Vec2::new(100.0, 100.0), 0.5)
        .unwrap());
    assert!(matches!(
        export.status(&reader),
        Err(CompletedPrefixExportError::OwnerChanged)
    ));
    assert!(matches!(
        CompletedPrefixExport::new(&reader, options()),
        Err(CompletedPrefixExportError::ViewerState)
    ));
    reader.reset_inspection_view().unwrap();
    assert!(CompletedPrefixExport::new(&reader, options()).is_ok());
    assert!(
        matches!(
            export.status(&reader),
            Err(CompletedPrefixExportError::OwnerChanged)
        ),
        "returning to the old camera cannot revive the old admission"
    );
}

#[test]
fn active_reader_effect_is_explicitly_rejected_and_cannot_relabel_an_old_sample() {
    let mut fixture = fixture();
    let mut reader = complete(&mut fixture);
    let export = CompletedPrefixExport::new(&reader, options()).unwrap();
    let object = reader.frame().objects[0].id;
    let effect = reader
        .prepare_click_indicate(
            object,
            noon_core::SemanticClickIndicate::new(1.25, noon_core::Color::YELLOW, 0.5),
        )
        .unwrap()
        .unwrap();
    reader.start_transient_animation(effect).unwrap();
    reader.advance_interactions(1.0).unwrap();
    assert!(reader.interactions_active());
    assert!(matches!(
        CompletedPrefixExport::new(&reader, options()),
        Err(CompletedPrefixExportError::ViewerState)
    ));
    assert!(export.status(&reader).is_err());
    reader.seek(0.0).unwrap();
    assert!(!reader.interactions_active());
    assert!(CompletedPrefixExport::new(&reader, options()).is_ok());
    assert!(matches!(
        export.status(&reader),
        Err(CompletedPrefixExportError::OwnerChanged)
    ));
}

#[test]
fn background_pointer_press_is_active_input_even_without_selection_or_effect() {
    use noon_core::{
        NativeInputModifiers, NativePointerCancellation, NativePointerId, NativePointerInput,
        NativePointerInputKind, Vec2,
    };
    let mut fixture = fixture();
    let mut reader = complete(&mut fixture);
    reader.enable_pointer_fill_selection(4.0).unwrap();
    let token = reader
        .configure_native_pointer_input(
            NativePointerId {
                source: 1,
                pointer: 2,
            },
            0,
        )
        .unwrap();
    let view = reader
        .inspection_pointer_view(0, Vec2::new(200.0, 200.0))
        .unwrap();
    let displayed = reader.capture_pointer_frame(view).unwrap();
    reader
        .submit_native_pointer_input(
            &token,
            NativePointerInput::new(
                1,
                token.pointer(),
                token.context(),
                NativeInputModifiers::default(),
                NativePointerInputKind::Press {
                    position: displayed.position(Vec2::new(1.0, 1.0)).unwrap(),
                    button: 0,
                },
            ),
        )
        .unwrap();
    assert!(reader.pointer_gesture_active());
    assert!(reader.selected_pointer_target().is_none());
    assert!(!reader.interactions_active());
    assert!(matches!(
        CompletedPrefixExport::new(&reader, options()),
        Err(CompletedPrefixExportError::ViewerState)
    ));
    let token = reader.native_pointer_input_token().unwrap();
    reader
        .submit_native_pointer_input(
            &token,
            NativePointerInput::new(
                2,
                token.pointer(),
                token.context(),
                NativeInputModifiers::default(),
                NativePointerInputKind::Cancel(NativePointerCancellation::CaptureLost),
            ),
        )
        .unwrap();
    assert!(!reader.pointer_gesture_active());
    assert!(CompletedPrefixExport::new(&reader, options()).is_ok());
}

#[test]
fn rational_frame_700_crop_uses_the_identical_absolute_pcm_sample_coordinates() {
    struct LongWait(bool);
    impl LiveContinuation for LongWait {
        type Error = LiveSessionError;
        fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, Self::Error> {
            if self.0 {
                Ok(ContinuationStep::Finished)
            } else {
                self.0 = true;
                live.wait_segment(34.65).map(ContinuationStep::Await)
            }
        }
    }
    let mut scene = Scene::new();
    // Linear interpolation crosses zero at known half-frame coordinates, so an
    // origin-rounding discrepancy cannot hide inside a broad audio tolerance.
    scene
        .audio(
            AudioResource::from_interleaved(8000, 1, vec![0.25, -0.25], 8).unwrap(),
            AudioClipOptions {
                duration: Some(40.0),
                looped: true,
                ..Default::default()
            },
        )
        .unwrap();
    let mut source = scene
        .into_prepared_source(LongWait(false), Default::default())
        .unwrap();
    assert_eq!(
        source.prepare_next().unwrap(),
        PreparedSourceStatus::ReaderReady
    );
    let mut reader = source.take_reader().unwrap();
    assert_eq!(
        source.prepare_next().unwrap(),
        PreparedSourceStatus::BatchReady
    );
    let batch = source.pending_batch().unwrap();
    reader
        .reserve_object_capacity(batch.required_object_capacity())
        .unwrap();
    reader.append(source.take_batch().unwrap()).unwrap();
    drop(source);
    let mut config = options();
    config.frame_rate = FrameRate::new(30_000, 1001).unwrap();
    config.max_frames = 1100;
    let full = CompletedPrefixExport::new(&reader, config).unwrap();
    config.start_frame = 700;
    let crop = CompletedPrefixExport::new(&reader, config).unwrap();
    assert_eq!(full.audio_grid().unwrap().frames(), 1_664_063);
    assert_eq!(full.audio_grid().unwrap().mixed_frames(), 1_663_200);
    let cropped_grid = crop.audio_grid().unwrap();
    assert_eq!(cropped_grid.frames(), 542_943);
    assert_eq!(cropped_grid.mixed_frames(), 542_080);
    let mut from_full = vec![0.0; MAX_AUDIO_MIX_BLOCK_FRAMES * 2];
    let mut from_crop = vec![0.0; from_full.len()];
    full.mix_audio_block(&reader, 1_121_120, &mut from_full)
        .unwrap();
    crop.mix_audio_block(&reader, 0, &mut from_crop).unwrap();
    let mismatch = from_full
        .iter()
        .zip(&from_crop)
        .enumerate()
        .find(|(_, (left, right))| left.to_bits() != right.to_bits())
        .map(|(sample, (left, right))| (sample, *left, *right));
    assert_eq!(
        mismatch, None,
        "frame700 has the same absolute 48kHz grid as the full movie"
    );
    let mut tail = vec![0.75; 1024 * 2];
    crop.mix_audio_block(&reader, cropped_grid.frames() - 1024, &mut tail)
        .unwrap();
    assert!(tail[..161 * 2].iter().any(|sample| *sample != 0.0));
    assert!(tail[161 * 2..].iter().all(|sample| *sample == 0.0));
}

#[test]
fn shorter_audio_does_not_shorten_completed_history_or_remove_its_silent_wait() {
    let mut fixture = fixture_with_clip(AudioClipOptions {
        duration: Some(1.0),
        gain: 0.5,
        fade_in: 0.25,
        fade_out: 0.25,
        ..Default::default()
    });
    let mut reader = complete(&mut fixture);
    let mut export = CompletedPrefixExport::new(&reader, options()).unwrap();
    assert_eq!(export.source_end(), 3.0);
    assert_eq!(export.planned().frames, 12);
    assert_eq!(export.planned().end_time, 3.0);
    let pcm = mix(&export, &reader, 4096);
    assert_eq!(pcm.len(), 144_000 * 2);
    assert!(pcm[..48_000 * 2].iter().any(|sample| *sample != 0.0));
    assert!(pcm[48_000 * 2..].iter().all(|sample| *sample == 0.0));
    let mut frames = 0;
    loop {
        match export.advance(&mut reader).unwrap() {
            ExportFramePolicyStatus::SampleReady(sample) => {
                assert_eq!(
                    reader.take_renderer_publication().context(),
                    sample.observation.publication
                );
                frames += usize::from(sample.frame.is_some());
                export.acknowledge_sample(&reader, sample).unwrap();
            }
            ExportFramePolicyStatus::Complete(_) => break,
            ExportFramePolicyStatus::NeedsSample(_) => unreachable!(),
        }
    }
    assert_eq!(frames, 12);
    assert_eq!(reader.frame().time, 3.0);
    assert_eq!(fixture.calls.get(), 3);
}
