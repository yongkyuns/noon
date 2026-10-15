use super::*;

#[test]
#[ignore = "requires Metal or software Vulkan; run by native host qualification"]
fn native_capture_first_path_after_single_sample_is_visible_in_its_first_publication() {
    use noon::{Color, MobjectTarget, Scene, SvgImportOptions};
    let scene = Scene::new();
    let mut session = scene.execution_session().unwrap();
    let mut options = CaptureOptions::new(320, 180);
    if cfg!(target_os = "macos") {
        options.backends = wgpu::Backends::METAL;
        options.force_fallback_adapter = false;
    } else {
        options.backends = wgpu::Backends::VULKAN;
        options.force_fallback_adapter = true;
    }
    let mut capture = crate::SessionCapture::new(&session, options).unwrap();
    capture.render(&mut session).unwrap();
    {
        let mut live = scene.live(&mut session);
        let wait = live.wait_segment(0.1).unwrap();
        live.advance_segment_to(wait, wait.end_time()).unwrap();
        live.complete_segment(wait).unwrap();
    }
    capture.render(&mut session).unwrap();
    {
        let mut live = scene.live(&mut session);
        let svg = live
            .create_svg(
                r#"<svg width="20" height="10"><rect width="20" height="10" fill="red"/></svg>"#,
                SvgImportOptions::default(),
                None,
            )
            .unwrap();
        live.add_many(&[MobjectTarget::Family(&svg)]).unwrap();
        live.shift_family(&svg, 1.0, 0.0).unwrap();
        live.set_family_fill(&svg, Some(Color::from_hex(0x00FF00)), None)
            .unwrap();
        let wait = live.wait_segment(0.1).unwrap();
        live.advance_segment_to(wait, wait.start_time()).unwrap();
    }
    let first = capture.capture(&mut session).unwrap();
    assert_eq!(first.receipt.published_time, 0.1);
    assert_eq!(&first.rgba[(90 * 320 + 160) * 4..][..4], &[0, 255, 0, 255]);
    let publication = first.receipt.publication;
    let pixels = first.rgba.to_vec();
    let repeated = capture.capture(&mut session).unwrap();
    assert_eq!(repeated.receipt.publication, publication);
    assert_eq!(repeated.rgba, pixels);
}

#[test]
fn first_gpu_fault_is_terminal_and_is_not_replaced_by_later_faults() {
    let fault = GpuFault::default();
    assert!(fault.check().is_ok());
    fault.record("first".to_owned());
    fault.record("second".to_owned());
    assert!(matches!(fault.check(), Err(CaptureError::Gpu(message)) if message == "first"));
}

#[test]
#[ignore = "requires software Vulkan; run by Native Host Smoke without DISPLAY"]
fn native_capture_spatial_depth_and_device_loss() {
    let mut options = CaptureOptions::new(257, 129);
    options.backends = wgpu::Backends::VULKAN;
    options.force_fallback_adapter = true;
    let mut gpu =
        pollster::block_on(NativeCapture::new(&options, options.layout().unwrap())).unwrap();
    let mut session = noon::example_scenes::spatial_mesh::session().unwrap();
    for (time, front_is_red) in [(0.0, true), (2.0, false)] {
        session.seek(time).unwrap();
        let view = CaptureView::new(&session, &options).unwrap();
        let query = session.query_viewports(&view.bounds);
        let visible = session.renderer_viewport_query(query).unwrap();
        let publication = session.take_renderer_publication();
        gpu.render(&publication, &view, visible.object_indices(), true)
            .unwrap();
        let center = &gpu.pixels()[(64 * 257 + 128) * 4..][..4];
        if front_is_red {
            assert!(
                center[0] > center[2],
                "front red mesh must occlude the blue mesh: {center:?}"
            );
        } else {
            assert!(
                center[2] > center[0],
                "blue mesh must occlude the moved red mesh: {center:?}"
            );
        }
        assert_eq!(center[3], 255);
        assert_eq!(publication.frame().time, time);
    }
    // A lost device must not return the last successful frame as new output.
    gpu.device.destroy();
    let _ = gpu.device.poll(wgpu::PollType::Poll);
    let view = CaptureView::new(&session, &options).unwrap();
    let query = session.query_viewports(&view.bounds);
    let visible = session.renderer_viewport_query(query).unwrap();
    let publication = session.take_renderer_publication();
    assert!(gpu
        .render(&publication, &view, visible.object_indices(), true)
        .is_err());
}
