use crate::integration::{
    CompletedPrefixExport, CompletedPrefixExportError, ExportFrameOptions, ExportFramePolicyStatus,
    ExportSampleKind, ExportStop, FrameRate, ReplayLimits,
};
use crate::{
    AnimationOptions, AudioClipOptions, AudioResource, PrefixError, PrefixReader, RateFunction,
    Scene,
};

fn history() -> PrefixReader {
    let mut scene = Scene::new();
    let shape = scene.circle(1.0).unwrap();
    let mut target = scene.circle(1.0).unwrap();
    target.set_translation(2.0, 0.0).unwrap();
    scene.add(&shape).unwrap();
    scene
        .audio(
            AudioResource::from_interleaved(8_000, 1, vec![0.25; 8_000], 32_000).unwrap(),
            AudioClipOptions {
                looped: true,
                duration: Some(4.0),
                ..Default::default()
            },
        )
        .unwrap();
    let mut source = scene.execution_session().unwrap();
    source
        .begin_replay_retention(ReplayLimits::default())
        .unwrap();
    let play = scene
        .live(&mut source)
        .declare_and_activate_transform_to(
            &shape,
            &target,
            AnimationOptions::new()
                .run_time(1.0)
                .rate_func(RateFunction::Linear),
        )
        .unwrap();
    source.advance_to(play.end_time()).unwrap();
    scene.live(&mut source).complete_segment(play).unwrap();
    let (mut publisher, mut reader) = source.create_prepared_prefix(play).unwrap();

    let wait = source.wait_segment(0.75).unwrap();
    source.advance_to(wait.end_time()).unwrap();
    scene.live(&mut source).complete_segment(wait).unwrap();
    source.seal_replay().unwrap();
    let terminal = source.finish_prepared_prefix(wait, &mut publisher).unwrap();
    drop(source);
    assert!(matches!(
        CompletedPrefixExport::new(&reader, options()),
        Err(CompletedPrefixExportError::Reader(PrefixError::NotReady))
    ));
    reader.reserve_append_capacity(&terminal).unwrap();
    reader.append(terminal).unwrap();
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
fn adopted_history_exports_exact_publications_after_source_drop() {
    let mut reader = history();
    let mut export = CompletedPrefixExport::new(&reader, options()).unwrap();
    let runtime = reader.runtime_identity();
    let mut output_frames = 0;
    let summary = loop {
        match export.advance(&mut reader).unwrap() {
            ExportFramePolicyStatus::SampleReady(sample) => {
                assert_eq!(reader.runtime_identity(), runtime);
                assert_eq!(reader.frame().time, sample.observation.published_time);
                assert_eq!(
                    reader.frame().objects[0].transform.translation.x,
                    (reader.frame().time * 2.0).min(2.0) as f32,
                );
                assert_eq!(
                    reader.take_renderer_publication().context(),
                    sample.observation.publication,
                );
                let mut wrong = sample;
                wrong.observation.requested_time += 0.01;
                assert!(export.acknowledge_sample(&reader, wrong).is_err());
                assert_eq!(
                    export.advance(&mut reader).unwrap(),
                    ExportFramePolicyStatus::SampleReady(sample),
                );
                output_frames += u64::from(sample.kind == ExportSampleKind::Output);
                export.acknowledge_sample(&reader, sample).unwrap();
            }
            ExportFramePolicyStatus::Complete(summary) => break summary,
            status => panic!("unexpected status {status:?}"),
        }
    };
    assert_eq!(summary, export.planned());
    assert_eq!(summary.frames, 7);
    assert_eq!(output_frames, summary.frames);
}

#[test]
fn cropped_pcm_keeps_its_phase_and_silences_the_terminal_hold() {
    let reader = history();
    let full = CompletedPrefixExport::new(&reader, options()).unwrap();
    let crop = CompletedPrefixExport::new(
        &reader,
        ExportFrameOptions {
            start_frame: 1,
            final_hold_seconds: 0.5,
            ..options()
        },
    )
    .unwrap();
    let publication = reader.publication_context();
    let grid = crop.audio_grid().unwrap();
    assert_eq!(grid.scene_start(), 0.25);
    assert_eq!(grid.frames(), 96_000);
    assert_eq!(grid.mixed_frames(), 72_000);
    let mut before = [0.0; 4];
    let mut after = [-1.0; 8];
    full.mix_audio_block(&reader, 83_998, &mut before).unwrap();
    crop.mix_audio_block(&reader, 71_998, &mut after).unwrap();
    assert_eq!(before, [0.25; 4]);
    assert_eq!(after[..4], before);
    assert_eq!(after[4..], [0.0; 4]);
    assert_eq!(reader.publication_context(), publication);
    assert_eq!(reader.frame().time, 0.0);
}

#[test]
fn a_reader_seek_invalidates_a_pending_export_receipt() {
    let mut reader = history();
    let mut export = CompletedPrefixExport::new(&reader, options()).unwrap();
    let ExportFramePolicyStatus::SampleReady(sample) = export.advance(&mut reader).unwrap() else {
        panic!("first sample must be ready");
    };
    reader.seek(0.5).unwrap();
    assert!(matches!(
        CompletedPrefixExport::new(&reader, options()),
        Err(CompletedPrefixExportError::NotAtStart)
    ));
    reader.seek(0.0).unwrap();
    assert!(matches!(
        export.acknowledge_sample(&reader, sample),
        Err(CompletedPrefixExportError::OwnerChanged)
    ));
}

#[test]
fn off_grid_stop_before_source_end_matches_the_output_plan() {
    let mut reader = history();
    let mut export = CompletedPrefixExport::new(
        &reader,
        ExportFrameOptions {
            stop: ExportStop::EndTime(1.625),
            ..options()
        },
    )
    .unwrap();
    while let ExportFramePolicyStatus::SampleReady(sample) = export.advance(&mut reader).unwrap() {
        reader.take_renderer_publication();
        export.acknowledge_sample(&reader, sample).unwrap();
    }
    assert_eq!(reader.frame().time, 1.625);
    assert_eq!(export.planned().source_end, None);
    assert_eq!(
        export.status(&reader).unwrap(),
        ExportFramePolicyStatus::Complete(export.planned()),
    );
}
