//! Exact analytic Line PassingFlash with transient membership and stable re-entry.

use std::rc::Rc;

use crate::{
    AnimationOptions, ContinuationStep, LiveContinuation, LiveProgram, LiveSession,
    ManimGeometryOptions, Mobject, RateFunction, Scene,
};

pub struct LinePassingFlash {
    line: Mobject,
    identity: noon_core::SemanticNodeId,
    stage: u8,
}

impl LiveContinuation for LinePassingFlash {
    type Error = String;

    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, String> {
        match self.stage {
            0 => {
                if live
                    .contains(&self.line)
                    .map_err(|error| error.to_string())?
                {
                    return Err("PassingFlash Line must begin detached".into());
                }
                self.stage = 1;
                live.wait_segment(0.25)
                    .map(ContinuationStep::Await)
                    .map_err(|error| error.to_string())
            }
            1 => {
                let segment = live
                    .declare_and_activate_passing_flash(
                        &self.line,
                        0.25,
                        AnimationOptions::new()
                            .run_time(2.0)
                            .rate_func(RateFunction::Linear),
                    )
                    .map_err(|error| error.to_string())?;
                if !live
                    .contains(&self.line)
                    .map_err(|error| error.to_string())?
                {
                    return Err("PassingFlash did not admit its transient Line".into());
                }
                self.stage = 2;
                Ok(ContinuationStep::Await(segment))
            }
            2 => {
                if live
                    .contains(&self.line)
                    .map_err(|error| error.to_string())?
                {
                    return Err("PassingFlash did not remove its transient Line".into());
                }
                if self.line.node_id() != self.identity {
                    return Err("PassingFlash changed the Line semantic identity".into());
                }
                live.add(&self.line).map_err(|error| error.to_string())?;
                if !live
                    .contains(&self.line)
                    .map_err(|error| error.to_string())?
                {
                    return Err(
                        "PassingFlash Line could not re-enter with the same identity".into(),
                    );
                }
                self.stage = 3;
                live.wait_segment(0.25)
                    .map(ContinuationStep::Await)
                    .map_err(|error| error.to_string())
            }
            3 => {
                self.stage = 4;
                Ok(ContinuationStep::Finished)
            }
            _ => Err("PassingFlash continuation resumed after completion".into()),
        }
    }
}

pub fn program() -> Result<LiveProgram<LinePassingFlash>, String> {
    let scene = Scene::new();
    let mut options =
        ManimGeometryOptions::line(-2.0, 0.0, 2.0, 0.0).map_err(|error| error.to_string())?;
    options
        .set_scale(1.25, 0.75)
        .map_err(|error| error.to_string())?;
    options
        .set_rotation(std::f64::consts::PI / 6.0)
        .map_err(|error| error.to_string())?;
    options
        .set_translation(0.5, -0.5)
        .map_err(|error| error.to_string())?;
    options.disable_fill();
    options
        .set_stroke_color(0.0, 1.0, 1.0, 1.0)
        .map_err(|error| error.to_string())?;
    options
        .set_stroke_width(0.08)
        .map_err(|error| error.to_string())?;
    let line = Mobject::from_manim_geometry(Rc::clone(scene.integration_store()), options)
        .map_err(|error| error.to_string())?;
    let identity = line.node_id();
    scene
        .into_live_program(LinePassingFlash {
            line,
            identity,
            stage: 0,
        })
        .map_err(|error| error.to_string())
}

/// Duration of the finite travelling-signal example, including its two holds.
pub const TRAIN_DURATION: f64 = 4.2;

/// Compose two finite signal trains using the ordinary live Rust scheduler.
///
/// Each rail stays present while its detached signal lines enter and leave at
/// their authored times. Live playback drives this continuation directly.
/// Recorded playback retains and seals the existing execution history before
/// seeking; merely rewinding an unrecorded session loses retired membership.
/// No separate clock, runtime model, or frontend updater is used.
///
/// This demonstrates finite analytic-Line pulses, not infinite dash phase or
/// flowing curves. Pair: web/python/examples/ordinary_signal_trains.py.
pub struct SignalTrains {
    trains: Vec<(Vec<Mobject>, f64)>,
    stage: u8,
}

impl LiveContinuation for SignalTrains {
    type Error = String;

    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, String> {
        use crate::AnimationCompositionRequest as Request;
        use noon_core::SemanticAnimationCompositionKind as Kind;

        match self.stage {
            0 => {
                self.stage = 1;
                live.wait_segment(0.5)
                    .map(ContinuationStep::Await)
                    .map_err(|error| error.to_string())
            }
            1 => {
                let request = Request::Composition {
                    kind: Kind::Parallel,
                    children: self
                        .trains
                        .iter()
                        .map(|(signals, time_width)| Request::Composition {
                            kind: Kind::Parallel,
                            children: signals
                                .iter()
                                .map(|target| Request::PassingFlash {
                                    target,
                                    time_width: *time_width,
                                    options: AnimationOptions::new()
                                        .run_time(1.6)
                                        .rate_func(RateFunction::Linear),
                                })
                                .collect(),
                            options: AnimationOptions::new()
                                .lag_ratio(0.125)
                                .rate_func(RateFunction::Linear),
                        })
                        .collect(),
                    options: AnimationOptions::new()
                        .run_time(3.2)
                        .lag_ratio(0.0)
                        .rate_func(RateFunction::Linear),
                };
                let segment = live
                    .declare_and_activate_composition(&request, AnimationOptions::new())
                    .map_err(|error| error.to_string())?;
                self.stage = 2;
                Ok(ContinuationStep::Await(segment))
            }
            2 => {
                for (signals, _) in &self.trains {
                    for signal in signals {
                        if live.contains(signal).map_err(|error| error.to_string())? {
                            return Err("PassingFlash did not retire a transient signal".into());
                        }
                    }
                }
                self.stage = 3;
                live.wait_segment(0.5)
                    .map(ContinuationStep::Await)
                    .map_err(|error| error.to_string())
            }
            3 => {
                self.stage = 4;
                Ok(ContinuationStep::Finished)
            }
            _ => Err("signal train continuation resumed after completion".into()),
        }
    }
}

fn train_scene() -> Result<(Scene, SignalTrains), String> {
    let build = || -> Result<_, Box<dyn std::error::Error>> {
        let mut scene = Scene::new();
        let mut trains = Vec::new();
        for (start, end, color, time_width) in [
            ((-3.0, 1.0), (3.0, 1.0), (0.0, 1.0, 1.0), 0.12),
            ((3.0, -1.0), (-3.0, -1.0), (1.0, 0.8, 0.0), 0.025),
        ] {
            let mut rail = scene.line(start, end)?;
            rail.set_stroke_color(0.25, 0.25, 0.25, 1.0)?;
            rail.set_stroke_width(0.025)?;
            rail.set_stroke_cap("butt")?;
            scene.add(&rail)?;

            let mut signals = Vec::new();
            for _ in 0..9 {
                let mut signal = scene.line(start, end)?;
                signal.set_stroke_color(color.0, color.1, color.2, 1.0)?;
                signal.set_stroke_width(0.1)?;
                signal.set_stroke_cap("butt")?;
                signals.push(signal);
            }
            trains.push((signals, time_width));
        }
        Ok((scene, SignalTrains { trains, stage: 0 }))
    };
    build().map_err(|error| error.to_string())
}

pub fn train_program() -> Result<LiveProgram<SignalTrains>, String> {
    let (scene, continuation) = train_scene()?;
    scene
        .into_live_program(continuation)
        .map_err(|error| error.to_string())
}

/// Record the same finite continuation once for ordinary retained playback.
///
/// Retention must start before the first admission or completion. The existing
/// runtime pins retired identities and retains local inverse revisions; no frame
/// images, second scene, special signal clock, or host callbacks are recorded.
/// After sealing, ordinary ExecutionSession::seek restores those revisions.
pub fn train_playback() -> Result<crate::SceneExecution, String> {
    let record = || -> Result<_, Box<dyn std::error::Error>> {
        let (scene, mut continuation) = train_scene()?;
        let mut execution = scene.execution_session()?;
        execution.begin_replay_retention(noon_runtime::ReplayLimits::default())?;
        {
            let mut live = scene.live(&mut execution);
            loop {
                let step = continuation.resume(&mut live)?;
                let ContinuationStep::Await(segment) = step else {
                    break;
                };
                live.advance_segment_to(segment, segment.end_time())?;
                live.complete_segment(segment)?;
            }
        }
        execution.seal_replay()?;
        let mut source = scene.into_execution_with_session(execution)?;
        source.seek(0.0)?;
        Ok(source)
    };
    record().map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use crate::{
        AnimationOptions, ContinuationStep, LiveContinuation, LiveProgram, LiveProgramStatus,
        Mobject, RustHostCallbackTable, Scene,
    };

    fn admit_completion<C>(
        program: &mut LiveProgram<C>,
        callbacks: &mut RustHostCallbackTable,
        time: f64,
    ) where
        C: LiveContinuation<Error = String>,
    {
        let status = program.drive_to(callbacks, time).unwrap();
        let LiveProgramStatus::PublicationPending(expected) = status else {
            panic!("expected publication at {time}, got {status:?}");
        };
        let context = program.take_renderer_publication().context();
        assert_eq!(context, expected);
        program.admit_publication(context).unwrap();
    }

    #[test]
    fn passing_flash_removes_then_reuses_one_stable_line_slot() {
        let mut program = super::program().unwrap();
        let mut callbacks = RustHostCallbackTable::new();
        assert!(program.session().frame().objects.is_empty());
        assert!(matches!(
            program.resume().unwrap(),
            LiveProgramStatus::Awaiting(_)
        ));
        program.take_renderer_publication();
        assert_eq!(
            program.drive_to(&mut callbacks, 0.25).unwrap(),
            LiveProgramStatus::ReadyToResume
        );
        assert!(matches!(
            program.resume().unwrap(),
            LiveProgramStatus::Awaiting(_)
        ));
        assert_eq!(program.session().frame().objects.len(), 1);
        assert!(program.session().frame().is_present(0));
        program.take_renderer_publication();
        assert!(matches!(
            program.drive_to(&mut callbacks, 1.25).unwrap(),
            LiveProgramStatus::Awaiting(_)
        ));
        assert_eq!(program.session().frame().reveals[0], 0.25);
        admit_completion(&mut program, &mut callbacks, 2.25);
        assert_eq!(program.session().frame().objects.len(), 1);
        assert!(!program.session().frame().is_present(0));
        assert!(matches!(
            program.resume().unwrap(),
            LiveProgramStatus::Awaiting(_)
        ));
        assert_eq!(program.session().frame().objects.len(), 1);
        assert!(program.session().frame().is_present(0));
        program.take_renderer_publication();
        assert_eq!(
            program.drive_to(&mut callbacks, 2.5).unwrap(),
            LiveProgramStatus::ReadyToResume
        );
        assert_eq!(program.resume().unwrap(), LiveProgramStatus::Finished);
    }

    struct PresentLineFlash {
        line: Mobject,
        activated: bool,
    }

    impl LiveContinuation for PresentLineFlash {
        type Error = String;

        fn resume(
            &mut self,
            live: &mut crate::LiveSession<'_>,
        ) -> Result<ContinuationStep, Self::Error> {
            if self.activated {
                if live
                    .contains(&self.line)
                    .map_err(|error| error.to_string())?
                {
                    return Err("PassingFlash did not remove its initially present Line".into());
                }
                return Ok(ContinuationStep::Finished);
            }
            self.activated = true;
            live.declare_and_activate_passing_flash(
                &self.line,
                0.25,
                AnimationOptions::new().run_time(1.0),
            )
            .map(ContinuationStep::Await)
            .map_err(|error| error.to_string())
        }
    }

    #[test]
    fn passing_flash_accepts_and_removes_an_initially_present_line() {
        let mut scene = Scene::new();
        let line = scene.line((-1.0, 0.0), (1.0, 0.0)).unwrap();
        scene.add(&line).unwrap();
        let mut program = scene
            .into_live_program(PresentLineFlash {
                line,
                activated: false,
            })
            .unwrap();
        let mut callbacks = RustHostCallbackTable::new();
        assert!(matches!(
            program.resume().unwrap(),
            LiveProgramStatus::Awaiting(_)
        ));
        admit_completion(&mut program, &mut callbacks, 1.0);
        assert_eq!(program.resume().unwrap(), LiveProgramStatus::Finished);
    }

    // Emulate the native host's segment drive and renderer-admission barrier.
    // Recorded, post-completion seeking is tested separately below.
    fn sample_program(
        program: &mut LiveProgram<super::SignalTrains>,
        callbacks: &mut crate::RustHostCallbackTable,
        time: f64,
    ) -> noon_runtime::FrameState {
        loop {
            match program.status() {
                LiveProgramStatus::ReadyToResume => {
                    if program.session().frame().time >= time {
                        break;
                    }
                    program.resume().unwrap();
                }
                LiveProgramStatus::Awaiting(_) => {
                    if program.session().frame().time >= time {
                        break;
                    }
                    program.drive_to(callbacks, time).unwrap();
                }
                LiveProgramStatus::PublicationPending(expected) => {
                    let publication = program.take_renderer_publication().context();
                    assert_eq!(publication, expected);
                    program.admit_publication(publication).unwrap();
                }
                LiveProgramStatus::Finished => break,
                LiveProgramStatus::Terminal => panic!("signal train terminated"),
            }
        }
        assert_eq!(program.session().frame().time, time);
        program.session().frame().clone()
    }

    #[test]
    fn finite_signal_trains_keep_rails_and_stagger_transient_signals() {
        let mut program = super::train_program().unwrap();
        let mut callbacks = crate::RustHostCallbackTable::new();
        let beginning = sample_program(&mut program, &mut callbacks, 0.0);
        assert_eq!(beginning.objects.len(), 2);
        assert!(beginning.is_present(0));
        assert!(beginning.is_present(1));

        // The active composition admits the eighteen detached pulse objects.
        let initial = sample_program(&mut program, &mut callbacks, 0.55);
        assert_eq!(initial.objects.len(), 20);
        let identities: Vec<_> = initial.objects.iter().map(|object| object.id).collect();
        for (time, expected_signals) in [(1.5, 10), (2.0, 16), (4.15, 0)] {
            let frame = sample_program(&mut program, &mut callbacks, time);
            assert_eq!(frame.objects.len(), 20);
            assert_eq!(
                frame
                    .objects
                    .iter()
                    .map(|object| object.id)
                    .collect::<Vec<_>>(),
                identities
            );
            for rail in 0..2 {
                assert!(frame.is_present(rail));
                assert_eq!(frame.objects[rail], beginning.objects[rail]);
                assert_eq!(frame.reveal(rail), 1.0);
            }
            let visible = (2..frame.objects.len())
                .filter(|&index| frame.is_present(index) && frame.reveal(index) > 1.0e-5)
                .count();
            assert_eq!(visible, expected_signals, "signal schedule at t={time}");
        }
        sample_program(&mut program, &mut callbacks, super::TRAIN_DURATION);
        assert_eq!(program.resume().unwrap(), LiveProgramStatus::Finished);
    }

    #[test]
    fn finite_signal_trains_repeat_from_fresh_program_without_callback_clock() {
        fn sample() -> Vec<noon_runtime::FrameState> {
            let mut program = super::train_program().unwrap();
            let mut callbacks = crate::RustHostCallbackTable::new();
            [0.0, 0.55, 0.9, 1.5, 2.0, 2.6, 3.55, 3.9, 4.15]
                .into_iter()
                .map(|time| sample_program(&mut program, &mut callbacks, time))
                .collect()
        }
        assert_eq!(sample(), sample());
    }

    #[test]
    fn finite_signal_trains_restore_retired_membership_with_existing_replay_retention() {
        let mut session = super::train_playback().unwrap();
        assert!(session.session().replay_is_sealed());
        let reference = session.seek(2.0).unwrap().clone();
        let retained = session.session().replay_stats();
        for (time, expected_signals) in [
            (4.2, 0),
            (1.5, 10),
            (0.0, 0),
            (2.0, 16),
            (4.15, 0),
            (0.9, 4),
            (1.5, 10),
        ] {
            let frame = session.seek(time).unwrap();
            for rail in 0..2 {
                assert!(frame.is_present(rail));
                assert_eq!(frame.objects[rail], reference.objects[rail]);
                assert_eq!(frame.reveal(rail), 1.0);
            }
            for (index, object) in frame.objects.iter().enumerate() {
                if frame.is_present(index) {
                    assert_eq!(object.id, reference.objects[index].id);
                }
            }
            let visible = (2..frame.objects.len())
                .filter(|&index| frame.is_present(index) && frame.reveal(index) > 1.0e-5)
                .count();
            assert_eq!(visible, expected_signals, "retained signals at t={time}");
        }
        let after = session.session().replay_stats();
        assert_eq!(after.revisions_retained, retained.revisions_retained);
        assert_eq!(after.payloads_retained, retained.payloads_retained);
    }
}
