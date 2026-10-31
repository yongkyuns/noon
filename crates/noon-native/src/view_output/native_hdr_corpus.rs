//! Normal native source/presentation versus normal offscreen export at the same
//! authored observations. Operator oracles remain in their existing qualifiers.
use super::*;
use noon::{
    integration::{ExportFrameOptions, ExportStop, FrameRate},
    LiveContinuation, LiveProgram, RustHostCallbackTable,
};
use std::convert::Infallible;

fn compare_program<C: LiveContinuation + 'static>(
    name: &str,
    factory: impl Fn() -> Result<LiveProgram<C>, String>,
    adapter: &wgpu::Adapter,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> serde_json::Value
where
    C::Error: std::fmt::Display + std::fmt::Debug,
{
    let mut views = Vec::new();
    for size in [[320, 180], [240, 160]] {
        let mut frames = Vec::new();
        noon_export::capture_frames(
            &mut factory().unwrap(),
            &mut RustHostCallbackTable::new(),
            ExportFrameOptions {
                frame_rate: FrameRate::new(4, 1).unwrap(),
                start_frame: 0,
                stop: ExportStop::SourceEnd,
                max_frames: 64,
                max_transitions_per_sample: 64,
                final_hold_seconds: 0.0,
            },
            noon_export::CaptureOptions::new(size[0], size[1]),
            |frame| {
                frames.push((frame.observation, frame.rgba.to_vec()));
                Ok::<_, Infallible>(())
            },
        )
        .unwrap();
        assert!(
            frames.len() >= 4,
            "{name}: source must provide real temporal coverage"
        );
        let mut source =
            LiveProgramExecutionSource::new(factory().unwrap(), RustHostCallbackTable::new())
                .unwrap();
        let mut draw = Draw::new(device, queue);
        let mut output = AuthoredViewOutput::new(
            adapter,
            device,
            FORMAT,
            noon_render_wgpu::OutputTransfer::Direct,
        );
        let mut prepared_modules = None;
        let mut maximum = 0;
        let mut changing_frames = 0;
        let first = &frames[0].1;
        for (observation, expected) in &frames {
            let mut observed = None;
            for _ in 0..64 {
                source.advance_to(observation.requested_time).unwrap();
                if let Some(programs) = source.pending_effect_programs() {
                    let request = draw.renderer.effect_module_preparation(device, programs);
                    let prepared = pollster::block_on(request.prepare()).unwrap();
                    draw.renderer
                        .cache_effect_programs(device, &prepared)
                        .unwrap();
                    source.publish_prepared_effects().unwrap();
                    prepared_modules = Some(prepared);
                    continue;
                }
                assert!(source.inset_2d_views().unwrap().is_empty());
                draw.camera = source.camera().unwrap();
                let bounds = draw
                    .camera
                    .viewport_bounds(size[0] as f32 / size[1] as f32)
                    .unwrap();
                draw.visible = Some(source.query_viewports(&[bounds]).object_indices().to_vec());
                let endpoint = source.pending_endpoint();
                let publication = source.take_renderer_publication();
                let context = publication.context();
                let pixels = draw
                    .frame(
                        device,
                        queue,
                        &mut output,
                        &publication,
                        size,
                        &OverlayGpuState::default(),
                        false,
                    )
                    .0;
                drop(prepared_modules.take());
                if endpoint.is_some() {
                    source.admit_retained_publication(context).unwrap();
                    continue;
                }
                assert_eq!(
                    source.frame_time(),
                    observation.published_time,
                    "{name}: normal hosts observe the same authored time"
                );
                observed = Some(pixels);
                break;
            }
            let pixels =
                observed.expect("native host must settle within its existing endpoint bound");
            let error = pixels
                .iter()
                .zip(expected)
                .map(|(a, b)| a.abs_diff(*b))
                .max()
                .unwrap();
            maximum = maximum.max(error);
            assert_eq!(pixels.len(), expected.len());
            assert!(
                error <= 2,
                "{name} {size:?} t={}: native/capture error {error}",
                observation.published_time
            );
            changing_frames += usize::from(expected != first);
        }
        drop(prepared_modules);
        assert!(
            changing_frames > 0,
            "{name}: blank/static agreement cannot qualify animated source"
        );
        source.retire_execution_source().unwrap();
        eprintln!(
            "native/capture {name} {size:?}: {} frames, max error {maximum}",
            frames.len()
        );
        views.push(serde_json::json!({"size":size,"frames":frames.len(),"changing_frames":changing_frames,"maximum_error":maximum}));
    }
    serde_json::json!({"case":name,"views":views})
}

#[test]
#[ignore = "requires a native raster adapter; no noop or empty-corpus qualification"]
fn appearance_sources_match_offscreen_capture() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    assert_ne!(adapter.get_info().backend, wgpu::Backend::Noop);
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    use noon::example_scenes as scenes;
    let cases = vec![
        compare_program(
            "glow-pulse",
            scenes::glow_pulse::program,
            &adapter,
            &device,
            &queue,
        ),
        compare_program(
            "image-glow",
            scenes::image_glow::program,
            &adapter,
            &device,
            &queue,
        ),
        compare_program(
            "linear-gradient-glow",
            scenes::linear_gradient::glow_program,
            &adapter,
            &device,
            &queue,
        ),
        compare_program(
            "radial-gradient",
            scenes::radial_gradient::program,
            &adapter,
            &device,
            &queue,
        ),
        compare_program(
            "image-fill",
            scenes::image_fill::program,
            &adapter,
            &device,
            &queue,
        ),
        compare_program(
            "noise-paint",
            scenes::noise_paint::program,
            &adapter,
            &device,
            &queue,
        ),
        compare_program(
            "hatch-paint",
            scenes::hatch_paint::program,
            &adapter,
            &device,
            &queue,
        ),
        compare_program("blur", scenes::blur::program, &adapter, &device, &queue),
        compare_program("shadow", scenes::shadow::program, &adapter, &device, &queue),
        compare_program(
            "ordered-effects",
            scenes::ordered_effects::program,
            &adapter,
            &device,
            &queue,
        ),
        compare_program(
            "composed-effects",
            scenes::composed_effects::animated_program,
            &adapter,
            &device,
            &queue,
        ),
        compare_program(
            "dissolve",
            scenes::dissolve::program,
            &adapter,
            &device,
            &queue,
        ),
        compare_program(
            "noise-dissolve",
            scenes::noise_dissolve::program,
            &adapter,
            &device,
            &queue,
        ),
        compare_program(
            "pixelate",
            scenes::pixelate::program,
            &adapter,
            &device,
            &queue,
        ),
        compare_program(
            "glass-lens",
            scenes::glass_lens::program,
            &adapter,
            &device,
            &queue,
        ),
        compare_program(
            "camera-bloom",
            scenes::camera_bloom::program,
            &adapter,
            &device,
            &queue,
        ),
        compare_program(
            "shader-time",
            scenes::shader_sweep::authored_time_program,
            &adapter,
            &device,
            &queue,
        ),
        compare_program(
            "shader-fade",
            scenes::shader_sweep::fade_in_program,
            &adapter,
            &device,
            &queue,
        ),
    ];
    assert_eq!(cases.len(), 18);
    if let Some(root) = std::env::var_os("NOON_NATIVE_HDR_ARTIFACTS") {
        let root = std::path::PathBuf::from(root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("appearance-capture.json"),
            serde_json::to_vec_pretty(
                &serde_json::json!({"adapter":format!("{:?}",adapter.get_info()),"cases":cases}),
            )
            .unwrap(),
        )
        .unwrap();
    }
}

#[test]
#[ignore = "requires a native raster adapter; exercises admitted finite viewer seek"]
fn finite_glow_seek_matches_forward_capture() {
    use noon::example_scenes::glow_profile::{session, Property};
    const TIMES: [f64; 5] = [0.0, 0.5, 5.0, 9.5, 10.0];
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    assert_ne!(adapter.get_info().backend, wgpu::Backend::Noop);
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let mut cases = Vec::new();
    for active in [1, 100] {
        for property in [Property::Intensity, Property::Radius] {
            for mut case in compare_finite_seek_capture(
                &format!("{active} {property:?}"),
                || session(active, property, 10.0).unwrap(),
                TIMES,
                &adapter,
                &device,
                &queue,
            ) {
                case["active"] = serde_json::json!(active);
                case["property"] = serde_json::json!(format!("{property:?}"));
                cases.push(case);
            }
        }
    }
    if let Some(root) = std::env::var_os("NOON_NATIVE_HDR_ARTIFACTS") {
        let root = std::path::PathBuf::from(root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("finite-glow-seek-capture.json"),
            serde_json::to_vec_pretty(
                &serde_json::json!({"adapter":format!("{:?}",adapter.get_info()),"cases":cases}),
            )
            .unwrap(),
        )
        .unwrap();
    }
}

fn compare_finite_seek_capture(
    label: &str,
    factory: impl Fn() -> noon::SceneExecution,
    times: [f64; 5],
    adapter: &wgpu::Adapter,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> Vec<serde_json::Value> {
    use crate::execution_source::StaticExecutionSource;
    const SEEKS: [usize; 7] = [0, 2, 1, 3, 0, 4, 2];
    let mut cases = Vec::new();
    for size in [[320, 180], [240, 160]] {
        let mut forward = factory();
        let mut capture = noon_export::SessionCapture::new(
            forward.session(),
            noon_export::CaptureOptions::new(size[0], size[1]),
        )
        .unwrap();
        let mut expected = Vec::new();
        for time in times {
            forward.session_mut().advance_to(time).unwrap();
            assert_eq!(forward.session().frame().time, time);
            expected.push(
                capture
                    .capture(forward.session_mut())
                    .unwrap()
                    .rgba
                    .to_vec(),
            );
        }
        assert!(expected[0]
            .as_chunks::<4>()
            .0
            .iter()
            .any(|p| p[..3] != [0, 0, 0]));
        assert_ne!(
            expected[0], expected[4],
            "animated source must change pixels"
        );
        let mut source = StaticExecutionSource::new(factory(), RustHostCallbackTable::new());
        assert_eq!(source.finite_transport_end(), Some(times[4]));
        let mut draw = Draw::new(device, queue);
        let mut output = AuthoredViewOutput::new(
            adapter,
            device,
            FORMAT,
            noon_render_wgpu::OutputTransfer::Direct,
        );
        let mut maximum = 0;
        for index in SEEKS {
            let time = times[index];
            source.seek_to(time).unwrap();
            assert_eq!(source.frame_time(), time);
            draw.camera = source.camera().unwrap();
            let bounds = draw
                .camera
                .viewport_bounds(size[0] as f32 / size[1] as f32)
                .unwrap();
            draw.visible = Some(source.query_viewports(&[bounds]).object_indices().to_vec());
            let publication = source.take_renderer_publication();
            let pixels = draw
                .frame(
                    device,
                    queue,
                    &mut output,
                    &publication,
                    size,
                    &OverlayGpuState::default(),
                    false,
                )
                .0;
            assert_eq!(pixels.len(), expected[index].len());
            let error = pixels
                .iter()
                .zip(&expected[index])
                .map(|(a, b)| a.abs_diff(*b))
                .max()
                .unwrap();
            maximum = maximum.max(error);
            assert!(
                error <= 2,
                "{label} {size:?} t={time}: seek/capture error {error}"
            );
        }
        source.retire_execution_source().unwrap();
        eprintln!(
            "native seek/capture {label} {size:?}: {} samples, max error {maximum}",
            SEEKS.len()
        );
        cases.push(
            serde_json::json!({"size":size,"times":SEEKS.map(|i|times[i]),"maximum_error":maximum}),
        );
    }
    cases
}

#[test]
#[ignore = "requires a native raster adapter; external ScanBand seek/capture qualification"]
fn scan_band_seek_matches_forward_capture() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    assert_ne!(adapter.get_info().backend, wgpu::Backend::Noop);
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let cases = compare_finite_seek_capture(
        "ScanBand",
        || noon::example_scenes::scan_band::execution().unwrap(),
        [0.0, 0.5, 1.0, 1.5, 2.0],
        &adapter,
        &device,
        &queue,
    );
    if let Some(root) = std::env::var_os("NOON_NATIVE_HDR_ARTIFACTS") {
        let root = std::path::PathBuf::from(root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("scan-band-seek-capture.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "adapter":format!("{:?}",adapter.get_info()),"cases":cases,
            }))
            .unwrap(),
        )
        .unwrap();
    }
}
