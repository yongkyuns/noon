//! Shape-keyed family replacement lifecycle shared by native and direct WASM qualification.

use std::rc::Rc;

use crate::{
    AnimationOptions, Color, ContinuationStep, IndicateOptions, LiveContinuation, LiveProgram,
    LiveSession, ManimGeometryOptions, Mobject, MobjectFamily, RateFunction, Scene, Vec2,
    VectorPath,
};

pub struct MatchingShapesLifecycle {
    source: MobjectFamily,
    target: MobjectFamily,
    stage: u8,
}

impl LiveContinuation for MatchingShapesLifecycle {
    type Error = String;

    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, String> {
        match self.stage {
            0 => {
                let segment = live
                    .declare_and_activate_matching_family_transform_to(
                        &self.source,
                        &self.target,
                        AnimationOptions::new()
                            .run_time(1.0)
                            .rate_func(RateFunction::Linear),
                    )
                    .map_err(|error| error.to_string())?;
                self.stage = 1;
                Ok(ContinuationStep::Await(segment))
            }
            1 => {
                // Successful activation proves matching completion replaced the source family
                // with the original authored target rather than a copied endpoint.
                let segment = live
                    .declare_and_activate_family_indicate(
                        &self.target,
                        IndicateOptions::default(),
                        AnimationOptions::new()
                            .run_time(1.0)
                            .rate_func(RateFunction::ThereAndBack),
                    )
                    .map_err(|error| error.to_string())?;
                self.stage = 2;
                Ok(ContinuationStep::Await(segment))
            }
            2 => {
                self.stage = 3;
                Ok(ContinuationStep::Finished)
            }
            _ => Err("matching-shapes lifecycle resumed after completion".into()),
        }
    }
}

fn triangle() -> VectorPath {
    VectorPath::new()
        .move_to(Vec2::new(-1.0, -1.0))
        .line_to(Vec2::new(1.0, -0.5))
        .line_to(Vec2::new(-0.25, 1.0))
        .close()
}

fn kite() -> VectorPath {
    VectorPath::new()
        .move_to(Vec2::new(0.0, -1.0))
        .line_to(Vec2::new(1.5, 0.0))
        .line_to(Vec2::new(0.0, 1.0))
        .line_to(Vec2::new(-0.5, 0.0))
        .close()
}

fn shape(scene: &Scene, path: VectorPath, x: f64, color: Color) -> Result<Mobject, String> {
    let mut object = Mobject::from_manim_geometry(
        Rc::clone(scene.integration_store()),
        ManimGeometryOptions::path(path).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    object
        .set_translation(x, 0.0)
        .map_err(|error| error.to_string())?;
    object
        .set_fill(
            f64::from(color.red),
            f64::from(color.green),
            f64::from(color.blue),
            0.9,
        )
        .map_err(|error| error.to_string())?;
    object
        .set_stroke_opacity(0.0)
        .map_err(|error| error.to_string())?;
    Ok(object)
}

/// Source order is triangle/kite while target order is kite/triangle.
///
/// At t=0.5 shape matching places triangle at x=1 and kite at x=-1. Index pairing would place
/// them near x=-3 and x=3. Exact completion replaces the source family with the authored target,
/// which is then used by the following `Indicate`.
pub fn program() -> Result<LiveProgram<MatchingShapesLifecycle>, String> {
    let mut scene = Scene::new();
    let source_triangle = shape(&scene, triangle(), -2.0, Color::PINK)?;
    let source_kite = shape(&scene, kite(), 2.0, Color::BLUE)?;
    let source = scene
        .family(&[(&source_triangle).into(), (&source_kite).into()])
        .map_err(|error| error.to_string())?;
    scene
        .add_many(&[(&source).into()])
        .map_err(|error| error.to_string())?;

    let target_kite = shape(&scene, kite(), -4.0, Color::BLUE)?;
    let target_triangle = shape(&scene, triangle(), 4.0, Color::PINK)?;
    let target = scene
        .family(&[(&target_kite).into(), (&target_triangle).into()])
        .map_err(|error| error.to_string())?;

    scene
        .into_live_program(MatchingShapesLifecycle {
            source,
            target,
            stage: 0,
        })
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LiveProgramStatus, RustHostCallbackTable};

    fn admit_completion(
        program: &mut LiveProgram<MatchingShapesLifecycle>,
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

    fn painter_xs(program: &LiveProgram<MatchingShapesLifecycle>) -> Vec<f32> {
        let session = program.session();
        session
            .painter_order()
            .iter()
            .map(|&index| {
                session.frame().objects[index as usize]
                    .transform
                    .translation
                    .x
            })
            .collect()
    }

    fn contains_x(values: &[f32], expected: f32) -> bool {
        values.iter().any(|value| (*value - expected).abs() < 1e-5)
    }

    #[test]
    fn native_matches_geometry_then_animates_the_replacement_target() {
        let mut program = program().unwrap();
        let mut callbacks = RustHostCallbackTable::new();
        assert!(matches!(
            program.resume().unwrap(),
            LiveProgramStatus::Awaiting(_)
        ));

        assert!(matches!(
            program.drive_to(&mut callbacks, 0.5).unwrap(),
            LiveProgramStatus::Awaiting(_)
        ));
        // Durable runtime slots may include target-side rows before semantic replacement. Painter
        // order is the renderer-facing live membership and therefore the correct observable here.
        let midpoint = painter_xs(&program);
        assert_eq!(midpoint.len(), 2);
        assert!(contains_x(&midpoint, -1.0));
        assert!(contains_x(&midpoint, 1.0));
        assert!(!contains_x(&midpoint, -3.0));
        assert!(!contains_x(&midpoint, 3.0));

        admit_completion(&mut program, &mut callbacks, 1.0);
        assert!(matches!(
            program.resume().unwrap(),
            LiveProgramStatus::Awaiting(_)
        ));
        let completed = painter_xs(&program);
        assert_eq!(completed.len(), 2);
        assert!(contains_x(&completed, -4.0));
        assert!(contains_x(&completed, 4.0));

        assert!(matches!(
            program.drive_to(&mut callbacks, 1.5).unwrap(),
            LiveProgramStatus::Awaiting(_)
        ));
        let painter_order = program.session().painter_order().to_vec();
        assert_eq!(painter_order.len(), 2);
        for index in painter_order {
            assert!(
                program
                    .session()
                    .frame()
                    .render_transform(index as usize)
                    .scale
                    .x
                    > 1.15
            );
        }

        admit_completion(&mut program, &mut callbacks, 2.0);
        assert_eq!(program.resume().unwrap(), LiveProgramStatus::Finished);
        let restored = painter_xs(&program);
        assert_eq!(restored.len(), 2);
        assert!(contains_x(&restored, -4.0));
        assert!(contains_x(&restored, 4.0));
    }
}
