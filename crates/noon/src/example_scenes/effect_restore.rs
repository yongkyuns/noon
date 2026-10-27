//! Ordered leaf and composed-family save/restore, paired with effect_restore.py.
use crate::{
    effects::{Blur, EffectUpdate, Glow, Pixels},
    Color, ContinuationStep, LiveContinuation, LiveProgram, LiveSession, Mobject, MobjectFamily,
    Scene,
};

pub struct EffectRestore {
    leaf: Mobject,
    group: MobjectFamily,
    saved_leaf: Option<Mobject>,
    saved_group: Option<MobjectFamily>,
    stage: u8,
}
fn message(error: impl std::fmt::Display) -> String {
    error.to_string()
}
impl EffectRestore {
    fn saved(&self) -> Result<(&Mobject, &MobjectFamily), String> {
        Ok((
            self.saved_leaf.as_ref().ok_or("missing saved leaf")?,
            self.saved_group.as_ref().ok_or("missing saved group")?,
        ))
    }
}
impl LiveContinuation for EffectRestore {
    type Error = String;
    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, String> {
        match self.stage {
            0 => {}
            1 => {
                self.saved_leaf = Some(
                    live.copy_mobject(&self.leaf)
                        .map_err(message)?
                        .root()
                        .clone(),
                );
                self.saved_group = Some(
                    live.copy_family(&self.group)
                        .map_err(message)?
                        .root()
                        .clone(),
                );
                live.set_effect(
                    &self.leaf,
                    "soften",
                    EffectUpdate::default().radius(Pixels(3.0)),
                )
                .map_err(message)?;
                live.set_effect(&self.leaf, "halo", EffectUpdate::default().intensity(1.0))
                    .map_err(message)?;
                live.set_effect(
                    &self.group,
                    "soften",
                    EffectUpdate::default().radius(Pixels(4.0)),
                )
                .map_err(message)?;
                live.set_family_object_opacity(&self.group, 0.25)
                    .map_err(message)?;
            }
            2 => {
                live.remove_effect(&self.leaf, "soften").map_err(message)?;
                live.remove_effect(&self.leaf, "halo").map_err(message)?;
                live.remove_effect(&self.group, "soften").map_err(message)?;
            }
            3 | 5 => {
                let (leaf, group) = self.saved()?;
                live.become_mobject(&self.leaf, leaf, Default::default())
                    .map_err(message)?;
                live.become_family(&self.group, group, Default::default())
                    .map_err(message)?;
            }
            4 => {
                let (leaf, group) = self.saved()?;
                live.set_effect(leaf, "halo", EffectUpdate::default().intensity(0.0))
                    .map_err(message)?;
                live.set_family_object_opacity(group, 0.0)
                    .map_err(message)?;
            }
            6 => {
                live.remove_effect(&self.leaf, "soften").map_err(message)?;
                live.remove_effect(&self.leaf, "halo").map_err(message)?;
                live.remove_effect(&self.group, "soften").map_err(message)?;
                live.set_family_object_opacity(&self.group, 1.0)
                    .map_err(message)?;
            }
            7 => return Ok(ContinuationStep::Finished),
            _ => return Err("effect restoration resumed after completion".into()),
        }
        self.stage += 1;
        Ok(ContinuationStep::Await(
            live.wait_segment(0.5).map_err(message)?,
        ))
    }
}

pub fn program() -> Result<LiveProgram<EffectRestore>, String> {
    let mut scene = Scene::new();
    let mut leaf = scene.square(2.0).map_err(message)?;
    leaf.disable_stroke().map_err(message)?;
    leaf.set_fill(1.0, 1.0, 1.0, 1.0).map_err(message)?;
    leaf.set_translation(-2.0, 0.0).map_err(message)?;
    leaf.add_effect(
        Blur::new(EffectUpdate::default().radius(Pixels(1.0))).map_err(message)?,
        "soften",
    )
    .map_err(message)?;
    leaf.add_effect(
        Glow::new(
            EffectUpdate::default()
                .radius(Pixels(3.25))
                .intensity(0.4)
                .color(Color::rgba(1.0, 0.0, 0.0, 1.0)),
        )
        .map_err(message)?,
        "halo",
    )
    .map_err(message)?;
    let mut a = scene.square(2.0).map_err(message)?;
    a.disable_stroke().map_err(message)?;
    a.set_fill(1.0, 1.0, 1.0, 0.6).map_err(message)?;
    a.set_translation(1.5, 0.0).map_err(message)?;
    let mut b = scene.square(2.0).map_err(message)?;
    b.disable_stroke().map_err(message)?;
    b.set_fill(1.0, 1.0, 1.0, 0.6).map_err(message)?;
    b.set_translation(2.5, 0.0).map_err(message)?;
    let mut group = scene.family(&[(&a).into(), (&b).into()]).map_err(message)?;
    group.set_object_opacity(0.6).map_err(message)?;
    group
        .add_effect(
            Blur::new(EffectUpdate::default().radius(Pixels(2.0))).map_err(message)?,
            "soften",
        )
        .map_err(message)?;
    let mut witness = scene.square(0.4).map_err(message)?;
    witness.disable_stroke().map_err(message)?;
    witness.set_fill(1.0, 1.0, 1.0, 1.0).map_err(message)?;
    witness.set_translation(-4.0, -2.0).map_err(message)?;
    scene
        .add_many(&[(&leaf).into(), (&group).into(), (&witness).into()])
        .map_err(message)?;
    scene
        .into_live_program(EffectRestore {
            leaf,
            group,
            saved_leaf: None,
            saved_group: None,
            stage: 0,
        })
        .map_err(message)
}
