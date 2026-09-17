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

fn rotated_triangle() -> VectorPath {
    VectorPath::new()
        .move_to(Vec2::new(1.0, -1.0))
        .line_to(Vec2::new(0.5, 1.0))
        .line_to(Vec2::new(-1.0, -0.25))
        .close()
}

fn shape(scene: &Scene, path: VectorPath, position: Vec2, color: Color) -> Result<Mobject, String> {
    let mut object = Mobject::from_manim_geometry(
        Rc::clone(scene.integration_store()),
        ManimGeometryOptions::path(path).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    object
        .set_translation(f64::from(position.x), f64::from(position.y))
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
    let source_triangle = shape(&scene, triangle(), Vec2::new(-2.0, 0.0), Color::PINK)?;
    let source_kite = shape(&scene, kite(), Vec2::new(2.0, 0.0), Color::BLUE)?;
    let source = scene
        .family(&[(&source_triangle).into(), (&source_kite).into()])
        .map_err(|error| error.to_string())?;
    scene
        .add_many(&[(&source).into()])
        .map_err(|error| error.to_string())?;

    let target_kite = shape(&scene, kite(), Vec2::new(-4.0, 0.0), Color::BLUE)?;
    let target_triangle = shape(&scene, triangle(), Vec2::new(4.0, 0.0), Color::PINK)?;
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

/// Duplicate-key growth plus one unmatched member in each family.
///
/// The three triangles share one matching key. The extra target triangle is a
/// compiler-owned padded occurrence, while the kite and rotated triangle use the
/// default directional FadeOut/FadeIn path. The target-only rotated triangle must
/// remain at its authored `(4, 1.5)` position throughout its fade-in.
pub fn breadth_program() -> Result<LiveProgram<MatchingShapesLifecycle>, String> {
    let mut scene = Scene::new();
    let source_first = shape(&scene, triangle(), Vec2::new(-4.0, 1.5), Color::BLUE)?;
    let source_second = shape(&scene, triangle(), Vec2::new(-1.0, 1.5), Color::GREEN)?;
    let source_leftover = shape(&scene, kite(), Vec2::new(-4.0, -1.5), Color::RED)?;
    let source = scene
        .family(&[
            (&source_first).into(),
            (&source_second).into(),
            (&source_leftover).into(),
        ])
        .map_err(|error| error.to_string())?;
    scene
        .add_many(&[(&source).into()])
        .map_err(|error| error.to_string())?;

    let target_first = shape(&scene, triangle(), Vec2::new(-4.0, -1.5), Color::YELLOW)?;
    let target_second = shape(&scene, triangle(), Vec2::new(-1.0, -1.5), Color::PINK)?;
    let target_padded = shape(&scene, triangle(), Vec2::new(2.0, -1.5), Color::BLUE)?;
    let target_leftover = shape(
        &scene,
        rotated_triangle(),
        Vec2::new(4.0, 1.5),
        Color::WHITE,
    )?;
    let target = scene
        .family(&[
            (&target_first).into(),
            (&target_second).into(),
            (&target_padded).into(),
            (&target_leftover).into(),
        ])
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

    #[test]
    fn native_duplicate_growth_and_default_leftovers_keep_authored_target_position() {
        let mut program = breadth_program().unwrap();
        let mut callbacks = RustHostCallbackTable::new();
        assert!(matches!(
            program.resume().unwrap(),
            LiveProgramStatus::Awaiting(_)
        ));
        assert!(matches!(
            program.drive_to(&mut callbacks, 0.5).unwrap(),
            LiveProgramStatus::Awaiting(_)
        ));

        // The unmatched source kite fades toward the unmatched target group's center.
        let source_leftover = program
            .session()
            .frame()
            .objects
            .iter()
            .find(|row| {
                (row.transform.translation.x + 0.25).abs() < 1e-5
                    && (row.transform.translation.y - 0.0).abs() < 1e-5
                    && row.appearance > 0.0
                    && row.appearance < 1.0
            })
            .expect("source leftover must be halfway through its directional FadeOut");
        assert!((source_leftover.appearance - 0.5).abs() < 1e-5);

        let publication = program.take_renderer_publication();
        let target_leftover = publication
            .transient_presentations()
            .iter()
            .find(|occurrence| {
                let transform = occurrence.state().transform;
                (transform.translation.x - 4.0).abs() < 1e-5
                    && (transform.translation.y - 1.5).abs() < 1e-5
            })
            .expect("target leftover must fade in at its authored position");
        assert!((target_leftover.state().appearance - 0.5).abs() < 1e-5);

        // Manim's 2 -> 3 alignment repeats the first source, not the last.
        // Its copy travels from (-4, 1.5) to (-1, -1.5), and fades from
        // transparent while interpolating the first source's blue to pink.
        let padded = publication
            .transient_presentations()
            .iter()
            .find(|occurrence| {
                // Transform geometry is baked into world-space render endpoints;
                // the semantic transform records the occurrence's interpolated position.
                let transform = occurrence.state().transform;
                (transform.translation.x + 2.5).abs() < 1e-5 && transform.translation.y.abs() < 1e-5
            })
            .expect("the padded first source must have its own midpoint presentation");
        let state = padded.state();
        assert!((state.appearance - 0.5).abs() < 1e-5);
        let fill = state
            .style
            .fill
            .expect("the padded triangle remains filled");
        for (actual, expected) in [
            (fill.red, (Color::BLUE.red + Color::PINK.red) * 0.5),
            (fill.green, (Color::BLUE.green + Color::PINK.green) * 0.5),
            (fill.blue, (Color::BLUE.blue + Color::PINK.blue) * 0.5),
            (fill.alpha, 0.9),
        ] {
            assert!((actual - expected).abs() < 1e-5);
        }

        admit_completion(&mut program, &mut callbacks, 1.0);
        assert!(matches!(
            program.resume().unwrap(),
            LiveProgramStatus::Awaiting(_)
        ));
        let completed = painter_xs(&program);
        assert_eq!(completed.len(), 4);
        for expected in [-4.0, -1.0, 2.0, 4.0] {
            assert!(contains_x(&completed, expected));
        }
        assert!(program
            .take_renderer_publication()
            .transient_presentations()
            .is_empty());
    }
}
