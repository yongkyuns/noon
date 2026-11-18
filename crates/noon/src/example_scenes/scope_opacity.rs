//! Final composed-family opacity, paired with scope_opacity.py.
use crate::{
    effects::{Blur, DissolveDirection, DissolveStyle, EffectUpdate, Glow, GlowSource, Pixels},
    AnimationCompositionRequest, AnimationOptions, ContinuationStep, IntoAnimationRequest,
    LiveContinuation, LiveProgram, MobjectFamily, RateFunction, Scene,
};

pub struct FamilyOpacity {
    group: MobjectFamily,
    parent: MobjectFamily,
    stage: u8,
}
fn message(error: impl std::fmt::Display) -> String {
    error.to_string()
}
impl LiveContinuation for FamilyOpacity {
    type Error = String;
    fn resume(&mut self, scene: &mut crate::ContinuationScene) -> Result<ContinuationStep, String> {
        if matches!(self.stage, 10 | 12 | 14 | 18 | 20) {
            let options = AnimationOptions::new()
                .run_time(1.0)
                .rate_func(RateFunction::Linear);
            let segment = match self.stage {
                10 | 14 => crate::animation_request::activate(
                    &mut scene.live(),
                    self.group
                        .animate()
                        .set_object_opacity(if self.stage == 10 { 0.25 } else { 0.0 })
                        .into(),
                    if self.stage == 14 {
                        options.rate_func(RateFunction::ThereAndBack)
                    } else {
                        options
                    },
                ),
                12 => crate::animation_request::activate(
                    &mut scene.live(),
                    (
                        self.group
                            .animate()
                            .set_object_opacity(0.75)
                            .with_options(options),
                        self.parent
                            .animate()
                            .set_object_opacity(0.5)
                            .with_options(options),
                    )
                        .into_animation_request(),
                    AnimationOptions::new(),
                ),
                18 | 20 => scene.live().declare_and_activate_composition(
                    &AnimationCompositionRequest::Dissolve {
                        target: (&self.group).into(),
                        direction: if self.stage == 18 {
                            DissolveDirection::Out
                        } else {
                            DissolveDirection::In
                        },
                        style: DissolveStyle::new(EffectUpdate::default().seed(7).softness(0.2))
                            .map_err(message)?,
                        options: if self.stage == 18 {
                            options.remover(false)
                        } else {
                            options
                        },
                    },
                    AnimationOptions::new(),
                ),
                _ => unreachable!(),
            }
            .map_err(message)?;
            self.stage += 1;
            return Ok(ContinuationStep::Await(segment));
        }
        match self.stage {
            0 => {}
            1 => {
                scene
                    .set_family_object_opacity(&self.group, 0.25)
                    .map_err(message)?;
            }
            2 => {
                scene
                    .set_family_object_opacity(&self.group, 0.0)
                    .map_err(message)?;
            }
            3 => {
                scene
                    .set_family_object_opacity(&self.group, 1.0)
                    .map_err(message)?;
            }
            4 => {
                scene
                    .add_effect(
                        &self.group,
                        Blur::new(EffectUpdate::default().radius(Pixels(0.0))).map_err(message)?,
                        "soften",
                    )
                    .map_err(message)?;
            }
            5 => {
                scene
                    .set_family_object_opacity(&self.group, 0.5)
                    .map_err(message)?;
            }
            6 => {
                scene
                    .set_effect(
                        &self.group,
                        "soften",
                        EffectUpdate::default().radius(Pixels(1.0)),
                    )
                    .map_err(message)?;
            }
            7 => {
                scene
                    .remove_effect(&self.group, "soften")
                    .map_err(message)?;
            }
            8 => {
                scene
                    .set_family_object_opacity(&self.parent, 1.0)
                    .map_err(message)?;
            }
            9 => {
                scene
                    .set_family_object_opacity(&self.group, 1.0)
                    .map_err(message)?;
            }
            11 | 13 | 15 | 19 | 21 => {}
            16 => {
                scene
                    .set_family_object_opacity(&self.group, 1.0)
                    .map_err(message)?;
                scene
                    .set_family_object_opacity(&self.parent, 1.0)
                    .map_err(message)?;
            }
            17 => {
                scene
                    .set_family_object_opacity(&self.group, 0.5)
                    .map_err(message)?;
                scene
                    .set_family_object_opacity(&self.parent, 0.6)
                    .map_err(message)?;
                scene
                    .add_effect(
                        &self.parent,
                        Glow::new(
                            EffectUpdate::default()
                                .radius(Pixels(2.0))
                                .source(GlowSource::Silhouette)
                                .intensity(1.0)
                                .color(crate::Color::rgba(1.0, 0.0, 0.0, 1.0)),
                        )
                        .map_err(message)?,
                        "halo",
                    )
                    .map_err(message)?;
            }
            22 => {
                scene.remove_effect(&self.parent, "halo").map_err(message)?;
                scene
                    .set_family_object_opacity(&self.group, 1.0)
                    .map_err(message)?;
                scene
                    .set_family_object_opacity(&self.parent, 1.0)
                    .map_err(message)?;
            }
            23 => return Ok(ContinuationStep::Finished),
            _ => return Err("family opacity resumed after completion".into()),
        }
        self.stage += 1;
        Ok(ContinuationStep::Await(
            scene.live().wait_segment(0.5).map_err(message)?,
        ))
    }
}

/// Paired with the gallery's comparison of group and per-shape fades.
#[cfg(all(feature = "native-text", feature = "bundled-fonts"))]
pub fn comparison_program() -> Result<LiveProgram<crate::SceneScript>, crate::ScriptError> {
    use crate::{Color, Text, Vec2};

    let mut scene = Scene::new();
    for (label, x) in [("As a group", -2.25), ("Per shape", 2.25)] {
        let text = scene.text(Text::new(label).with_font_size(34.0))?;
        scene.shift(&text, Vec2::new(x, 1.27))?;
        scene.add(&text)?;
    }
    let mut spot = |x| {
        let circle = scene.circle(0.82)?;
        scene.set_fill(&circle, Color::BLUE, 1.0)?;
        scene.disable_stroke(&circle)?;
        scene.shift(&circle, Vec2::new(x, -0.2))?;
        Ok::<_, crate::ScriptError>(circle)
    };
    let left = spot(-2.7)?;
    let right = spot(-1.8)?;
    let first = spot(1.8)?;
    let second = spot(2.7)?;
    let group = scene.family(&[(&left).into(), (&right).into()])?;
    scene.add_many(&[(&group).into(), (&first).into(), (&second).into()])?;
    scene.into_script(move |scene| async move {
        scene.wait(0.6).await?;
        for (opacity, duration, hold) in [(0.42, 1.2, 1.2), (1.0, 1.0, 0.8)] {
            scene
                .play_with(
                    (
                        group.animate().set_object_opacity(opacity),
                        first.animate().set_object_opacity(opacity),
                        second.animate().set_object_opacity(opacity),
                    ),
                    AnimationOptions::new()
                        .run_time(duration)
                        .rate_func(RateFunction::Linear),
                )
                .await?;
            scene.wait(hold).await?;
        }
        Ok(())
    })
}

pub fn program() -> Result<LiveProgram<FamilyOpacity>, String> {
    let mut scene = Scene::new();
    scene
        .camera_frame()
        .map_err(message)?
        .set_scale(0.75, 0.75)
        .map_err(message)?;
    // An unrelated witness remains visible during the exact empty group state.
    let mut witness = scene.square(0.6).map_err(message)?;
    witness.disable_stroke().map_err(message)?;
    witness.set_fill(1.0, 1.0, 1.0, 1.0).map_err(message)?;
    witness.set_translation(-4.0, -2.0).map_err(message)?;
    scene.add(&witness).map_err(message)?;
    let mut red = scene.rectangle(2.0, 2.0).map_err(message)?;
    red.disable_stroke().map_err(message)?;
    red.set_fill(1.0, 0.0, 0.0, 1.0).map_err(message)?;
    red.set_translation(-0.5, 0.0).map_err(message)?;
    let mut blue = scene.rectangle(2.0, 2.0).map_err(message)?;
    blue.disable_stroke().map_err(message)?;
    blue.set_fill(0.0, 0.0, 1.0, 0.5).map_err(message)?;
    blue.set_translation(0.5, 0.0).map_err(message)?;
    let group = scene
        .family(&[(&red).into(), (&blue).into()])
        .map_err(message)?;
    group.set_object_opacity(0.5).map_err(message)?;
    let parent = scene.family(&[(&group).into()]).map_err(message)?;
    parent.set_object_opacity(0.6).map_err(message)?;
    scene.add_many(&[(&parent).into()]).map_err(message)?;
    scene
        .into_live_program(FamilyOpacity {
            group,
            parent,
            stage: 0,
        })
        .map_err(message)
}
