//! Typed geometry and bounds-dependent construction across shared continuation barriers.

use crate::{
    AnimationCompositionRequest, AnimationOptions, Color, ContinuationStep, LiveContinuation,
    LiveProgram, LiveSession, ManimGeometryOptions, ManimLineEndpoints, Mobject,
    MobjectFamilyMember, RateFunction, Scene, SemanticAnimationCompositionKind, TransformToRequest,
    Vec2, VectorPath,
};
use std::rc::Rc;

const LINE_START: (f64, f64) = (-1.111_538_105_676_658, -3.074_759_526_419_164_5);
const LINE_END: (f64, f64) = (1.111_538_105_676_658, -0.925_240_473_580_835_5);
const LINE_COLOR: Color = Color::rgb(0.2, 0.4, 0.8);
const LINE_OPACITY: f64 = 0.35;

pub struct LiveGeometryConstruction {
    stage: u8,
    rectangle: Option<Mobject>,
    line: Option<Mobject>,
}

impl LiveContinuation for LiveGeometryConstruction {
    type Error = String;

    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, String> {
        match self.stage {
            0 => {
                self.stage = 1;
                live.wait_segment(1.0)
                    .map(ContinuationStep::Await)
                    .map_err(|e| e.to_string())
            }
            1 => {
                let mut rectangle =
                    ManimGeometryOptions::rectangle(1.2, 0.8).map_err(|error| error.to_string())?;
                rectangle
                    .set_translation(2.0, 0.0)
                    .map_err(|error| error.to_string())?;
                rectangle
                    .set_fill(0.0, 1.0, 0.0, 1.0)
                    .map_err(|error| error.to_string())?;
                rectangle.disable_stroke();
                let rectangle = live
                    .create_manim_geometry(rectangle)
                    .map_err(|e| e.to_string())?;
                let mut line = ManimGeometryOptions::line(-1.0, -0.5, 1.0, 0.5)
                    .map_err(|error| error.to_string())?;
                line.set_scale(1.5, 0.75)
                    .map_err(|error| error.to_string())?;
                line.set_rotation(std::f64::consts::PI / 6.0)
                    .map_err(|error| error.to_string())?;
                line.set_translation(0.0, -2.0)
                    .map_err(|error| error.to_string())?;
                line.set_fill_color(1.0, 1.0, 0.0, 0.7)
                    .map_err(|error| error.to_string())?;
                line.set_stroke_color(
                    f64::from(LINE_COLOR.red),
                    f64::from(LINE_COLOR.green),
                    f64::from(LINE_COLOR.blue),
                    1.0,
                )
                .map_err(|error| error.to_string())?;
                line.set_stroke_opacity(LINE_OPACITY)
                    .map_err(|error| error.to_string())?;
                line.set_stroke_width(0.04)
                    .map_err(|error| error.to_string())?;
                let line = live
                    .create_manim_geometry(line)
                    .map_err(|e| e.to_string())?;
                assert_endpoints(
                    line.manim_line_endpoints()
                        .map_err(|error| error.to_string())?,
                    LINE_START,
                    LINE_END,
                )?;
                assert_color(
                    line.manim_color().map_err(|error| error.to_string())?,
                    Color::rgb(1.0, 1.0, 0.0),
                )?;
                let mut late_path = ManimGeometryOptions::path(
                    VectorPath::new()
                        .move_to(Vec2::new(-0.3, -0.3))
                        .line_to(Vec2::new(0.3, -0.3))
                        .line_to(Vec2::new(0.0, 0.3))
                        .close(),
                )
                .map_err(|error| error.to_string())?;
                late_path
                    .set_translation(0.0, 2.0)
                    .map_err(|error| error.to_string())?;
                late_path
                    .set_fill(0.0, 1.0, 1.0, 1.0)
                    .map_err(|error| error.to_string())?;
                late_path.disable_stroke();
                let late_path = live
                    .create_manim_geometry(late_path)
                    .map_err(|e| e.to_string())?;
                live.add_many(&[
                    MobjectFamilyMember::Mobject(&rectangle),
                    MobjectFamilyMember::Mobject(&line),
                    MobjectFamilyMember::Mobject(&late_path),
                ])
                .map_err(|e| e.to_string())?;
                let target = live.target_editor(&rectangle).map_err(|e| e.to_string())?;
                live.set_translation(&target, 2.0, 1.0)
                    .map_err(|e| e.to_string())?;
                let line_target = live.target_editor(&line).map_err(|e| e.to_string())?;
                live.set_translation(&line_target, 0.0, -1.0)
                    .map_err(|e| e.to_string())?;
                self.rectangle = Some(rectangle.clone());
                self.line = Some(line.clone());
                self.stage = 2;
                let options = AnimationOptions::new()
                    .run_time(1.0)
                    .rate_func(RateFunction::Linear);
                live.declare_and_activate_animation_composition(
                    SemanticAnimationCompositionKind::Parallel,
                    &[
                        AnimationCompositionRequest::TransformTo(TransformToRequest::new(
                            &rectangle, &target, options,
                        )),
                        AnimationCompositionRequest::TransformTo(TransformToRequest::new(
                            &line,
                            &line_target,
                            options,
                        )),
                    ],
                    options,
                    AnimationOptions::new(),
                )
                .map(ContinuationStep::Await)
                .map_err(|e| e.to_string())
            }
            2 => {
                let line = self.line.as_ref().ok_or("missing live line")?;
                assert_endpoints(
                    live.effective_line_endpoints(line)
                        .map_err(|error| error.to_string())?,
                    (LINE_START.0, LINE_START.1 + 1.0),
                    (LINE_END.0, LINE_END.1 + 1.0),
                )?;
                assert_color(
                    live.effective_manim_color(line)
                        .map_err(|error| error.to_string())?,
                    Color::rgb(1.0, 1.0, 0.0),
                )?;
                let mut dot = ManimGeometryOptions::dot(-4.0, -1.5, 0.25)
                    .map_err(|error| error.to_string())?;
                dot.set_color(1.0, 0.0, 0.0, 1.0)
                    .map_err(|error| error.to_string())?;
                let dot = live.create_manim_geometry(dot).map_err(|e| e.to_string())?;
                let mut annulus = ManimGeometryOptions::annulus(0.25, 0.5, 9, 4.0, -1.5)
                    .map_err(|error| error.to_string())?;
                annulus
                    .set_color(1.0, 1.0, 0.0, 1.0)
                    .map_err(|error| error.to_string())?;
                let annulus = live
                    .create_manim_geometry(annulus)
                    .map_err(|e| e.to_string())?;
                let layout = live
                    .effective_layout(self.rectangle.as_ref().ok_or("missing live rectangle")?)
                    .map_err(|e| e.to_string())?;
                let bounds = noon_core::Bounds2D64 {
                    min_x: layout.center.0 - layout.width * 0.5,
                    max_x: layout.center.0 + layout.width * 0.5,
                    min_y: layout.center.1 - layout.height * 0.5,
                    max_y: layout.center.1 + layout.height * 0.5,
                };
                let mut underline = ManimGeometryOptions::underline(bounds, 0.15)
                    .map_err(|error| error.to_string())?;
                // Rust widths use scene units; Python's Manim width 8 maps to 0.08.
                underline
                    .set_stroke_width(0.08)
                    .map_err(|error| error.to_string())?;
                let underline = live
                    .create_manim_geometry(underline)
                    .map_err(|e| e.to_string())?;
                live.add_many(&[
                    MobjectFamilyMember::Mobject(&dot),
                    MobjectFamilyMember::Mobject(&annulus),
                    MobjectFamilyMember::Mobject(&underline),
                ])
                .map_err(|e| e.to_string())?;
                self.stage = 3;
                Ok(ContinuationStep::Finished)
            }
            _ => Err("geometry continuation resumed after completion".into()),
        }
    }
}

fn assert_endpoints(
    actual: ManimLineEndpoints,
    expected_start: (f64, f64),
    expected_end: (f64, f64),
) -> Result<(), String> {
    for (actual, expected, label) in [
        (actual.start.0, expected_start.0, "start x"),
        (actual.start.1, expected_start.1, "start y"),
        (actual.end.0, expected_end.0, "end x"),
        (actual.end.1, expected_end.1, "end y"),
    ] {
        if (actual - expected).abs() > 1.0e-6 {
            return Err(format!("unexpected Line {label}: {actual} != {expected}"));
        }
    }
    Ok(())
}

fn assert_color(actual: Color, expected: Color) -> Result<(), String> {
    for (actual, expected, label) in [
        (actual.red, expected.red, "red"),
        (actual.green, expected.green, "green"),
        (actual.blue, expected.blue, "blue"),
        (actual.alpha, expected.alpha, "alpha"),
    ] {
        if (actual - expected).abs() > 1.0e-6 {
            return Err(format!(
                "unexpected Line color {label}: {actual} != {expected}"
            ));
        }
    }
    Ok(())
}

/// The Python pair also asks a one-leaf family for the background's shared bounds.
pub fn program() -> Result<LiveProgram<LiveGeometryConstruction>, String> {
    let mut scene = Scene::new();
    let path = VectorPath::new()
        .move_to(Vec2::new(-0.6, -0.5))
        .line_to(Vec2::new(0.6, -0.5))
        .quadratic_to(Vec2::new(0.8, 0.0), Vec2::new(0.6, 0.5))
        .cubic_to(
            Vec2::new(0.2, 0.7),
            Vec2::new(-0.2, 0.7),
            Vec2::new(-0.6, 0.5),
        )
        .close();
    let mut options = ManimGeometryOptions::path(path).map_err(|error| error.to_string())?;
    options
        .set_translation(-2.0, 0.0)
        .map_err(|error| error.to_string())?;
    options
        .set_fill(0.0, 0.0, 1.0, 1.0)
        .map_err(|error| error.to_string())?;
    options.disable_stroke();
    let path = Mobject::from_manim_geometry(Rc::clone(scene.integration_store()), options)
        .map_err(|error| error.to_string())?;
    let bounds = path
        .layout_bounds()
        .map_err(|error| error.to_string())?
        .ok_or("path has no bounds")?;
    let family_bounds = scene
        .family(&[(&path).into()])
        .map_err(|error| error.to_string())?
        .layout_bounds()
        .map_err(|error| error.to_string())?
        .ok_or("path family has no bounds")?;
    let outline = Mobject::from_manim_geometry(
        Rc::clone(scene.integration_store()),
        ManimGeometryOptions::surrounding_rectangle(bounds, 0.15, 0.15, 0.1)
            .map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    let background = Mobject::from_manim_geometry(
        Rc::clone(scene.integration_store()),
        ManimGeometryOptions::background_rectangle(family_bounds, 0.25, 0.25, 0.1, 0.5)
            .map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    scene
        .add_many(&[
            MobjectFamilyMember::Mobject(&background),
            MobjectFamilyMember::Mobject(&path),
            MobjectFamilyMember::Mobject(&outline),
        ])
        .map_err(|error| error.to_string())?;
    scene
        .into_live_program(LiveGeometryConstruction {
            stage: 0,
            rectangle: None,
            line: None,
        })
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use crate::{LiveProgramStatus, RustHostCallbackTable};

    #[test]
    fn path_resources_and_late_geometry_share_one_continuation() {
        let mut program = super::program().unwrap();
        let mut callbacks = RustHostCallbackTable::new();
        assert_eq!(program.session().frame().objects.len(), 3);
        assert!(matches!(
            program.resume().unwrap(),
            LiveProgramStatus::Awaiting(_)
        ));
        program.take_renderer_publication();
        assert_eq!(
            program.drive_to(&mut callbacks, 1.0).unwrap(),
            LiveProgramStatus::ReadyToResume
        );
        assert!(matches!(
            program.resume().unwrap(),
            LiveProgramStatus::Awaiting(_)
        ));
        assert_eq!(program.session().frame().objects.len(), 6);
        program.take_renderer_publication();
        program.drive_to(&mut callbacks, 2.0).unwrap();
        let publication = program.take_renderer_publication().context();
        program.admit_publication(publication).unwrap();
        assert_eq!(program.resume().unwrap(), LiveProgramStatus::Finished);
        assert_eq!(program.session().frame().objects.len(), 9);
        let underline = &program.session().frame().objects[8];
        assert!(
            (underline.transform.translation.y - 0.45).abs() < 1e-6,
            "Underline must observe the effective target after animation"
        );
        let rectangle = &program.session().frame().objects[3];
        assert_eq!(rectangle.transform.translation, Vec2::new(2.0, 1.0));
    }
    use crate::Vec2;
}
