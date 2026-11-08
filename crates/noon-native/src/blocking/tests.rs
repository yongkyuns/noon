use super::*;

fn capability(host: &Rc<NativeBlockingHost>) -> BlockingScene {
    let erased: Rc<dyn BlockingSceneHost> = host.clone();
    BlockingScene::for_host(Rc::downgrade(&erased))
}

#[test]
fn native_authoring_capability_is_scoped_and_weak() {
    let host = Rc::new(NativeBlockingHost::new(NativeViewportConfig::default()));
    let scene = capability(&host);
    let circle = scene.circle(0.5).unwrap();
    scene.add(&circle).unwrap();
    host.with_scene(&mut |_, _| {
        assert!(matches!(scene.time(), Err(ScriptError::Reentrant)));
        Ok(())
    })
    .unwrap();
    assert_eq!(scene.time().unwrap(), 0.0);
    drop(host);
    assert!(matches!(scene.time(), Err(ScriptError::Inactive)));
}

#[test]
fn error_before_first_boundary_does_not_start_a_window_or_leave_a_live_capability() {
    let host = Rc::new(NativeBlockingHost::new(NativeViewportConfig::default()));
    let mut escaped = None;
    let result = run_with_host(
        |scene| {
            escaped = Some(scene.clone());
            let square = scene.square(1.0)?;
            scene.add(&square)?;
            Err(ScriptError::Host(
                "author stopped during construction".into(),
            ))
        },
        host.clone(),
    );
    assert!(
        matches!(result, Err(NativeHostError::Program(message)) if message.contains("author stopped"))
    );
    assert!(matches!(*host.state.borrow(), HostState::Retired));
    assert!(matches!(
        escaped.unwrap().time(),
        Err(ScriptError::Inactive)
    ));
}

#[cfg(target_os = "linux")]
fn desktop_test_host() -> Rc<NativeBlockingHost> {
    use winit::platform::x11::EventLoopBuilderExtX11;
    let host = Rc::new(NativeBlockingHost::new(NativeViewportConfig {
        title: "Noon ordinary Rust authoring qualification".into(),
        width: 320,
        height: 180,
        ..Default::default()
    }));
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    host.event_loop.replace(Some(builder.build().unwrap()));
    host
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "requires X11 and WGPU; run in its own process with a bounded timeout"]
fn ordinary_helpers_pump_native_frames_and_return_after_admitted_boundaries() {
    let host = desktop_test_host();
    host.close_after_authoring.set(true);
    let observer = host.clone();
    let mut returned = false;
    run_with_host(
        |scene| {
            noon::example_scenes::blocking_authoring::construct(scene)?;
            let (origin, window) = observer.with_running(|running| {
                assert_eq!(running.app.execution.frame_time(), 1.0);
                assert!(running.app.execution.pending_endpoint().is_none());
                assert!(running.app.last_geometry_draw_calls > 0);
                assert!(running.app.pointer.presented.is_some());
                Ok((
                    running.app.realtime_clock.unwrap().wall_origin,
                    running.app.window.as_ref().unwrap().clone(),
                ))
            })?;
            // X11's geometry round trip confirms that the real server resize is
            // queued. The native handler has not run since the author resumed.
            let resized = winit::dpi::PhysicalSize::new(400, 240);
            let _ = window.request_inner_size(resized);
            assert_eq!(window.inner_size(), resized);
            observer.with_running(|running| {
                assert_eq!(running.app.gpu.as_ref().unwrap().config.width, 320);
                Ok(())
            })?;
            scene.wait(0.1)?;
            observer.with_running(|running| {
                let gpu = running.app.gpu.as_ref().unwrap();
                assert_eq!((gpu.config.width, gpu.config.height), (400, 240));
                assert_eq!(running.app.realtime_clock.unwrap().wall_origin, origin);
                assert!(running.app.execution.source_active());
                Ok(())
            })?;
            returned = true;
            Ok(())
        },
        host,
    )
    .unwrap();
    assert!(returned);
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "requires X11 and WGPU; run in its own process with a bounded timeout"]
fn rejected_first_boundary_is_recoverable_and_window_close_cancels_the_next_wait() {
    use noon::{AnimationOptions, Vec2};
    let host = desktop_test_host();
    let observer = host.clone();
    let mut cancelled = false;
    let result = run_with_host(
        |scene| {
            let square = scene.square(0.5)?;
            scene.add(&square)?;
            assert!(scene.wait(-1.0).is_err());
            scene.wait(0.0)?;
            assert_eq!(scene.time()?, 0.0);
            scene.play_with(
                square.animate().shift(Vec2::new(1.0, 0.0)),
                AnimationOptions::new().run_time(0.05),
            )?;
            assert!((scene.effective_layout(&square)?.center.0 - 1.0).abs() < 1e-5);
            let close = observer.with_running(|running| {
                running.app.close_on_user_event = true;
                Ok(running.event_loop.create_proxy())
            })?;
            // The next pump delivers this queued platform event only after
            // scene.wait has activated its long, still-outstanding boundary.
            close.send_event(()).unwrap();
            let waiting_since = std::time::Instant::now();
            assert!(matches!(scene.wait(60.0), Err(ScriptError::Cancelled)));
            assert!(waiting_since.elapsed() < std::time::Duration::from_secs(10));
            observer.with_running(|running| {
                assert!(matches!(
                    running.app.close_event_timeline,
                    Some(crate::TimelineWakeState::Deadline(end)) if (end - 60.05).abs() < 1e-8
                ));
                assert!(running.app.execution.frame_time() < 60.05);
                Ok(())
            })?;
            assert!(matches!(scene.time(), Err(ScriptError::Cancelled)));
            assert!(matches!(scene.wait(0.0), Err(ScriptError::Cancelled)));
            cancelled = true;
            Ok(())
        },
        host,
    );
    assert!(cancelled);
    assert!(
        matches!(result, Err(NativeHostError::Program(message)) if message.contains("cancelled"))
    );
}
