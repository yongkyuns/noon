//! Plain-Text family fades composed with a disjoint glyph Write.

use crate::{
    AnimationCompositionRequest, AnimationOptions, ContinuationStep, LiveContinuation, LiveProgram,
    LiveSession, Mobject, MobjectFamily, RateFunction, Scene, SemanticAnimationCompositionKind,
    SemanticFadeDirection,
};

pub struct TextFamilyFade {
    left: Mobject,
    right: Mobject,
    writing: Mobject,
    family: MobjectFamily,
    stage: u8,
}

impl LiveContinuation for TextFamilyFade {
    type Error = String;

    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, String> {
        match self.stage {
            0 => {
                let family_options = AnimationOptions::new()
                    .run_time(2.0)
                    .rate_func(RateFunction::Linear)
                    .lag_ratio(0.25);
                let write_options = AnimationOptions::new()
                    .run_time(2.0)
                    .rate_func(RateFunction::Linear);
                let segment = live
                    .declare_and_activate_animation_composition(
                        SemanticAnimationCompositionKind::Parallel,
                        &[
                            AnimationCompositionRequest::FamilyFade {
                                target: &self.family,
                                direction: SemanticFadeDirection::In,
                                options: family_options,
                            },
                            AnimationCompositionRequest::TextWrite {
                                target: &self.writing,
                                reverse_member_order: false,
                                options: write_options,
                            },
                        ],
                        AnimationOptions::new().rate_func(RateFunction::Linear),
                        AnimationOptions::new(),
                    )
                    .map_err(|error| error.to_string())?;
                self.stage = 1;
                Ok(ContinuationStep::Await(segment))
            }
            1 => {
                for member in [&self.left, &self.right, &self.writing] {
                    if !live.contains(member).map_err(|error| error.to_string())? {
                        return Err("Text family FadeIn did not admit every expected object".into());
                    }
                }
                {
                    let store = self.family.store().borrow();
                    let family = store
                        .node(self.family.node_id())
                        .ok_or("Text family identity disappeared after FadeIn")?;
                    if family.parents().len() != 1
                        || [&self.left, &self.right].iter().any(|member| {
                            !store
                                .node(member.node_id())
                                .is_some_and(|node| node.parents().contains(&self.family.node_id()))
                        })
                    {
                        return Err(
                            "Text family FadeIn changed authoritative family identity".into()
                        );
                    }
                }
                self.stage = 2;
                live.declare_and_activate_family_fade(
                    &self.family,
                    SemanticFadeDirection::Out,
                    AnimationOptions::new()
                        .run_time(1.0)
                        .rate_func(RateFunction::Linear)
                        .lag_ratio(0.25),
                )
                .map(ContinuationStep::Await)
                .map_err(|error| error.to_string())
            }
            2 => {
                if live
                    .contains(&self.left)
                    .map_err(|error| error.to_string())?
                    || live
                        .contains(&self.right)
                        .map_err(|error| error.to_string())?
                    || !live
                        .contains(&self.writing)
                        .map_err(|error| error.to_string())?
                {
                    return Err(
                        "Text family FadeOut did not remove only its authoritative family".into(),
                    );
                }
                {
                    let store = self.family.store().borrow();
                    let family = store
                        .node(self.family.node_id())
                        .ok_or("Text family identity disappeared after FadeOut")?;
                    if !family.parents().is_empty()
                        || !store
                            .node(self.left.node_id())
                            .is_some_and(|node| node.parents().contains(&self.family.node_id()))
                    {
                        return Err(
                            "Text family FadeOut destroyed authoritative family identity".into(),
                        );
                    }
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
            _ => Err("Text family Fade continuation resumed after completion".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LiveProgramStatus, RustHostCallbackTable};

    fn admit_completion(
        program: &mut LiveProgram<TextFamilyFade>,
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

    #[test]
    fn native_text_family_fade_keeps_one_family_identity_and_cleans_up_after_fade_out() {
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
        let frame = program.session().frame();
        assert!(frame.is_present(0) && frame.is_present(1) && frame.is_present(2));
        assert!(frame.objects[0].appearance > frame.objects[1].appearance);
        assert!(program
            .session()
            .active_family_animation_indices()
            .contains(&2));

        admit_completion(&mut program, &mut callbacks, 2.0);
        assert!(matches!(
            program.resume().unwrap(),
            LiveProgramStatus::Awaiting(_)
        ));
        assert!(matches!(
            program.drive_to(&mut callbacks, 2.5).unwrap(),
            LiveProgramStatus::Awaiting(_)
        ));
        let frame = program.session().frame();
        assert!(frame.objects[0].appearance < frame.objects[1].appearance);
        assert_eq!(frame.objects[2].appearance, 1.0);

        admit_completion(&mut program, &mut callbacks, 3.0);
        assert!(matches!(
            program.resume().unwrap(),
            LiveProgramStatus::Awaiting(_)
        ));
        let frame = program.session().frame();
        assert!(!frame.is_present(0) && !frame.is_present(1));
        assert!(frame.is_present(2));
        admit_completion(&mut program, &mut callbacks, 3.25);
        assert_eq!(program.resume().unwrap(), LiveProgramStatus::Finished);
    }
}

pub fn program() -> Result<LiveProgram<TextFamilyFade>, String> {
    let scene = Scene::new();
    let mut left = scene.text("LEFT").map_err(|error| error.to_string())?;
    left.set_translation(-2.0, 0.5)?;
    let mut right = scene.text("RIGHT").map_err(|error| error.to_string())?;
    right.set_translation(1.0, 0.5)?;
    let mut writing = scene.text("WRITE").map_err(|error| error.to_string())?;
    writing.set_translation(-1.0, -1.0)?;
    let family = scene.family(&[&left, &right])?;
    scene
        .into_live_program(TextFamilyFade {
            left,
            right,
            writing,
            family,
            stage: 0,
        })
        .map_err(|error| error.to_string())
}
