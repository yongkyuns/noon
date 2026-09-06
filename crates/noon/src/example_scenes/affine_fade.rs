//! Scaled and translated FadeIn/FadeOut through shared Rust semantics.

use std::rc::Rc;

use crate::{
    AnimationOptions, Color, ContinuationStep, FadeEndpoint, FadeTranslation, LiveContinuation,
    LiveProgram, LiveSession, Mobject, RateFunction, Scene, SemanticFadeDirection, SemanticVec3,
};

pub struct AffineFade {
    circle: Mobject,
    stage: u8,
}

impl LiveContinuation for AffineFade {
    type Error = String;

    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, String> {
        let options = AnimationOptions::new()
            .run_time(1.0)
            .rate_func(RateFunction::Linear);
        match self.stage {
            0 => {
                self.stage = 1;
                live.declare_and_activate_fade_with_endpoint(
                    &self.circle,
                    SemanticFadeDirection::In,
                    FadeEndpoint::new(
                        0.25,
                        FadeTranslation::Shift(SemanticVec3::new(2.0, 0.0, 0.0)),
                    ),
                    options,
                )
                .map(ContinuationStep::Await)
                .map_err(|error| error.to_string())
            }
            1 => {
                let state = live
                    .effective(&self.circle)
                    .map_err(|error| error.to_string())?;
                if state.transform != noon_core::Transform2D::IDENTITY {
                    return Err("FadeIn did not publish the canonical endpoint".into());
                }
                self.stage = 2;
                live.declare_and_activate_fade_with_endpoint(
                    &self.circle,
                    SemanticFadeDirection::Out,
                    FadeEndpoint::new(
                        0.15,
                        FadeTranslation::Point(SemanticVec3::new(2.0, 0.0, 0.0)),
                    ),
                    options,
                )
                .map(ContinuationStep::Await)
                .map_err(|error| error.to_string())
            }
            2 => {
                if live
                    .contains(&self.circle)
                    .map_err(|error| error.to_string())?
                {
                    return Err("FadeOut did not remove its semantic target".into());
                }
                self.stage = 3;
                Ok(ContinuationStep::Finished)
            }
            _ => Err("affine fade continuation resumed after completion".into()),
        }
    }
}

pub fn program() -> Result<LiveProgram<AffineFade>, String> {
    let scene = Scene::new();
    let mut circle = Mobject::manim_circle(Rc::clone(scene.store()), 0.4)?;
    circle.set_fill(
        f64::from(Color::BLUE.red),
        f64::from(Color::BLUE.green),
        f64::from(Color::BLUE.blue),
        1.0,
    )?;
    circle.set_stroke_opacity(0.0)?;
    scene
        .into_live_program(AffineFade { circle, stage: 0 })
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LiveProgramStatus, RustHostCallbackTable};

    #[test]
    fn affine_fade_resolves_shift_and_point_endpoints_in_rust() {
        let mut program = program().unwrap();
        let mut callbacks = RustHostCallbackTable::new();
        assert!(matches!(
            program.resume().unwrap(),
            LiveProgramStatus::Awaiting(_)
        ));

        for (time, x, scale, appearance) in [
            (0.0, -2.0, 0.25, 0.0),
            (0.5, -1.0, 0.625, 0.5),
            (1.0, 0.0, 1.0, 1.0),
            (1.5, 1.0, 0.575, 0.5),
        ] {
            let status = program.drive_to(&mut callbacks, time).unwrap();
            if let LiveProgramStatus::PublicationPending(expected) = status {
                let context = program.take_renderer_publication().context();
                assert_eq!(context, expected);
                program.admit_publication(context).unwrap();
                assert!(matches!(
                    program.resume().unwrap(),
                    LiveProgramStatus::Awaiting(_)
                ));
            }
            let object = &program.session().frame().objects[0];
            let transform = program.session().frame().render_transform(0);
            assert!((transform.translation.x - x).abs() < 1.0e-6);
            assert!((transform.scale.x - scale).abs() < 1.0e-6);
            assert!((object.appearance - appearance).abs() < 1.0e-6);
        }

        let status = program.drive_to(&mut callbacks, 2.0).unwrap();
        let LiveProgramStatus::PublicationPending(expected) = status else {
            panic!("expected FadeOut completion publication, got {status:?}");
        };
        let context = program.take_renderer_publication().context();
        assert_eq!(context, expected);
        program.admit_publication(context).unwrap();
        assert_eq!(program.resume().unwrap(), LiveProgramStatus::Finished);
        assert!(!program.session().frame().is_present(0));
    }
}
