//! Nested mixed-source controls, paired with web/python/examples/composed_effects.py.
use crate::{
    effects::{Blur, EffectUpdate, Pixels},
    Color, ContinuationStep, ImageMobjectOptions, LiveContinuation, LiveProgram, LiveSession,
    MobjectFamily, RasterImageSampling, Scene,
};

pub struct ComposedEffects {
    reference: MobjectFamily,
    inner: MobjectFamily,
    outer: MobjectFamily,
    stage: u8,
}

fn message(error: impl std::fmt::Display) -> String {
    error.to_string()
}

impl LiveContinuation for ComposedEffects {
    type Error = String;
    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, String> {
        match self.stage {
            0 => {}
            1 => {
                for family in [&self.reference, &self.inner] {
                    live.set_effect(
                        family,
                        "soften",
                        EffectUpdate::default().radius(Pixels(4.0)),
                    )
                    .map_err(message)?;
                }
                live.set_glow(&self.outer, EffectUpdate::default().intensity(0.6))
                    .map_err(message)?;
            }
            2 => {
                for family in [&self.reference, &self.inner] {
                    live.remove_effect(family, "soften").map_err(message)?;
                }
            }
            3 => live.remove_glow(&self.outer).map_err(message)?,
            4 => return Ok(ContinuationStep::Finished),
            _ => return Err("composed example resumed after completion".into()),
        }
        self.stage += 1;
        live.wait_segment(0.5)
            .map(ContinuationStep::Await)
            .map_err(message)
    }
}

fn source(scene: &mut Scene, x: f64) -> Result<MobjectFamily, String> {
    let pixels: Vec<u8> = [0, 128, 255, 64]
        .into_iter()
        .flat_map(|alpha| [255, 255, 255, alpha])
        .collect();
    let mut options = ImageMobjectOptions::rgba8(2, 2, pixels).map_err(message)?;
    options.set_height(1.2).map_err(message)?;
    options.set_sampling(RasterImageSampling::Nearest);
    let mut image = scene.image(options).map_err(message)?;
    image.set_translation(x - 0.25, 0.0).map_err(message)?;
    let mut rectangle = scene.rectangle(1.0, 0.8).map_err(message)?;
    rectangle.set_fill(1.0, 1.0, 1.0, 0.5).map_err(message)?;
    rectangle.disable_stroke().map_err(message)?;
    rectangle.set_translation(x + 0.25, 0.0).map_err(message)?;
    let mut family = scene
        .family(&[(&image).into(), (&rectangle).into()])
        .map_err(message)?;
    family
        .add_effect(
            Blur::new(EffectUpdate::default().radius(Pixels(2.0))).map_err(message)?,
            "soften",
        )
        .map_err(message)?;
    Ok(family)
}

pub fn program() -> Result<LiveProgram<ComposedEffects>, String> {
    let mut scene = Scene::new();
    let mut background = scene.rectangle(16.0, 9.0).map_err(message)?;
    background.set_fill(0.0, 0.0, 0.0, 1.0).map_err(message)?;
    background.disable_stroke().map_err(message)?;
    scene.add(&background).map_err(message)?;
    let reference = source(&mut scene, -2.0)?;
    let inner = source(&mut scene, 2.0)?;
    let mut outer = scene.family(&[(&inner).into()]).map_err(message)?;
    outer
        .set_glow(
            EffectUpdate::default()
                .radius(Pixels(3.25))
                .intensity(1.25)
                .color(Color::rgba(1.0, 0.0, 0.0, 1.0)),
        )
        .map_err(message)?;
    scene
        .add_many(&[(&reference).into(), (&outer).into()])
        .map_err(message)?;
    scene
        .into_live_program(ComposedEffects {
            reference,
            inner,
            outer,
            stage: 0,
        })
        .map_err(message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LiveProgramStatus, RustHostCallbackTable};

    #[test]
    fn mixed_nested_program_updates_and_retires_only_scope_rows() {
        let mut program = program().unwrap();
        let mut callbacks = RustHostCallbackTable::new();
        program.resume().unwrap();
        let leaves = program
            .session()
            .frame()
            .objects
            .iter()
            .filter(|row| row.content.composed_source().is_none())
            .map(|row| row.id)
            .collect::<Vec<_>>();
        for (time, expected_scopes) in [(0.5, 3), (1.0, 1), (1.5, 0), (2.0, 0)] {
            let status = program.drive_to(&mut callbacks, time).unwrap();
            if let LiveProgramStatus::PublicationPending(expected) = status {
                let context = program.take_renderer_publication().context();
                assert_eq!(context, expected);
                program.admit_publication(context).unwrap();
            } else {
                assert_eq!(status, LiveProgramStatus::ReadyToResume);
            }
            let status = program.resume().unwrap();
            let frame = program.session().frame();
            assert_eq!(
                frame
                    .objects
                    .iter()
                    .enumerate()
                    .filter(|(i, row)| frame.is_present(*i) && row.effects.is_some())
                    .count(),
                expected_scopes
            );
            for leaf in &leaves {
                assert!(frame
                    .objects
                    .iter()
                    .enumerate()
                    .any(|(i, row)| row.id == *leaf && frame.is_present(i)));
            }
            if time == 2.0 {
                assert_eq!(status, LiveProgramStatus::Finished);
            }
        }
    }
}
