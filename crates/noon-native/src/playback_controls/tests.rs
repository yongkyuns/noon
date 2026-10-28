use super::*;

use crate::{NativeViewportConfig, RealtimeClock};
use noon::{AudioClipOptions, AudioResource, RateFunction, Scene, SectionType, SemanticVec3};
use std::time::Duration;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::MouseButton;
use winit::keyboard::ModifiersState;

const SIZE: PhysicalSize<u32> = PhysicalSize::new(800, 400);

fn app(action: Option<PlaybackControl>, audio: bool) -> NativeApp {
    let mut scene = Scene::new();
    let mut button = scene.rectangle(2.0, 1.0).unwrap();
    button.set_fill(0.2, 0.4, 0.5, 1.0).unwrap();
    scene.add(&button).unwrap();
    if let Some(action) = action {
        scene.on_click_playback(&button, action).unwrap();
    }
    for (name, time) in [("Begin", 0.0), ("Middle", 4.0), ("End", 8.0)] {
        scene
            .next_section_at(name, SectionType::Normal, time)
            .unwrap();
    }
    // A future compiled scalar interval gives the shared runtime a real sleep
    // deadline and finite endpoint. Scene::wait alone is an authoring cursor,
    // so a static snapshot with no tracks correctly has runtime duration zero.
    let marker = scene.circle(0.1).unwrap();
    scene.add(&marker).unwrap();
    let tracker = scene.value_tracker(0.0).unwrap();
    let position = scene
        .position_from_tracker(
            &tracker,
            SemanticVec3::new(1.0, 0.0, 0.0),
            SemanticVec3::new(3.0, 2.0, 0.0),
        )
        .unwrap();
    scene.bind_position(&marker, &position).unwrap();
    scene.wait(2.0).unwrap();
    scene
        .play_value(&tracker, 1.0)
        .rate_func(RateFunction::Linear)
        .run_time(2.0)
        .unwrap();
    if audio {
        scene
            .audio(
                AudioResource::from_interleaved(8_000, 1, vec![0.25; 8], 32).unwrap(),
                AudioClipOptions {
                    looped: true,
                    duration: Some(12.0),
                    ..Default::default()
                },
            )
            .unwrap();
    }
    let section_navigation = scene.section_navigation();
    let mut app = NativeApp::new(
        scene.into_execution().unwrap(),
        NativeViewportConfig {
            playback_controls: true,
            section_navigation,
            ..Default::default()
        },
    );
    present(&mut app);
    app
}

// Model the existing publication contract for deterministic CPU tests. The
// separate X11/Pulse fixture exercises real surface preparation and output.
fn present(app: &mut NativeApp) {
    let _ = app.execution.take_renderer_publication();
    let frame = app.capture_pointer_presentation(SIZE, 1.0).unwrap();
    app.execution.admit_presented_frame(&frame).unwrap();
    app.pointer.presented = Some(frame);
    app.pointer.refresh_pending = false;
    app.force_full_redraw = false;
    app.playback_picture_pending = false;
}

fn click(app: &mut NativeApp) {
    present(app);
    app.dispatch_pointer_position(PhysicalPosition::new(400.0, 200.0), SIZE, 1.0)
        .unwrap();
    for state in [ElementState::Pressed, ElementState::Released] {
        present(app);
        app.dispatch_pointer_button(MouseButton::Left, state, SIZE, 1.0)
            .unwrap();
    }
}

#[test]
fn pause_samples_sleeping_authored_wait_and_resume_excludes_control_latency() {
    let mut app = app(None, false);
    let origin = Instant::now();
    app.realtime_clock = Some(RealtimeClock::new(origin, 0.0));
    let revision = app.session().publication_context().scene_revision();
    app.pause_playback(origin + Duration::from_millis(3_250))
        .unwrap();
    assert_eq!(app.execution.frame_time(), 3.25);
    assert!(app.playback_paused);
    assert!(app.realtime_clock.is_none());
    app.advance_realtime_timeline(origin + Duration::from_secs(100))
        .unwrap();
    assert_eq!(app.execution.frame_time(), 3.25);
    app.resume_playback().unwrap();
    assert!(!app.playback_paused);
    assert!(app.playback_picture_pending);
    app.advance_realtime_timeline(origin + Duration::from_secs(110))
        .unwrap();
    assert_eq!(app.execution.frame_time(), 3.25);
    present(&mut app);
    app.advance_realtime_timeline(origin + Duration::from_secs(120))
        .unwrap();
    app.pause_playback(origin + Duration::from_millis(120_750))
        .unwrap();
    assert_eq!(app.execution.frame_time(), 4.0);
    assert_eq!(
        app.session().publication_context().scene_revision(),
        revision
    );
}

#[test]
fn seek_and_restart_preserve_intent_and_require_the_target_picture() {
    for paused in [false, true] {
        let mut app = app(None, false);
        app.playback_paused = paused;
        app.realtime_clock = Some(RealtimeClock::new(Instant::now(), 0.0));
        app.seek_playback(6.125).unwrap();
        assert_eq!(app.execution.frame_time(), 6.125);
        assert_eq!(app.playback_paused, paused);
        assert!(app.realtime_clock.is_none());
        assert!(app.playback_picture_pending);
        assert!(app.pointer.presented.is_none());
        app.advance_realtime_timeline(Instant::now() + Duration::from_secs(100))
            .unwrap();
        assert_eq!(app.execution.frame_time(), 6.125);
        app.dispatch_playback_control(PlaybackControl::Restart, Instant::now())
            .unwrap();
        assert_eq!(app.execution.frame_time(), 0.0);
        assert_eq!(app.playback_paused, paused);
    }
}

#[test]
fn denied_seek_keeps_current_picture_clock_and_viewer_state() {
    let mut app = app(None, false);
    let origin = Instant::now();
    app.realtime_clock = Some(RealtimeClock::new(origin, 0.0));
    let publication = app.session().publication_context();
    for time in [-1.0, 12.01, f64::NAN, f64::INFINITY] {
        assert!(app.seek_playback(time).is_err());
        assert_eq!(app.execution.frame_time(), 0.0);
        assert_eq!(app.session().publication_context(), publication);
        assert_eq!(app.realtime_clock.unwrap().wall_origin, origin);
        assert!(!app.playback_paused);
        assert!(!app.playback_picture_pending);
        assert!(app.pointer.presented.is_some());
    }
}

#[test]
fn section_navigation_observes_elapsed_waits_and_never_wraps() {
    let mut app = app(None, false);
    let origin = Instant::now();
    app.realtime_clock = Some(RealtimeClock::new(origin, 0.0));
    app.dispatch_playback_control(
        PlaybackControl::NextSection,
        origin + Duration::from_secs(5),
    )
    .unwrap();
    assert_eq!(app.execution.frame_time(), 8.0);
    assert!(!app.playback_paused);
    app.playback_paused = true;
    app.dispatch_playback_control(PlaybackControl::PreviousSection, origin)
        .unwrap();
    assert_eq!(app.execution.frame_time(), 4.0);
    assert!(app.playback_paused);
    app.dispatch_playback_control(PlaybackControl::PreviousSection, origin)
        .unwrap();
    assert_eq!(app.execution.frame_time(), 0.0);
    let publication = app.session().publication_context();
    app.dispatch_playback_control(PlaybackControl::PreviousSection, origin)
        .unwrap();
    assert_eq!(app.session().publication_context(), publication);
}

#[test]
fn play_at_completion_restarts_the_same_finite_session() {
    let mut app = app(None, false);
    app.seek_playback(12.0).unwrap();
    let revision = app.session().publication_context().scene_revision();
    app.dispatch_playback_control(PlaybackControl::TogglePlayback, Instant::now())
        .unwrap();
    assert_eq!(app.execution.frame_time(), 0.0);
    assert!(!app.playback_paused);
    assert!(app.playback_picture_pending);
    assert_eq!(
        app.session().publication_context().scene_revision(),
        revision
    );
}

#[test]
fn viewer_mute_and_volume_preserve_authored_pcm_revision_and_position() {
    let mut app = app(None, true);
    app.seek_playback(1.125).unwrap();
    let publication = app.session().publication_context();
    let format = noon::integration::AudioMixFormat::new(8_000, 1).unwrap();
    let mut mixer = app.session().prepare_audio_mix(format, 32).unwrap();
    let mut expected = [0.0; 32];
    mixer.mix_audio_block(1.125, 0, &mut expected).unwrap();
    app.set_viewer_volume(0.25).unwrap();
    app.set_viewer_muted(true).unwrap();
    for volume in [-1.0, 1.001, f64::NAN, f64::INFINITY] {
        assert!(app.set_viewer_volume(volume).is_err());
        assert_eq!(app.viewer_volume, 0.25);
        assert!(app.viewer_muted);
    }
    app.set_viewer_muted(false).unwrap();
    assert_eq!(app.viewer_volume, 0.25);
    let mut actual = [0.0; 32];
    app.session()
        .prepare_audio_mix(format, 32)
        .unwrap()
        .mix_audio_block(1.125, 0, &mut actual)
        .unwrap();
    assert_eq!(actual, expected);
    assert_eq!(app.session().publication_context(), publication);
    assert_eq!(app.execution.frame_time(), 1.125);
}

#[test]
fn keyboard_controls_are_opt_in_and_repeat_release_or_modified_keys_do_not_toggle() {
    let mut app = app(None, false);
    let key = PhysicalKey::Code(KeyCode::Space);
    app.config.playback_controls = false;
    assert!(!app
        .dispatch_playback_key(key, ElementState::Pressed, false)
        .unwrap());
    app.config.playback_controls = true;
    for (state, repeat) in [
        (ElementState::Pressed, true),
        (ElementState::Released, false),
    ] {
        assert!(app.dispatch_playback_key(key, state, repeat).unwrap());
        assert!(!app.playback_paused);
    }
    app.pointer.modifiers = ModifiersState::CONTROL;
    assert!(!app
        .dispatch_playback_key(key, ElementState::Pressed, false)
        .unwrap());
    app.pointer.modifiers = ModifiersState::empty();
    assert!(app
        .dispatch_playback_key(key, ElementState::Pressed, false)
        .unwrap());
    assert!(app.playback_paused);
    assert!(!app
        .dispatch_playback_key(
            PhysicalKey::Code(KeyCode::KeyA),
            ElementState::Pressed,
            false
        )
        .unwrap());
}

#[test]
fn keyboard_seek_and_viewer_gain_share_the_same_admitted_commands() {
    let mut app = app(None, false);
    app.playback_paused = true;
    for (key, time) in [
        (KeyCode::ArrowRight, 5.0),
        (KeyCode::PageDown, 8.0),
        (KeyCode::ArrowLeft, 3.0),
        (KeyCode::Home, 0.0),
    ] {
        app.dispatch_playback_key(PhysicalKey::Code(key), ElementState::Pressed, false)
            .unwrap();
        assert_eq!(app.execution.frame_time(), time);
        assert!(app.playback_paused);
    }
    app.dispatch_playback_key(
        PhysicalKey::Code(KeyCode::ArrowDown),
        ElementState::Pressed,
        false,
    )
    .unwrap();
    assert_eq!(app.viewer_volume, 0.95);
    app.dispatch_playback_key(
        PhysicalKey::Code(KeyCode::KeyM),
        ElementState::Pressed,
        false,
    )
    .unwrap();
    assert!(app.viewer_muted);
}

#[test]
fn scene_clicks_dispatch_once_after_publication_and_keep_paused_seek_silent() {
    for control in [
        PlaybackControl::TogglePlayback,
        PlaybackControl::NextSection,
        PlaybackControl::PreviousSection,
        PlaybackControl::Restart,
        PlaybackControl::ToggleAudioMuted,
    ] {
        let mut app = app(Some(control), false);
        app.playback_paused = true;
        app.seek_playback(6.0).unwrap();
        let revision = app.session().publication_context().scene_revision();
        click(&mut app);
        match control {
            PlaybackControl::TogglePlayback => assert!(!app.playback_paused),
            PlaybackControl::NextSection => assert_eq!(app.execution.frame_time(), 8.0),
            PlaybackControl::PreviousSection => assert_eq!(app.execution.frame_time(), 4.0),
            PlaybackControl::Restart => assert_eq!(app.execution.frame_time(), 0.0),
            PlaybackControl::ToggleAudioMuted => assert!(app.viewer_muted),
        }
        assert_eq!(
            app.session().publication_context().scene_revision(),
            revision
        );
        present(&mut app);
        app.dispatch_pointer_button(MouseButton::Left, ElementState::Released, SIZE, 1.0)
            .unwrap();
        if control == PlaybackControl::TogglePlayback {
            assert!(!app.playback_paused);
        }
        if control == PlaybackControl::ToggleAudioMuted {
            assert!(app.viewer_muted);
        }
    }
}

#[test]
fn cancelled_and_dragged_scene_controls_do_not_change_transport() {
    for cancel in [true, false] {
        let mut app = app(Some(PlaybackControl::TogglePlayback), false);
        present(&mut app);
        app.dispatch_pointer_position(PhysicalPosition::new(400.0, 200.0), SIZE, 1.0)
            .unwrap();
        present(&mut app);
        app.dispatch_pointer_button(MouseButton::Left, ElementState::Pressed, SIZE, 1.0)
            .unwrap();
        if cancel {
            app.pointer_focus_lost().unwrap();
        } else {
            present(&mut app);
            app.dispatch_pointer_position(PhysicalPosition::new(425.0, 200.0), SIZE, 1.0)
                .unwrap();
        }
        present(&mut app);
        app.dispatch_pointer_button(MouseButton::Left, ElementState::Released, SIZE, 1.0)
            .unwrap();
        assert!(!app.playback_paused);
        assert_eq!(app.execution.frame_time(), 0.0);
    }
}
