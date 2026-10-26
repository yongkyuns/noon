//! Actual X11/ALSA/Pulse qualification. Captured PCM is checked independently.

use super::*;
use crate::{NativeViewportConfig, RealtimeClock};
use noon::integration::{AudioMixFormat, PreparedAudioMix};
use noon::{AudioClipOptions, AudioResource, ExecutionSession, Scene, SectionType};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalPosition;
use winit::event::{MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::platform::x11::EventLoopBuilderExtX11;
use winit::window::WindowId;

// Xvfb's FIFO surface can present without physical display pacing. Keep this
// controlled software-device proof at 60 Hz so rendering does not manufacture
// an unbounded CPU competitor for the software audio daemon.
const PICTURE_INTERVAL: Duration = Duration::from_nanos(16_666_667);

fn fixture() -> (ExecutionSession, noon::SceneSectionNavigation) {
    use noon_core::{
        AnimationOptions, CompositionTimeMap, RateFunction, SemanticAnimationCompositionKind,
        SemanticAnimationIntent, SemanticMutationTransaction, SemanticObjectTrackProperty,
        SemanticObjectTrackValues, SemanticVec3, TrackTiming,
    };
    let mut scene = Scene::new();
    let mut button = scene.rectangle(2.0, 1.0).unwrap();
    button.set_fill(0.2, 0.4, 0.7, 1.0).unwrap();
    scene.add(&button).unwrap();
    scene
        .on_click_playback(&button, PlaybackControl::Restart)
        .unwrap();
    let mut marker = scene.circle(0.25).unwrap();
    marker.set_translation(-1.5, 1.5).unwrap();
    marker.set_fill(0.2, 0.8, 0.4, 1.0).unwrap();
    scene.add(&marker).unwrap();
    // Distinct nonperiodic stereo carriers make an incorrect seek offset or
    // channel mapping observable. Expected PCM still comes from the shared mixer.
    let mut samples = Vec::with_capacity(8 * 48_000 * 2);
    for frame in 0..8 * 48_000 {
        let t = f64::from(frame) / 48_000.0;
        for (carrier, sweep) in [(733.0, 71.0), (1_319.0, 113.0)] {
            samples.push((std::f64::consts::TAU * (carrier * t + sweep * t * t)).sin() as f32);
        }
    }
    scene
        .audio(
            AudioResource::from_interleaved(48_000, 2, samples, 8 * 48_000 * 8).unwrap(),
            AudioClipOptions {
                gain: 0.15,
                ..Default::default()
            },
        )
        .unwrap();
    for (name, time) in [("Begin", 0.0), ("Second", 2.0), ("Third", 4.0)] {
        scene
            .next_section_at(name, SectionType::Normal, time)
            .unwrap();
    }
    let mut tx = SemanticMutationTransaction::new();
    let track = tx.create_object_property_track(
        marker.node_id(),
        SemanticObjectTrackProperty::Position,
        SemanticObjectTrackValues::Vec3 {
            from: SemanticVec3::new(-1.5, 1.5, 0.0),
            to: SemanticVec3::new(1.5, 1.5, 0.0),
        },
        TrackTiming::new(0.0, 8.0, RateFunction::Linear),
        CompositionTimeMap::identity(),
    );
    let applied = tx
        .apply(&mut scene.integration_store().borrow_mut())
        .unwrap();
    let animation = scene
        .declare_animation(
            SemanticAnimationIntent::Composition {
                kind: SemanticAnimationCompositionKind::Parallel,
                children: vec![applied.resolve(track).unwrap()],
            },
            AnimationOptions::new(),
        )
        .unwrap();
    (
        scene
            .execution_session_with_animation_root(&animation)
            .unwrap(),
        scene.section_navigation(),
    )
}

struct Run {
    name: &'static str,
    clock: RealtimeClock,
    picture: Instant,
}

struct Command {
    name: &'static str,
    at: Instant,
    time: f64,
    run: usize,
}

struct Harness {
    app: NativeApp,
    expected: PreparedAudioMix,
    directory: PathBuf,
    started: Instant,
    first_picture: Option<Instant>,
    last_picture: Option<Instant>,
    pending_picture: Option<Instant>,
    pending_run: Option<(&'static str, f64)>,
    runs: Vec<Run>,
    commands: Vec<Command>,
    phase: usize,
    phase_started: Instant,
    held_time: f64,
    picture_hold: Option<Instant>,
    silent_preparation: Duration,
    max_streams: usize,
    max_display_lag: f64,
    scene_revision: u64,
    execution_revision: u64,
    presents: usize,
    complete: bool,
}

impl Harness {
    fn record(&mut self, name: &'static str, at: Instant) {
        self.commands.push(Command {
            name,
            at,
            time: self.app.execution.frame_time(),
            run: self.runs.len().saturating_sub(1),
        });
        self.phase_started = at;
    }

    fn invariant(&mut self) {
        let context = self.app.session().publication_context();
        assert_eq!(context.scene_revision().get(), self.scene_revision);
        assert_eq!(context.execution_revision().get(), self.execution_revision);
        let streams = usize::from(self.app.audio_output.is_some())
            + usize::from(self.app.releasing_audio_output.is_some());
        self.max_streams = self.max_streams.max(streams);
        assert!(streams <= 2);
    }

    fn expect_run(&mut self, name: &'static str, time: f64) {
        self.pending_run = Some((name, time));
        self.pending_picture = None;
        assert_eq!(self.app.execution.frame_time(), time);
        assert!(self.app.audio_output.as_ref().unwrap().clock().is_none());
    }

    fn observe_anchor(&mut self) {
        let Some((name, time)) = self.pending_run else {
            return;
        };
        let Some(clock) = self.app.audio_output.as_ref().and_then(|o| o.clock()) else {
            return;
        };
        assert_eq!(
            clock.scene_origin, time,
            "replacement must start at the exact admitted target"
        );
        let picture = self
            .pending_picture
            .expect("device admitted before target picture");
        assert!(clock.wall_origin + Duration::from_millis(1) >= picture);
        assert!(
            self.app.releasing_audio_output.is_none(),
            "new output overlapped an old release"
        );
        write_expected(
            &mut self.expected,
            time,
            &self.directory.join(format!("{name}.f32")),
        );
        self.runs.push(Run {
            name,
            clock,
            picture,
        });
        self.pending_run = None;
        self.phase_started = clock.wall_origin;
        eprintln!(
            "native controls fixture: {name} source={time:.9}, anchor={:.9}s",
            clock.wall_origin.duration_since(self.started).as_secs_f64()
        );
    }

    fn render(&mut self) {
        self.app.force_full_redraw = true;
        self.app.redraw().unwrap();
        assert!(self.app.last_geometry_draw_calls > 0);
        self.presents += 1;
        self.last_picture = Some(Instant::now());
    }

    fn picture_due(&self, now: Instant) -> bool {
        self.app.publication_pending()
            || self
                .last_picture
                .is_none_or(|last| now.duration_since(last) >= PICTURE_INTERVAL)
    }

    fn click_restart(&mut self) {
        let window = self.app.window.as_ref().unwrap();
        let size = window.inner_size();
        let scale = window.scale_factor();
        self.render();
        self.app
            .dispatch_pointer_position(
                PhysicalPosition::new(f64::from(size.width) / 2.0, f64::from(size.height) / 2.0),
                size,
                scale,
            )
            .unwrap();
        for state in [ElementState::Pressed, ElementState::Released] {
            // Every edge borrows a real retained/presented target picture.
            self.render();
            self.app
                .dispatch_pointer_button(MouseButton::Left, state, size, scale)
                .unwrap();
        }
        assert_eq!(self.app.execution.frame_time(), 0.0);
        assert!(self.app.playback_paused);
        assert!(self.app.audio_output.is_none());
        self.render();
        assert_eq!(self.app.presented_frame_time, Some(0.0));
    }

    fn advance_phase(&mut self, now: Instant, event_loop: &ActiveEventLoop) {
        if self.pending_run.is_some() {
            return;
        }
        let elapsed = now
            .saturating_duration_since(self.phase_started)
            .as_secs_f64();
        match self.phase {
            0 if elapsed >= 0.40 => {
                self.app.set_viewer_volume(0.25).unwrap();
                self.record("volume_quarter", now);
            }
            1 if elapsed >= 0.25 => {
                self.app
                    .dispatch_playback_control(PlaybackControl::ToggleAudioMuted, now)
                    .unwrap();
                self.record("mute", now);
            }
            2 if elapsed >= 0.25 => {
                self.app.set_viewer_muted(false).unwrap();
                self.record("unmute", now);
            }
            3 if elapsed >= 0.25 => {
                self.app.set_viewer_volume(1.0).unwrap();
                self.record("volume_full", now);
            }
            4 if elapsed >= 0.25 => {
                assert_eq!(
                    self.app
                        .audio_output
                        .as_ref()
                        .unwrap()
                        .clock()
                        .unwrap()
                        .wall_origin,
                    self.runs[0].clock.wall_origin,
                    "viewer gain restarted the source"
                );
                self.app.pause_playback(now).unwrap();
                self.held_time = self.app.execution.frame_time();
                self.record("pause_one", now);
            }
            5 if elapsed >= 0.30 => {
                assert_eq!(self.app.execution.frame_time(), self.held_time);
                assert!(
                    self.app.audio_output.is_none() && self.app.releasing_audio_output.is_none()
                );
                self.app.resume_playback().unwrap();
                self.record("resume", now);
                self.expect_run("resumed", self.held_time);
            }
            6 if elapsed >= 0.35 => {
                let original = self.app.audio_output.as_ref().unwrap().clock().unwrap();
                self.record("rapid_seek", now);
                for index in 0..16 {
                    self.app
                        .seek_playback(1.5 + f64::from(index) * 0.05)
                        .unwrap();
                    self.invariant();
                    assert!(self.app.audio_output.as_ref().unwrap().clock().is_none());
                    assert_eq!(
                        self.app
                            .releasing_audio_output
                            .as_ref()
                            .unwrap()
                            .clock()
                            .unwrap()
                            .wall_origin,
                        original.wall_origin,
                        "rapid seeks replaced the original release"
                    );
                }
                self.expect_run("seeked", 2.25);
                self.picture_hold = Some(Instant::now() + Duration::from_millis(150));
            }
            7 if elapsed >= 0.35 => {
                self.app.pause_playback(now).unwrap();
                self.held_time = self.app.execution.frame_time();
                self.record("pause_two", now);
            }
            8 if elapsed >= 0.30 => {
                assert_eq!(self.app.execution.frame_time(), self.held_time);
                assert!(
                    self.app.audio_output.is_none() && self.app.releasing_audio_output.is_none()
                );
                self.app
                    .dispatch_playback_control(PlaybackControl::NextSection, now)
                    .unwrap();
                assert_eq!(self.app.execution.frame_time(), 4.0);
                assert!(self.app.playback_paused);
                self.render();
                assert_eq!(self.app.presented_frame_time, Some(4.0));
                self.record("next_section", Instant::now());
                self.app
                    .dispatch_playback_control(PlaybackControl::PreviousSection, Instant::now())
                    .unwrap();
                assert_eq!(self.app.execution.frame_time(), 2.0);
                self.render();
                assert_eq!(self.app.presented_frame_time, Some(2.0));
                self.record("previous_section", Instant::now());
                self.click_restart();
                self.record("scene_restart", Instant::now());
            }
            9 if elapsed >= 0.20 => {
                assert_eq!(self.app.execution.frame_time(), 0.0);
                self.app
                    .dispatch_playback_control(PlaybackControl::TogglePlayback, now)
                    .unwrap();
                self.record("resume_restart", now);
                self.expect_run("restarted", 0.0);
            }
            10 if elapsed >= 0.35 => {
                self.app.seek_playback(7.8).unwrap();
                self.record("seek_end", now);
                self.expect_run("ending", 7.8);
            }
            11 if elapsed >= 0.40 => {
                assert_eq!(self.app.execution.frame_time(), 8.0);
                assert_eq!(self.app.presented_frame_time, Some(8.0));
                assert!(
                    self.app.audio_output.is_none() && self.app.releasing_audio_output.is_none()
                );
                self.record("complete", now);
                self.complete = true;
                event_loop.exit();
            }
            _ => return,
        }
        self.phase += 1;
        self.invariant();
    }
}

impl ApplicationHandler for Harness {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.app.resumed(event_loop);
        if self.app.error.is_some() {
            return;
        }
        if let Some(path) = std::env::var_os("NOON_NATIVE_AUDIO_PULSE_PLUGIN") {
            let path = PathBuf::from(path).canonicalize().unwrap();
            assert!(std::fs::read_to_string("/proc/self/maps")
                .unwrap()
                .lines()
                .any(|line| line.ends_with(path.to_str().unwrap())));
        }
        let begin = Instant::now();
        while begin.elapsed() < Duration::from_millis(250) {
            let output = self.app.audio_output.as_ref().unwrap();
            output.check(Instant::now()).unwrap();
            assert!(output.clock().is_none());
            assert!(self.app.pointer.presented.is_none());
            std::thread::sleep(Duration::from_millis(5));
        }
        self.silent_preparation = begin.elapsed();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let redraw = matches!(event, WindowEvent::RedrawRequested);
        if redraw
            && self
                .picture_hold
                .is_some_and(|until| Instant::now() < until)
        {
            return;
        }
        if redraw && !self.picture_due(Instant::now()) {
            return;
        }
        self.app.window_event(event_loop, id, event);
        if self.app.error.is_some() || !redraw {
            return;
        }
        let Some(time) = self.app.presented_frame_time else {
            return;
        };
        let now = Instant::now();
        self.first_picture.get_or_insert(now);
        self.last_picture = Some(now);
        self.presents += 1;
        if self.pending_run.is_some_and(|(_, target)| target == time) {
            self.pending_picture.get_or_insert(now);
        }
        if let Some(clock) = self.app.audio_output.as_ref().and_then(|o| o.clock()) {
            let lag = clock.scene_time_at(now).min(8.0) - time;
            assert!(
                lag >= -0.001,
                "picture ran ahead of the admitted audio clock: {lag}"
            );
            self.max_display_lag = self.max_display_lag.max(lag);
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        let (elapsed, budget) = self.first_picture.map_or(
            (now.duration_since(self.started), Duration::from_secs(45)),
            |first| (now.duration_since(first), Duration::from_secs(15)),
        );
        assert!(elapsed < budget,
            "native controls deadline: phase={}, elapsed={elapsed:?}, runtime={}, pending={:?}, current={}, releasing={}",
            self.phase, self.app.execution.frame_time(), self.pending_run,
            self.app.audio_output.is_some(), self.app.releasing_audio_output.is_some());
        if self.picture_hold.is_some_and(|until| now < until) {
            self.app.check_audio(now).unwrap();
            assert_eq!(self.app.execution.frame_time(), 2.25);
            assert!(self.app.audio_output.as_ref().unwrap().clock().is_none());
            assert!(!self.app.audio_picture_ready);
            event_loop.set_control_flow(ControlFlow::WaitUntil(now + Duration::from_millis(5)));
            return;
        }
        self.picture_hold = None;
        if self.picture_due(now) {
            self.app.about_to_wait(event_loop);
        } else {
            // Audio health and command observations retain their 5 ms test
            // wake. Only the fixture's request/delivery of pictures is paced.
            self.app.check_audio(now).unwrap();
        }
        if self.app.error.is_some() {
            return;
        }
        self.observe_anchor();
        self.invariant();
        self.advance_phase(Instant::now(), event_loop);
        // Test orchestration may wake a paused viewer; it never advances Runtime
        // or device time and is absent from the product host.
        event_loop.set_control_flow(ControlFlow::WaitUntil(
            Instant::now() + Duration::from_millis(5),
        ));
    }

    fn exiting(&mut self, event_loop: &ActiveEventLoop) {
        self.app.exiting(event_loop);
    }
}

fn write_expected(mixer: &mut PreparedAudioMix, origin: f64, path: &Path) {
    let mut file = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
    let mut block = [0.0_f32; 2_048];
    for first in (0..96_000_u64).step_by(1_024) {
        let count = (96_000 - first).min(1_024) as usize * 2;
        mixer
            .mix_audio_block(origin, first, &mut block[..count])
            .unwrap();
        for value in &block[..count] {
            file.write_all(&value.to_le_bytes()).unwrap();
        }
    }
    file.flush().unwrap();
}

#[test]
#[ignore = "requires X11, WGPU and a captured private PulseAudio output; selected by native audio qualification"]
fn native_audio_controls_device_smoke() {
    let directory = PathBuf::from(std::env::var_os("NOON_NATIVE_AUDIO_PROOF_DIR").unwrap());
    std::fs::create_dir_all(&directory).unwrap();
    let (session, sections) = fixture();
    let context = session.publication_context();
    // Freeze the authored reference before any viewer command. Later volume or
    // mute cannot change the oracle by mutating the session under test.
    let expected = session
        .prepare_audio_mix(AudioMixFormat::STEREO_48_KHZ, 1_024)
        .unwrap();
    let now = Instant::now();
    let mut harness = Harness {
        app: NativeApp::new(
            session,
            NativeViewportConfig {
                title: "Noon native playback controls proof".into(),
                width: 320,
                height: 180,
                playback_controls: true,
                section_navigation: sections,
                ..Default::default()
            },
        ),
        expected,
        directory: directory.clone(),
        started: now,
        first_picture: None,
        last_picture: None,
        pending_picture: None,
        pending_run: Some(("initial", 0.0)),
        runs: Vec::new(),
        commands: Vec::new(),
        phase: 0,
        phase_started: now,
        held_time: 0.0,
        picture_hold: None,
        silent_preparation: Duration::ZERO,
        max_streams: 0,
        max_display_lag: 0.0,
        scene_revision: context.scene_revision().get(),
        execution_revision: context.execution_revision().get(),
        presents: 0,
        complete: false,
    };
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    builder.build().unwrap().run_app(&mut harness).unwrap();
    assert!(harness.app.error.is_none(), "{:?}", harness.app.error);
    assert!(harness.complete);
    assert_eq!(harness.runs.len(), 5);
    assert_eq!(harness.max_streams, 2);
    harness.invariant();
    let seconds = |at: Instant| at.duration_since(harness.started).as_secs_f64();
    let runs = harness.runs.iter().map(|run| format!(
        "{{\"name\":\"{}\",\"source_origin\":{:.17},\"anchor_seconds\":{:.9},\"picture_seconds\":{:.9}}}",
        run.name, run.clock.scene_origin, seconds(run.clock.wall_origin), seconds(run.picture),
    )).collect::<Vec<_>>().join(",\n");
    let commands = harness
        .commands
        .iter()
        .map(|command| {
            format!(
                "{{\"name\":\"{}\",\"wall_seconds\":{:.9},\"scene_time\":{:.17},\"run\":{}}}",
                command.name,
                seconds(command.at),
                command.time,
                command.run,
            )
        })
        .collect::<Vec<_>>()
        .join(",\n");
    let report = format!(
        concat!(
        "{{\n\"sample_rate\":48000,\"channels\":2,\"duration\":8.0,\n",
        "\"prepared_silent_seconds\":{:.9},\"displayed_end_time\":8.0,\n",
        "\"scene_revision\":{},\"execution_revision\":{},\"authored_revisions_unchanged\":true,\n",
        "\"rapid_seek_count\":16,\"max_live_streams\":{},\"scene_click_restart\":true,\n",
        "\"releases_retired\":true,\"displayed_frames\":{},\"max_display_lag_seconds\":{:.9},\n",
            "\"fixture_refresh_hz\":60,\n",
        "\"runs\":[{}],\n\"commands\":[{}]\n}}\n"),
        harness.silent_preparation.as_secs_f64(),
        harness.scene_revision,
        harness.execution_revision,
        harness.max_streams,
        harness.presents,
        harness.max_display_lag,
        runs,
        commands,
    );
    std::fs::write(directory.join("report.json"), report).unwrap();
}
