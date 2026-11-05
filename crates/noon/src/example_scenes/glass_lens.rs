//! A moving glass card treats the earlier texture while retaining its own paint.
//! Paired with `web/python/examples/showcase_glass_lens.py` on the ordinary live timeline.
use crate::{
    effects::{BackdropBlur, DissolveDirection, DissolveStyle, EffectUpdate, Pixels},
    AnimationCompositionRequest, AnimationOptions, ContinuationStep, ImageMobjectOptions,
    LiveContinuation, LiveProgram, LiveSession, MobjectFamily, RasterImageSampling, RateFunction,
    Scene, Text,
};

pub struct GlassLens {
    card: MobjectFamily,
    stage: u8,
}

fn message(error: impl std::fmt::Display) -> String {
    error.to_string()
}

impl LiveContinuation for GlassLens {
    type Error = String;

    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, String> {
        let segment = match self.stage {
            0 | 2 | 4 | 8 | 10 => live.wait_segment(0.5),
            1 | 3 => {
                let target = live.copy_family(&self.card).map_err(message)?;
                let (dx, radius) = if self.stage == 1 {
                    (4.0, 18.0)
                } else {
                    (-4.0, 0.0)
                };
                live.shift_family(target.root(), dx, 0.0).map_err(message)?;
                live.set_effect(
                    target.root(),
                    "glass",
                    EffectUpdate::default().radius(Pixels(radius)),
                )
                .map_err(message)?;
                live.declare_and_activate_composition(
                    &AnimationCompositionRequest::FamilyMethodTargetTo {
                        source: &self.card,
                        target_state: target.root(),
                        options: AnimationOptions::new()
                            .run_time(1.5)
                            .rate_func(RateFunction::Smooth),
                    },
                    AnimationOptions::new(),
                )
            }
            5 => {
                live.remove_effect(&self.card, "glass").map_err(message)?;
                live.wait_segment(0.5)
            }
            6 => {
                live.add_effect(
                    &self.card,
                    BackdropBlur::new(EffectUpdate::default().radius(Pixels(12.0)))
                        .map_err(message)?,
                    "glass",
                )
                .map_err(message)?;
                live.wait_segment(0.5)
            }
            7 | 9 => live.declare_and_activate_composition(
                &AnimationCompositionRequest::Dissolve {
                    target: (&self.card).into(),
                    direction: if self.stage == 7 {
                        DissolveDirection::Out
                    } else {
                        DissolveDirection::In
                    },
                    style: DissolveStyle::new(EffectUpdate::default().seed(7).softness(0.2))
                        .map_err(message)?,
                    options: AnimationOptions::new()
                        .run_time(1.0)
                        .rate_func(RateFunction::Linear)
                        .remover(false),
                },
                AnimationOptions::new(),
            ),
            11 => return Ok(ContinuationStep::Finished),
            _ => return Err("glass lens resumed after completion".into()),
        }
        .map_err(message)?;
        self.stage += 1;
        Ok(ContinuationStep::Await(segment))
    }
}

/// A prepared texture, not a per-frame callback or procedural animation clock.
pub fn checker_rgba8() -> Vec<u8> {
    let mut pixels = Vec::with_capacity(48 * 24 * 4);
    for y in 0..24 {
        for x in 0..48 {
            let color = if (x / 3 + y / 3) % 2 == 0 {
                [28, 82, 148, 255]
            } else {
                [240, 166, 70, 255]
            };
            pixels.extend_from_slice(&color);
        }
    }
    pixels
}

fn setup() -> Result<(Scene, GlassLens), String> {
    let mut scene = Scene::new();
    scene
        .camera_frame()
        .map_err(message)?
        .set_scale(1.125, 1.125)
        .map_err(message)?;
    let mut background = scene.rectangle(16.0, 9.0).map_err(message)?;
    background
        .set_fill(8.0 / 255.0, 16.0 / 255.0, 31.0 / 255.0, 1.0)
        .map_err(message)?;
    background.disable_stroke().map_err(message)?;
    scene.add(&background).map_err(message)?;
    let mut texture_options =
        ImageMobjectOptions::rgba8(48, 24, checker_rgba8()).map_err(message)?;
    texture_options.set_height(6.0).map_err(message)?;
    texture_options.set_sampling(RasterImageSampling::Nearest);
    let texture = scene.image(texture_options).map_err(message)?;
    scene.add(&texture).map_err(message)?;

    let mut panel = scene.rectangle(3.0, 2.0).map_err(message)?;
    panel.set_fill(1.0, 1.0, 1.0, 0.12).map_err(message)?;
    panel.disable_stroke().map_err(message)?;
    panel.set_translation(-2.0, 0.0).map_err(message)?;
    let mut emblem_options = super::image_glow::image_options()?;
    emblem_options.set_height(0.6).map_err(message)?;
    let mut emblem = scene.image(emblem_options).map_err(message)?;
    emblem.set_translation(-2.95, 0.55).map_err(message)?;
    let mut label = scene
        .text(Text::new("GLASS").with_font_size(24.0))
        .map_err(message)?;
    label.set_translation(-2.0, -0.35).map_err(message)?;
    let mut card = scene
        .family(&[(&panel).into(), (&emblem).into(), (&label).into()])
        .map_err(message)?;
    card.set_object_opacity(0.85).map_err(message)?;
    card.add_effect(
        BackdropBlur::new(EffectUpdate::default().radius(Pixels(6.0))).map_err(message)?,
        "glass",
    )
    .map_err(message)?;
    scene.add_many(&[(&card).into()]).map_err(message)?;
    Ok((scene, GlassLens { card, stage: 0 }))
}

pub fn program() -> Result<LiveProgram<GlassLens>, String> {
    let (scene, continuation) = setup()?;
    scene.into_live_program(continuation).map_err(message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LiveProgramStatus, RustHostCallbackTable};

    fn drive(
        program: &mut LiveProgram<GlassLens>,
        callbacks: &mut RustHostCallbackTable,
        time: f64,
    ) -> LiveProgramStatus {
        let status = program.drive_to(callbacks, time).unwrap();
        match status {
            LiveProgramStatus::PublicationPending(expected) => {
                assert_eq!(program.take_renderer_publication().context(), expected);
                program.admit_publication(expected).unwrap();
                program.resume().unwrap()
            }
            LiveProgramStatus::ReadyToResume => program.resume().unwrap(),
            _ => status,
        }
    }

    fn radius(program: &LiveProgram<GlassLens>) -> Option<f64> {
        let radii = program
            .session()
            .frame()
            .objects
            .iter()
            .filter_map(|row| row.effects.as_ref())
            .flat_map(|stack| stack.iter())
            .filter_map(|effect| effect.definition.as_backdrop_blur())
            .map(|effect| effect.radius().value())
            .collect::<Vec<_>>();
        assert!(radii.len() <= 1, "only the glass scope owns a backdrop");
        radii.first().copied()
    }

    #[test]
    fn paired_glass_motion_neutral_removal_reattachment_and_visibility_execute() {
        let mut program = program().unwrap();
        let mut callbacks = RustHostCallbackTable::new();
        program.resume().unwrap();
        for (time, expected) in [
            (0.25, Some(6.0)),
            (0.5, Some(6.0)),
            (1.25, Some(12.0)),
            (2.0, Some(18.0)),
            (2.5, Some(18.0)),
            (3.25, Some(9.0)),
            (4.0, Some(0.0)),
            (4.5, None),
            (5.0, Some(12.0)),
            (5.5, Some(12.0)),
            (6.0, Some(12.0)),
            (6.5, Some(12.0)),
            (7.0, Some(12.0)),
            (7.5, Some(12.0)),
            (8.0, Some(12.0)),
        ] {
            let status = drive(&mut program, &mut callbacks, time);
            assert_eq!(radius(&program), expected, "at authored time {time}");
            assert_ne!(status, LiveProgramStatus::Finished);
        }
        assert_eq!(
            drive(&mut program, &mut callbacks, 8.5),
            LiveProgramStatus::Finished
        );
    }

    #[cfg(feature = "prepared-prefix")]
    #[test]
    fn prepared_prefix_adopts_glass_pixels_backdrop_and_dissolve_history() {
        use noon_runtime::ReplayLimits;

        let (scene, mut continuation) = setup().unwrap();
        let mut source = scene.execution_session().unwrap();
        source
            .begin_replay_retention(ReplayLimits::default())
            .unwrap();
        let first = match continuation.resume(&mut scene.live(&mut source)).unwrap() {
            ContinuationStep::Await(segment) => segment,
            other => panic!("expected initial wait, got {other:?}"),
        };
        source.advance_to(first.end_time()).unwrap();
        scene.live(&mut source).complete_segment(first).unwrap();
        let (mut publisher, mut reader) = source.create_prepared_prefix(first).unwrap();
        let paused = reader.frame().clone();
        let image_handles = reader
            .frame()
            .objects
            .iter()
            .filter_map(|row| row.content.image())
            .map(|image| image.resource())
            .collect::<Vec<_>>();
        let resources = reader.resource_publication();
        assert_eq!(image_handles.len(), 2);
        assert!(image_handles
            .iter()
            .all(|handle| resources.raster_image_resources().get(*handle).is_some()));
        assert!(reader.retained_resource_bytes() > 0);

        let mut last = first;
        loop {
            match continuation.resume(&mut scene.live(&mut source)).unwrap() {
                ContinuationStep::Await(segment) => {
                    source.advance_to(segment.end_time()).unwrap();
                    scene.live(&mut source).complete_segment(segment).unwrap();
                    let suffix = source
                        .publish_prepared_prefix(segment, &mut publisher)
                        .unwrap_or_else(|error| {
                            panic!("glass suffix at {} failed: {error}", segment.end_time())
                        });
                    reader.preflight_append(&suffix).unwrap();
                    reader.append(suffix).unwrap();
                    assert_eq!(reader.frame(), &paused, "adoption stays paused");
                    last = segment;
                }
                ContinuationStep::Finished => break,
                ContinuationStep::Publish(_) => panic!("local glass resources need no host work"),
            }
        }
        source.seal_replay().unwrap();
        reader
            .append(source.finish_prepared_prefix(last, &mut publisher).unwrap())
            .unwrap();
        for time in [
            0.0, 0.25, 0.5, 1.25, 2.0, 2.25, 2.5, 3.25, 4.0, 4.5, 5.0, 5.5, 6.0, 6.5, 7.0, 7.5,
            8.0, 8.5,
        ] {
            source.seek(time).unwrap();
            reader.seek(time).unwrap();
            assert_eq!(reader.frame(), source.frame(), "at authored time {time}");
        }
    }
}
