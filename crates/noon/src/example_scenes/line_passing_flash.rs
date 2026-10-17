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

/// Compose two finite signal trains using the existing Rust animation scheduler.
///
/// The upper rail carries long cyan pulses from left to right; the lower rail
/// carries short yellow pulses in the opposite direction. A separate dim rail
/// stays visible before, during, and after the transient signals. Each train has
/// nine signals lasting 1.6 seconds, staggered by 0.2 seconds (lag ratio 0.125).
///
/// This deliberately tests composition before adding a new flow primitive. It
/// is not a dense/infinite patterned-stroke implementation, a round-dot profile,
/// or a claim of support for moving/curved source paths. The same declarations,
/// lowering, and timeline drive native Rust and the paired Python scene; no
/// callback, private clock, or per-frame geometry reconstruction is installed.
/// Pair: `web/python/examples/ordinary_signal_trains.py`.
pub fn train_session() -> Result<crate::ExecutionSession, String> {
    let build = || -> Result<_, Box<dyn std::error::Error>> {
        use crate::AnimationCompositionRequest;
        use noon_core::SemanticAnimationCompositionKind;

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
        // Requests are inert: detached pulses are not scheduled at bootstrap.
        // The live operation below admits all targets and declares their tracks
        // together, through the same atomic composition used by the frontends.
        let together = AnimationCompositionRequest::Composition {
            kind: SemanticAnimationCompositionKind::Parallel,
            children: trains
                .iter()
                .map(|(signals, time_width)| AnimationCompositionRequest::Composition {
                    kind: SemanticAnimationCompositionKind::Parallel,
                    children: signals
                        .iter()
                        .map(|signal| AnimationCompositionRequest::PassingFlash {
                            target: signal,
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

        let mut execution = scene.execution_session()?;
        {
            let mut live = scene.live(&mut execution);
            let before = live.wait_segment(0.5)?;
            live.advance_segment_to(before, before.end_time())?;
            live.complete_segment(before)?;
            let signals =
                live.declare_and_activate_composition(&together, AnimationOptions::new())?;
            live.advance_segment_to(signals, signals.end_time())?;
            live.complete_segment(signals)?;
            let after = live.wait_segment(0.5)?;
            live.advance_segment_to(after, after.end_time())?;
            live.complete_segment(after)?;
        }
        execution.seek(0.0)?;
        Ok(execution)
    };
    build().map_err(|error| error.to_string())
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

    #[test]
    fn finite_signal_trains_keep_the_rails_and_stagger_transient_signals() {
        let mut session = super::train_session().unwrap();
        let initial = session.frame().clone();
        assert_eq!(initial.objects.len(), 20);
        let identities: Vec<_> = initial.objects.iter().map(|object| object.id).collect();
        // Independent schedule: nine 1.6-second signals per rail, starting at
        // 0.5 + i * 0.2. Exact starts/ends have zero visible window.
        for (time, expected_signals) in [(0.0, 0), (1.5, 10), (2.0, 16), (4.2, 0)] {
            let frame = session.advance_to(time).unwrap();
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
                assert_eq!(frame.objects[rail], initial.objects[rail]);
                assert_eq!(frame.reveal(rail), 1.0);
            }
            let visible = (2..frame.objects.len())
                .filter(|&index| frame.is_present(index) && frame.reveal(index) > 1.0e-5)
                .count();
            assert_eq!(visible, expected_signals, "signal schedule at t={time}");
        }
        assert!((session.frame().time - super::TRAIN_DURATION).abs() < 1.0e-9);
    }

    #[test]
    fn finite_signal_trains_seek_and_replay_without_a_callback_clock() {
        let mut session = super::train_session().unwrap();
        let mut forward = Vec::new();
        for time in [0.0, 0.55, 0.9, 1.5, 2.0, 2.6, 3.55, 3.9, 4.2] {
            forward.push(session.advance_to(time).unwrap().clone());
        }
        for expected in forward.iter().rev().chain(forward.iter()) {
            assert_eq!(session.seek(expected.time).unwrap(), expected);
        }
    }
}
