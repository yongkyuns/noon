//! Shared Create/Uncreate reveal timing for plain Text and Text families.

use crate::{
    AnimationCompositionRequest, AnimationOptions, ContinuationStep, LiveContinuation, LiveProgram,
    LiveSession, Mobject, MobjectFamily, RateFunction, Scene, SemanticAnimationCompositionKind,
    SemanticNodeId, TransformToRequest,
};

pub struct TextFamilyReveal {
    left: Mobject,
    right: Mobject,
    family: MobjectFamily,
    solo: Mobject,
    moving: Mobject,
    moving_target: Mobject,
    left_target: Mobject,
    root: SemanticNodeId,
    stage: u8,
}

impl LiveContinuation for TextFamilyReveal {
    type Error = String;

    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, String> {
        match self.stage {
            0 => {
                let rejected = live.declare_and_activate_animation_composition(
                    SemanticAnimationCompositionKind::Parallel,
                    &[
                        AnimationCompositionRequest::FamilyReveal {
                            target: &self.family,
                            reverse: false,
                            options: AnimationOptions::new()
                                .run_time(0.25)
                                .rate_func(RateFunction::Linear)
                                .lag_ratio(0.25),
                        },
                        AnimationCompositionRequest::TransformTo(TransformToRequest::new(
                            &self.left,
                            &self.left_target,
                            AnimationOptions::new()
                                .run_time(0.25)
                                .rate_func(RateFunction::Linear),
                        )),
                    ],
                    AnimationOptions::new().rate_func(RateFunction::Linear),
                    AnimationOptions::new(),
                );
                if rejected.is_ok()
                    || noon_core::semantic_scene_root_contains(
                        &self.family.store().borrow(),
                        self.root,
                        self.left.node_id(),
                    )
                    .map_err(|error| error.to_string())?
                {
                    return Err("overlapping family Create admitted a partial family root".into());
                }

                let segment = live
                    .declare_and_activate_animation_composition(
                        SemanticAnimationCompositionKind::Parallel,
                        &[
                            AnimationCompositionRequest::FamilyReveal {
                                target: &self.family,
                                reverse: false,
                                options: AnimationOptions::new()
                                    .run_time(2.0)
                                    .rate_func(RateFunction::Linear)
                                    .lag_ratio(0.25),
                            },
                            AnimationCompositionRequest::TextReveal {
                                target: &self.solo,
                                reverse: false,
                                options: AnimationOptions::new()
                                    .run_time(2.0)
                                    .rate_func(RateFunction::Linear)
                                    .lag_ratio(0.25),
                            },
                            AnimationCompositionRequest::TransformTo(TransformToRequest::new(
                                &self.moving,
                                &self.moving_target,
                                AnimationOptions::new()
                                    .run_time(2.0)
                                    .rate_func(RateFunction::Linear),
                            )),
                        ],
                        AnimationOptions::new().rate_func(RateFunction::Linear),
                        AnimationOptions::new(),
                    )
                    .map_err(|error| error.to_string())?;
                self.stage = 1;
                Ok(ContinuationStep::Await(segment))
            }
            1 => {
                if !live
                    .contains(&self.moving)
                    .map_err(|error| error.to_string())?
                    || !live
                        .contains(&self.solo)
                        .map_err(|error| error.to_string())?
                    || (live
                        .effective(&self.moving)
                        .map_err(|error| error.to_string())?
                        .transform
                        .translation
                        .x
                        - 3.0)
                        .abs()
                        > 1.0e-6
                {
                    return Err("Text Create did not complete its disjoint composition".into());
                }
                let store = self.family.store().borrow();
                let family = store
                    .node(self.family.node_id())
                    .ok_or("Text family identity disappeared after Create")?;
                let left_reachable =
                    noon_core::semantic_scene_root_contains(&store, self.root, self.left.node_id())
                        .map_err(|error| error.to_string())?;
                let right_reachable = noon_core::semantic_scene_root_contains(
                    &store,
                    self.root,
                    self.right.node_id(),
                )
                .map_err(|error| error.to_string())?;
                if family.parents().len() != 1 || !left_reachable || !right_reachable {
                    return Err("family Create changed authoritative family identity".into());
                }
                drop(store);
                self.stage = 2;
                live.declare_and_activate_animation_composition(
                    SemanticAnimationCompositionKind::Parallel,
                    &[
                        AnimationCompositionRequest::FamilyReveal {
                            target: &self.family,
                            reverse: true,
                            options: AnimationOptions::new()
                                .run_time(1.0)
                                .rate_func(RateFunction::Linear)
                                .lag_ratio(0.25)
                                .introducer(false)
                                .remover(true)
                                .reverse_rate_function(true),
                        },
                        AnimationCompositionRequest::TextReveal {
                            target: &self.solo,
                            reverse: true,
                            options: AnimationOptions::new()
                                .run_time(1.0)
                                .rate_func(RateFunction::Linear)
                                .lag_ratio(0.25)
                                .introducer(false)
                                .remover(true)
                                .reverse_rate_function(true),
                        },
                    ],
                    AnimationOptions::new().rate_func(RateFunction::Linear),
                    AnimationOptions::new(),
                )
                .map(ContinuationStep::Await)
                .map_err(|error| error.to_string())
            }
            2 => {
                let store = self.family.store().borrow();
                let family = store
                    .node(self.family.node_id())
                    .ok_or("Text family identity disappeared after Uncreate")?;
                let left_reachable =
                    noon_core::semantic_scene_root_contains(&store, self.root, self.left.node_id())
                        .map_err(|error| error.to_string())?;
                let right_reachable = noon_core::semantic_scene_root_contains(
                    &store,
                    self.root,
                    self.right.node_id(),
                )
                .map_err(|error| error.to_string())?;
                if !family.parents().is_empty()
                    || left_reachable
                    || right_reachable
                    || live
                        .contains(&self.solo)
                        .map_err(|error| error.to_string())?
                    || !live
                        .contains(&self.moving)
                        .map_err(|error| error.to_string())?
                {
                    return Err("Uncreate did not remove only its Text roots".into());
                }
                drop(store);
                self.stage = 3;
                live.wait_segment(0.25)
                    .map(ContinuationStep::Await)
                    .map_err(|error| error.to_string())
            }
            3 => {
                self.stage = 4;
                Ok(ContinuationStep::Finished)
            }
            _ => Err("Text family reveal continuation resumed after completion".into()),
        }
    }
}

pub fn program() -> Result<LiveProgram<TextFamilyReveal>, String> {
    let mut scene = Scene::new();
    let root = scene.root();
    let mut left = scene.text("I").map_err(|error| error.to_string())?;
    left.set_translation(-3.0, 0.75)?;
    let mut right = scene.text("LONG").map_err(|error| error.to_string())?;
    right.set_translation(0.0, 0.75)?;
    let family = scene.family(&[(&left).into(), (&right).into()])?;
    let mut solo = scene.text("ONE").map_err(|error| error.to_string())?;
    solo.set_translation(-2.0, -1.25)?;
    let mut moving = scene.square(0.6)?;
    moving.set_translation(1.0, -1.25)?;
    scene.add(&moving).map_err(|error| error.to_string())?;
    let mut moving_target = moving.target_editor()?;
    moving_target.shift(2.0, 0.0)?;
    let mut left_target = left.target_editor()?;
    left_target.shift(0.0, 1.0)?;
    scene
        .into_live_program(TextFamilyReveal {
            left,
            right,
            family,
            solo,
            moving,
            moving_target,
            left_target,
            root,
            stage: 0,
        })
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LiveProgramStatus, RustHostCallbackTable};

    fn admit_completion(
        program: &mut LiveProgram<TextFamilyReveal>,
        callbacks: &mut RustHostCallbackTable,
        time: f64,
    ) {
        let status = program.drive_to(callbacks, time).unwrap();
        let LiveProgramStatus::PublicationPending(expected) = status else {
            panic!("expected publication at {time}, got {status:?}");
        };
        let context = program.take_renderer_publication().context();
        assert_eq!(context, expected);
        program.admit_publication(context).unwrap();
    }

    fn progress(program: &LiveProgram<TextFamilyReveal>, object: usize, count: u32) -> Vec<f32> {
        let frame = program.session().planned_family_frame();
        let leaf = frame
            .planned_family_leaf(program.session().family_animation_plans(), object)
            .unwrap()
            .unwrap();
        assert_eq!(leaf.span().member_count, count);
        (0..count)
            .map(|member| leaf.member_progress(member).unwrap())
            .collect()
    }

    #[test]
    fn native_text_reveal_uses_global_family_timing_and_cleans_up_both_roots() {
        let mut program = program().unwrap();
        let mut callbacks = RustHostCallbackTable::new();
        assert!(matches!(
            program.resume().unwrap(),
            LiveProgramStatus::Awaiting(_)
        ));
        assert!(matches!(
            program.drive_to(&mut callbacks, 1.0).unwrap(),
            LiveProgramStatus::Awaiting(_)
        ));
        assert_eq!(progress(&program, 1, 1), vec![1.0]);
        assert_eq!(progress(&program, 2, 4), vec![0.75, 0.5, 0.25, 0.0]);
        assert!(program.session().frame().is_present(3));

        admit_completion(&mut program, &mut callbacks, 2.0);
        assert!(matches!(
            program.resume().unwrap(),
            LiveProgramStatus::Awaiting(_)
        ));
        assert!(matches!(
            program.drive_to(&mut callbacks, 2.5).unwrap(),
            LiveProgramStatus::Awaiting(_)
        ));
        assert_eq!(progress(&program, 1, 1), vec![0.0]);
        assert_eq!(progress(&program, 2, 4), vec![0.25, 0.5, 0.75, 1.0]);

        admit_completion(&mut program, &mut callbacks, 3.0);
        assert!(matches!(
            program.resume().unwrap(),
            LiveProgramStatus::Awaiting(_)
        ));
        let frame = program.session().frame();
        assert!(frame.is_present(0));
        assert!(!frame.is_present(1) && !frame.is_present(2) && !frame.is_present(3));
        assert_eq!(
            program.drive_to(&mut callbacks, 3.25).unwrap(),
            LiveProgramStatus::ReadyToResume,
            "a clean wait completes without another renderer publication"
        );
        assert_eq!(program.resume().unwrap(), LiveProgramStatus::Finished);
    }
}
