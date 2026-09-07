//! Global glyph Write/Unwrite timing across one plain-Text family.

use crate::{
    AnimationCompositionRequest, AnimationOptions, ContinuationStep, LiveContinuation, LiveProgram,
    LiveSession, Mobject, MobjectFamily, RateFunction, Scene, SemanticAnimationCompositionKind,
    TransformToRequest,
};

pub struct TextFamilyWrite {
    left: Mobject,
    right: Mobject,
    family: MobjectFamily,
    moving: Mobject,
    moving_target: Mobject,
    left_target: Mobject,
    stage: u8,
}

impl LiveContinuation for TextFamilyWrite {
    type Error = String;

    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, String> {
        match self.stage {
            0 => {
                let rejected = live.declare_and_activate_animation_composition(
                    SemanticAnimationCompositionKind::Parallel,
                    &[
                        AnimationCompositionRequest::FamilyTextWrite {
                            target: &self.family,
                            reverse_member_order: false,
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
                    || self
                        .family
                        .store()
                        .borrow()
                        .node(self.family.node_id())
                        .unwrap()
                        .parents()
                        .len()
                        != 0
                {
                    return Err("overlapping family Write admitted a partial family root".into());
                }

                let family_options = AnimationOptions::new()
                    .run_time(2.0)
                    .rate_func(RateFunction::Linear)
                    .lag_ratio(0.25);
                let transform_options = AnimationOptions::new()
                    .run_time(2.0)
                    .rate_func(RateFunction::Linear);
                let segment = live
                    .declare_and_activate_animation_composition(
                        SemanticAnimationCompositionKind::Parallel,
                        &[
                            AnimationCompositionRequest::FamilyTextWrite {
                                target: &self.family,
                                reverse_member_order: false,
                                options: family_options,
                            },
                            AnimationCompositionRequest::TransformTo(TransformToRequest::new(
                                &self.moving,
                                &self.moving_target,
                                transform_options,
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
                    || (live
                        .effective(&self.moving)
                        .map_err(|error| error.to_string())?
                        .transform
                        .translation
                        .x
                        - 2.0)
                        .abs()
                        > 1.0e-6
                {
                    return Err("family Write did not complete its disjoint transform".into());
                }
                let store = self.family.store().borrow();
                let family = store
                    .node(self.family.node_id())
                    .ok_or("Text family identity disappeared after Write")?;
                if family.parents().len() != 1
                    || [&self.left, &self.right].iter().any(|member| {
                        !store
                            .node(member.node_id())
                            .is_some_and(|node| node.parents().contains(&self.family.node_id()))
                    })
                {
                    return Err("family Write changed authoritative family identity".into());
                }
                drop(store);
                self.stage = 2;
                live.declare_and_activate_family_text_write(
                    &self.family,
                    true,
                    AnimationOptions::new()
                        .run_time(1.0)
                        .rate_func(RateFunction::Linear)
                        .lag_ratio(0.25)
                        .introducer(false)
                        .remover(true)
                        .reverse_rate_function(true),
                )
                .map(ContinuationStep::Await)
                .map_err(|error| error.to_string())
            }
            2 => {
                let store = self.family.store().borrow();
                let family = store
                    .node(self.family.node_id())
                    .ok_or("Text family identity disappeared after Unwrite")?;
                if !family.parents().is_empty()
                    || !store
                        .node(self.left.node_id())
                        .is_some_and(|node| node.parents().contains(&self.family.node_id()))
                    || !live
                        .contains(&self.moving)
                        .map_err(|error| error.to_string())?
                {
                    return Err("family Unwrite did not remove only its family root".into());
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
            _ => Err("Text family Write continuation resumed after completion".into()),
        }
    }
}

pub fn program() -> Result<LiveProgram<TextFamilyWrite>, String> {
    let mut scene = Scene::new();
    let mut left = scene.text("I").map_err(|error| error.to_string())?;
    left.set_translation(-3.0, 0.75)?;
    let mut right = scene.text("LONG").map_err(|error| error.to_string())?;
    right.set_translation(0.0, 0.75)?;
    let family = scene.family(&[&left, &right])?;
    let mut moving = scene.square(0.6)?;
    moving.set_translation(0.0, -1.25)?;
    scene.add(&moving)?;
    let mut moving_target = moving.target_editor()?;
    moving_target.shift(2.0, 0.0)?;
    let mut left_target = left.target_editor()?;
    left_target.shift(0.0, 1.0)?;
    scene
        .into_live_program(TextFamilyWrite {
            left,
            right,
            family,
            moving,
            moving_target,
            left_target,
            stage: 0,
        })
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LiveProgramStatus, RustHostCallbackTable};

    fn admit_completion(
        program: &mut LiveProgram<TextFamilyWrite>,
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

    fn progress(program: &LiveProgram<TextFamilyWrite>, object: usize, count: u32) -> Vec<f32> {
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
    fn native_family_write_uses_one_global_five_glyph_order_and_cleans_up() {
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

        admit_completion(&mut program, &mut callbacks, 2.0);
        assert!(matches!(
            program.resume().unwrap(),
            LiveProgramStatus::Awaiting(_)
        ));
        assert!(matches!(
            program.drive_to(&mut callbacks, 2.5).unwrap(),
            LiveProgramStatus::Awaiting(_)
        ));
        assert_eq!(progress(&program, 1, 1), vec![1.0]);
        assert_eq!(progress(&program, 2, 4), vec![0.75, 0.5, 0.25, 0.0]);

        admit_completion(&mut program, &mut callbacks, 3.0);
        assert!(matches!(
            program.resume().unwrap(),
            LiveProgramStatus::Awaiting(_)
        ));
        let frame = program.session().frame();
        assert!(frame.is_present(0));
        assert!(!frame.is_present(1) && !frame.is_present(2));
        assert_eq!(
            program.drive_to(&mut callbacks, 3.25).unwrap(),
            LiveProgramStatus::ReadyToResume
        );
        assert_eq!(program.resume().unwrap(), LiveProgramStatus::Finished);
    }
}
