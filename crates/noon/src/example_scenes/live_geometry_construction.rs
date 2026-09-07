//! Typed path and matcher construction before and after a shared continuation barrier.

use crate::{
    AnimationOptions, ContinuationStep, LiveContinuation, LiveProgram, LiveSession,
    ManimGeometryOptions, Mobject, MobjectFamilyMember, RateFunction, Scene, Vec2, VectorPath,
};
use std::rc::Rc;

pub struct LiveGeometryConstruction {
    stage: u8,
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
                let mut rectangle = ManimGeometryOptions::rectangle(1.2, 0.8)?;
                rectangle.set_translation(2.0, 0.0)?;
                rectangle.set_fill(0.0, 1.0, 0.0, 1.0)?;
                rectangle.disable_stroke();
                let rectangle = live
                    .create_manim_geometry(rectangle)
                    .map_err(|e| e.to_string())?;
                let mut line = ManimGeometryOptions::line(-1.0, -2.0, 1.0, -2.0)?;
                line.set_stroke_width(0.04)?;
                let line = live
                    .create_manim_geometry(line)
                    .map_err(|e| e.to_string())?;
                let mut late_path = ManimGeometryOptions::path(
                    VectorPath::new()
                        .move_to(Vec2::new(-0.3, -0.3))
                        .line_to(Vec2::new(0.3, -0.3))
                        .line_to(Vec2::new(0.0, 0.3))
                        .close(),
                )?;
                late_path.set_translation(0.0, 2.0)?;
                late_path.set_fill(0.0, 1.0, 1.0, 1.0)?;
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
                self.stage = 2;
                live.declare_and_activate_transform_to(
                    &rectangle,
                    &target,
                    AnimationOptions::new()
                        .run_time(1.0)
                        .rate_func(RateFunction::Linear),
                )
                .map(ContinuationStep::Await)
                .map_err(|e| e.to_string())
            }
            2 => {
                self.stage = 3;
                Ok(ContinuationStep::Finished)
            }
            _ => Err("geometry continuation resumed after completion".into()),
        }
    }
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
    let mut options = ManimGeometryOptions::path(path)?;
    options.set_translation(-2.0, 0.0)?;
    options.set_fill(0.0, 0.0, 1.0, 1.0)?;
    options.disable_stroke();
    let path = Mobject::from_manim_geometry(Rc::clone(scene.store()), options)?;
    let bounds = path.layout_bounds()?.ok_or("path has no bounds")?;
    let family_bounds = scene
        .family(&[&path])?
        .layout_bounds()?
        .ok_or("path family has no bounds")?;
    let outline = Mobject::from_manim_geometry(
        Rc::clone(scene.store()),
        ManimGeometryOptions::surrounding_rectangle(bounds, 0.15, 0.15, 0.1)?,
    )?;
    let background = Mobject::from_manim_geometry(
        Rc::clone(scene.store()),
        ManimGeometryOptions::background_rectangle(family_bounds, 0.25, 0.25, 0.1, 0.5)?,
    )?;
    scene.add_many(&[
        MobjectFamilyMember::Mobject(&background),
        MobjectFamilyMember::Mobject(&path),
        MobjectFamilyMember::Mobject(&outline),
    ])?;
    scene
        .into_live_program(LiveGeometryConstruction { stage: 0 })
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
        let rectangle = &program.session().frame().objects[3];
        assert_eq!(rectangle.transform.translation, Vec2::new(2.0, 1.0));
    }
    use crate::Vec2;
}
