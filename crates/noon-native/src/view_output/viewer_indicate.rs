use super::*;
use noon::{AnimationOptions, Create, ManimAxesOptions, RateFunction, ScriptError};
use winit::dpi::PhysicalPosition;

const SIZE: [u32; 2] = [512, 288];

fn graph_app() -> NativeApp {
    let program = Scene::script(|scene| async move {
        let mut options = ManimAxesOptions::new([-4.0, 4.0, 2.0], [-2.0, 2.0, 1.0], 8.0, 4.0);
        options.style.stroke_width = 0.065;
        options.ticks.half_length = 0.24;
        let axes = scene.axes(&options)?;
        scene.add(axes.family())?;
        let graph = axes
            .plot(|x| 1.25 + 0.25 * x.sin(), Some(&[-4.0, 4.0, 0.1]), false)
            .map_err(|error| ScriptError::Host(error.to_string()))?;
        scene
            .play_with(
                Create::new(&graph),
                AnimationOptions::new()
                    .run_time(3.0)
                    .rate_func(RateFunction::Linear),
            )
            .await?;
        scene.wait(1.0).await
    })
    .unwrap();
    let source = PlaybackExecutionSource::new(program, RustHostCallbackTable::new()).unwrap();
    let mut app = NativeApp::from_source(Box::new(source), NativeViewportConfig::default());
    for _ in 0..8 {
        app.execution.prepare_until(2.0).unwrap();
        if app.execution.presentation_ready() {
            break;
        }
    }
    app.execution.advance_to(0.6).unwrap();
    assert_eq!(app.session().frame().time, 0.6);
    app
}

fn present(app: &mut NativeApp, wall_time: f64, mut draw: impl FnMut(&RendererPublication<'_>)) {
    app.execution.advance_interactions(wall_time).unwrap();
    let frame = app
        .capture_pointer_presentation(PhysicalSize::from(SIZE), 1.0)
        .unwrap();
    let pick = app.capture_viewer_pick_frame(&frame, false);
    let samples = app.session().viewer_indications();
    app.execution
        .with_viewer_renderer_publication(&samples, &app.viewer_presentation, &mut |publication| {
            draw(&publication);
            Ok(publication.context())
        })
        .unwrap();
    app.viewer_presentation.presented(&samples);
    app.execution.admit_presented_frame(&frame).unwrap();
    app.pointer.presented = Some(frame);
    app.pointer.presented_viewer = pick;
    app.pointer.refresh_pending = false;
}

fn tick_edge(app: &mut NativeApp, state: ElementState) {
    let size = PhysicalSize::from(SIZE);
    let pixels_per_unit = f64::from(SIZE[1]) / 8.0;
    let x = f64::from(SIZE[0]) * 0.5 - 2.0 * pixels_per_unit;
    let y = f64::from(SIZE[1]) * 0.5 - 0.20 * pixels_per_unit;
    app.dispatch_pointer_position(PhysicalPosition::new(x, y), size, 1.0)
        .unwrap();
    assert!(matches!(
        app.dispatch_pointer_button(MouseButton::Left, state, size, 1.0)
            .unwrap(),
        crate::pointer_input::PointerDispatch::Admitted
    ));
}

#[test]
fn native_graph_tick_feedback_is_scoped_to_the_viewer_publication() {
    let mut app = graph_app();
    let canonical = app.session().frame().clone();
    let mut axes = Vec::new();
    present(&mut app, 0.0, |publication| {
        for row in &canonical.objects {
            if publication.viewer_group(row.id).is_some() {
                axes.push(row.id);
            }
        }
    });
    assert!(axes.len() > 2);

    tick_edge(&mut app, ElementState::Pressed);
    // A normal playback frame can be evaluated before release. The contact
    // stays attached to the tick in the last actual picture.
    app.execution.advance_to(0.75).unwrap();
    tick_edge(&mut app, ElementState::Released);
    assert!(app.session().viewer_inspection_active());
    app.playback_paused = true;
    present(&mut app, 0.0, |_| {});
    let canonical = app.session().frame().clone();
    let context = app.session().publication_context();
    present(&mut app, 0.5, |publication| {
        assert_eq!(publication.context(), context);
        for (index, object) in canonical.objects.iter().enumerate() {
            let presented = &publication.frame().objects[index];
            if axes.contains(&object.id) {
                assert_ne!(presented.style.stroke, object.style.stroke);
                assert!(publication.changes().object_indices().contains(&index));
            } else {
                assert_eq!(presented, object);
            }
        }
    });
    assert_eq!(app.session().frame(), &canonical);
    present(&mut app, 1.1, |publication| {
        assert_eq!(publication.frame(), &canonical);
        for id in &axes {
            assert!(publication
                .changes()
                .object_indices()
                .contains(&publication.object_index(*id).unwrap()));
        }
    });
    assert!(!app.session().viewer_inspection_active());
    assert_eq!(app.session().publication_context(), context);
}

#[test]
#[ignore = "requires a real native raster adapter; run explicitly for pixel qualification"]
fn native_graph_tick_pulses_both_axes_during_playback_and_pause() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    assert_ne!(adapter.get_info().backend, wgpu::Backend::Noop);
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();

    fn yellow_axes(pixels: &[u8]) -> (usize, usize) {
        let mut counts = (0, 0);
        for (index, pixel) in pixels.as_chunks::<4>().0.iter().enumerate() {
            if u16::from(pixel[0]) < u16::from(pixel[2]) + 18
                || u16::from(pixel[1]) < u16::from(pixel[2]) + 18
            {
                continue;
            }
            let x = (index % SIZE[0] as usize) as i32 - (SIZE[0] as i32 / 2);
            let y = (index / SIZE[0] as usize) as i32 - (SIZE[1] as i32 / 2);
            counts.0 += usize::from(x.abs() < 5 && (25..70).contains(&y.abs()));
            counts.1 += usize::from(y.abs() < 5 && (25..120).contains(&x.abs()));
        }
        counts
    }

    for paused in [false, true] {
        let mut app = graph_app();
        app.playback_paused = paused;
        let mut renderer = Draw::new(&device, &queue);
        let mut output = AuthoredViewOutput::new(
            &adapter,
            &device,
            FORMAT,
            noon_render_wgpu::OutputTransfer::Direct,
        );
        let mut render = |app: &mut NativeApp, time| {
            let mut pixels = Vec::new();
            present(app, time, |publication| {
                pixels = renderer
                    .frame(
                        &device,
                        &queue,
                        &mut output,
                        publication,
                        SIZE,
                        &OverlayGpuState::default(),
                        false,
                    )
                    .0;
            });
            pixels
        };
        let baseline = render(&mut app, 0.0);
        assert_eq!(yellow_axes(&baseline), (0, 0));
        tick_edge(&mut app, ElementState::Pressed);
        tick_edge(&mut app, ElementState::Released);
        render(&mut app, 0.0);
        if !paused {
            app.execution.advance_to(1.1).unwrap();
        }
        let peak = render(&mut app, 0.5);
        let (vertical, horizontal) = yellow_axes(&peak);
        assert!(vertical > 30 && horizontal > 30, "{vertical}, {horizontal}");
        assert_eq!(app.session().frame().time, if paused { 0.6 } else { 1.1 });
        let restored = render(&mut app, 1.1);
        assert_eq!(yellow_axes(&restored), (0, 0));
        if paused {
            assert!(
                restored == baseline,
                "paused view must restore exact pixels"
            );
        } else {
            assert!(
                restored != baseline,
                "the authored curve continued revealing"
            );
        }
        eprintln!(
            "{:?}, paused={paused}, yellow pixels on vertical and horizontal axes={vertical},{horizontal}",
            adapter.get_info().backend,
        );
    }
}
