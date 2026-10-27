//! Final composed-family opacity, paired with scope_opacity.py.
use crate::{
    effects::{Blur, EffectUpdate, Pixels},
    ContinuationStep, LiveContinuation, LiveProgram, LiveSession, MobjectFamily, Scene,
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
    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, String> {
        match self.stage {
            0 => {}
            1 => {
                live.set_family_object_opacity(&self.group, 0.25)
                    .map_err(message)?;
            }
            2 => {
                live.set_family_object_opacity(&self.group, 0.0)
                    .map_err(message)?;
            }
            3 => {
                live.set_family_object_opacity(&self.group, 1.0)
                    .map_err(message)?;
            }
            4 => {
                live.add_effect(
                    &self.group,
                    Blur::new(EffectUpdate::default().radius(Pixels(0.0))).map_err(message)?,
                    "soften",
                )
                .map_err(message)?;
            }
            5 => {
                live.set_family_object_opacity(&self.group, 0.5)
                    .map_err(message)?;
            }
            6 => {
                live.set_effect(
                    &self.group,
                    "soften",
                    EffectUpdate::default().radius(Pixels(1.0)),
                )
                .map_err(message)?;
            }
            7 => {
                live.remove_effect(&self.group, "soften").map_err(message)?;
            }
            8 => {
                live.set_family_object_opacity(&self.parent, 1.0)
                    .map_err(message)?;
            }
            9 => {
                live.set_family_object_opacity(&self.group, 1.0)
                    .map_err(message)?;
            }
            10 => return Ok(ContinuationStep::Finished),
            _ => return Err("family opacity resumed after completion".into()),
        }
        self.stage += 1;
        Ok(ContinuationStep::Await(
            live.wait_segment(0.5).map_err(message)?,
        ))
    }
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
