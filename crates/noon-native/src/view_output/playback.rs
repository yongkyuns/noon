//! Real retained GPU output across incremental-reader to original-source handoff.
use super::*;
use noon::integration::{ExportFrameOptions, ExportStop, FrameRate};
use noon::{
    ContinuationStep, LiveContinuation, LiveProgram, LiveSession, LiveSessionError, Mobject,
};
use noon_core::{HostCallbackId, SemanticMutationTransaction};
use std::{cell::RefCell, convert::Infallible, rc::Rc};
const CALLBACK: HostCallbackId = HostCallbackId::new(720);
struct Source {
    marker: Mobject,
    stage: u8,
}
impl LiveContinuation for Source {
    const EFFECTS: noon::ContinuationEffects = noon::ContinuationEffects::SceneOnly;
    type Error = LiveSessionError;
    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, Self::Error> {
        let step = self.stage;
        self.stage += 1;
        Ok(match step {
            0 => ContinuationStep::Await(live.wait_segment(1.0)?),
            1 => {
                let mut tx = SemanticMutationTransaction::new();
                tx.add_updater(self.marker.node_id(), CALLBACK, 1.0, None);
                live.apply(tx)?;
                ContinuationStep::Await(live.wait_segment(1.0)?)
            }
            2 => ContinuationStep::Finished,
            _ => panic!("source restarted"),
        })
    }
}
type Trace = Rc<RefCell<Vec<(f64, f64)>>>;
fn program() -> (LiveProgram<Source>, RustHostCallbackTable, Trace) {
    let mut scene = Scene::new();
    let mut marker = scene.circle(0.5).unwrap();
    marker.set_fill(0.1, 0.4, 1.0, 1.0).unwrap();
    marker.disable_stroke().unwrap();
    scene.add(&marker).unwrap();
    let trace = Trace::default();
    let recorded = trace.clone();
    let mut callbacks = RustHostCallbackTable::new();
    callbacks
        .insert(CALLBACK, move |ctx| {
            recorded.borrow_mut().push((ctx.time(), ctx.delta_time()));
            let mut transform = ctx.target_state().transform;
            transform.translation.x += 0.25; // Stateful: wrong invocation counts change the image.
            ctx.set_target_transform(transform)
        })
        .unwrap();
    (
        scene
            .into_live_program(Source { marker, stage: 0 })
            .unwrap(),
        callbacks,
        trace,
    )
}
#[test]
#[ignore = "requires a real native raster adapter; never qualified by the noop device"]
fn native_incremental_handoff_pixels_match_original_observed_source() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    eprintln!("playback handoff adapter: {:?}", adapter.get_info());
    assert_ne!(adapter.get_info().backend, wgpu::Backend::Noop);
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let evidence = std::env::var_os("NOON_PLAYBACK_EVIDENCE_DIR").map(std::path::PathBuf::from);
    if let Some(path) = &evidence {
        std::fs::create_dir_all(path).unwrap();
    }
    let mut report = Vec::new();
    for size in [[128, 96], [96, 128]] {
        let (mut reference, mut callbacks, reference_trace) = program();
        let mut frames = Vec::new();
        noon_export::capture_frames(
            &mut reference,
            &mut callbacks,
            ExportFrameOptions {
                frame_rate: FrameRate::new(4, 1).unwrap(),
                start_frame: 0,
                stop: ExportStop::SourceEnd,
                max_frames: 16,
                max_transitions_per_sample: 64,
                final_hold_seconds: 0.25,
            },
            noon_export::CaptureOptions::new(size[0], size[1]),
            |frame| {
                frames.push((frame.observation, frame.rgba.to_vec()));
                Ok::<_, Infallible>(())
            },
        )
        .unwrap();
        assert_eq!(frames.len(), 9);
        let (program, callbacks, trace) = program();
        let mut source = PlaybackExecutionSource::new(program, callbacks).unwrap();
        let initial = source.session().runtime_identity();
        let mut draw = Draw::new(&device, &queue);
        let mut output = AuthoredViewOutput::new(
            &adapter,
            &device,
            FORMAT,
            noon_render_wgpu::OutputTransfer::Direct,
        );
        let mut incarnation = initial;
        let mut maximum = 0;
        let mut changing = 0;
        for (index, (observation, expected)) in frames.iter().enumerate() {
            source
                .prepare_until(observation.requested_time + 1.0)
                .unwrap();
            let mut actual = None;
            for _ in 0..8 {
                source.advance_to(observation.requested_time).unwrap();
                assert!(
                    !source.buffering(),
                    "this exact source has one segment per demanded interval"
                );
                let endpoint = source.pending_endpoint();
                draw.camera = source.camera().unwrap();
                let bounds = draw
                    .camera
                    .viewport_bounds(size[0] as f32 / size[1] as f32)
                    .unwrap();
                draw.visible = Some(source.query_viewports(&[bounds]).object_indices().to_vec());
                let current = source.session().runtime_identity();
                let changed_source = current != incarnation;
                if changed_source {
                    let format = if draw.profile == ViewColorProfile::LinearHdr {
                        wgpu::TextureFormat::Rgba16Float
                    } else {
                        FORMAT
                    };
                    reset_renderer_source(
                        &device,
                        &queue,
                        format,
                        size,
                        &mut draw.preparer,
                        &mut draw.renderer,
                        &mut draw.text,
                    );
                }
                incarnation = current;
                let mut publication = source.take_renderer_publication();
                if changed_source {
                    publication.invalidate_all();
                }
                let context = publication.context();
                let pixels = draw
                    .frame(
                        &device,
                        &queue,
                        &mut output,
                        &publication,
                        size,
                        &OverlayGpuState::default(),
                        false,
                    )
                    .0;
                if endpoint.is_some() {
                    source.admit_retained_publication(context).unwrap();
                } else {
                    actual = Some(pixels);
                    break;
                }
            }
            let actual = actual.expect("bounded source observation settles");
            if index == 0 {
                assert!(
                    source.source_active(),
                    "first GPU picture precedes source completion"
                );
            }
            assert_eq!(source.frame_time(), observation.published_time);
            assert_eq!(
                source.session().runtime_identity() == initial,
                observation.requested_time < 1.0
            );
            assert!(
                actual
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .filter(|pixel| pixel[2] > 100)
                    .count()
                    > 50,
                "positive interior ink"
            );
            assert_eq!(actual.len(), expected.len());
            let error = actual
                .iter()
                .zip(expected)
                .map(|(a, b)| a.abs_diff(*b))
                .max()
                .unwrap();
            maximum = maximum.max(error);
            assert!(
                error <= 2,
                "{size:?} t={} native/original error {error}",
                observation.published_time
            );
            changing += usize::from(expected != &frames[0].1);
            if let Some(path) = &evidence {
                let name = format!("{}x{}-{index}", size[0], size[1]);
                std::fs::write(path.join(format!("{name}-native.rgba")), &actual).unwrap();
                std::fs::write(path.join(format!("{name}-reference.rgba")), expected).unwrap();
            }
        }
        assert_eq!(*trace.borrow(), *reference_trace.borrow());
        assert_eq!(trace.borrow().len(), 5);
        assert!(changing >= 4);
        source.retire_execution_source().unwrap();
        report.push(serde_json::json!({"size":size,"frames":frames.len(),"changing_frames":changing,"first_gpu_frame_source_active":true,"maximum_channel_error":maximum,"callback_trace":*trace.borrow()}));
    }
    if let Some(path) = evidence {
        std::fs::write(
            path.join("report.json"),
            serde_json::to_vec_pretty(&report).unwrap(),
        )
        .unwrap();
    }
    eprintln!(
        "playback handoff pixels: {}",
        serde_json::to_string(&report).unwrap()
    );
}
