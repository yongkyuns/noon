//! Same-identity flagged `become` across a live continuation barrier.

use crate::{
    ContinuationStep, LiveContinuation, LiveProgram, LiveSession, ManimBecomeOptions,
    ManimGeometryOptions, Mobject, MobjectFamilyMember, Scene,
};
use std::rc::Rc;

const ELLIPSE_HULL_WIDTH: f64 = 3.663_013_982_517_412_6;
const ELLIPSE_HULL_HEIGHT: f64 = 2.464_228_071_008_26;

pub struct OrdinaryBecomeSemantics {
    fitted: Mobject,
    fitted_target: Mobject,
    stretched: Mobject,
    stretched_target: Mobject,
    ellipse: Mobject,
    ellipse_target: Mobject,
    stage: u8,
}

impl LiveContinuation for OrdinaryBecomeSemantics {
    type Error = String;

    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, String> {
        match self.stage {
            0 => {
                self.stage = 1;
                live.wait_segment(0.5)
                    .map(ContinuationStep::Await)
                    .map_err(|error| error.to_string())
            }
            1 => {
                live.become_mobject(
                    &self.fitted,
                    &self.fitted_target,
                    ManimBecomeOptions {
                        match_height: true,
                        match_width: true,
                        match_center: true,
                        stretch: false,
                    },
                )
                .map_err(|error| error.to_string())?;
                live.become_mobject(
                    &self.stretched,
                    &self.stretched_target,
                    ManimBecomeOptions {
                        match_height: true,
                        match_width: true,
                        match_center: true,
                        stretch: true,
                    },
                )
                .map_err(|error| error.to_string())?;
                live.become_mobject(
                    &self.ellipse,
                    &self.ellipse_target,
                    ManimBecomeOptions {
                        match_center: true,
                        ..ManimBecomeOptions::default()
                    },
                )
                .map_err(|error| error.to_string())?;

                for target in [
                    &self.fitted_target,
                    &self.stretched_target,
                    &self.ellipse_target,
                ] {
                    if live.contains(target).map_err(|error| error.to_string())? {
                        return Err("become admitted a detached target operand".into());
                    }
                }
                assert_layout(live, &self.fitted, -2.0, 0.0, 3.0, 6.0)?;
                assert_layout(live, &self.stretched, 2.0, 0.0, 3.0, 1.0)?;
                assert_layout(
                    live,
                    &self.ellipse,
                    0.0,
                    -2.5,
                    ELLIPSE_HULL_WIDTH,
                    ELLIPSE_HULL_HEIGHT,
                )?;
                let ellipse_state = self.ellipse.state()?;
                let ellipse_target_state = self.ellipse_target.state()?;
                if ellipse_state.content != ellipse_target_state.content
                    || ellipse_state.style != ellipse_target_state.style
                {
                    return Err(
                        "become did not preserve the ellipse target content and style".into(),
                    );
                }

                self.stage = 2;
                live.wait_segment(0.25)
                    .map(ContinuationStep::Await)
                    .map_err(|error| error.to_string())
            }
            2 => {
                assert_layout(live, &self.fitted, -2.0, 0.0, 3.0, 6.0)?;
                assert_layout(live, &self.stretched, 2.0, 0.0, 3.0, 1.0)?;
                assert_layout(
                    live,
                    &self.ellipse,
                    0.0,
                    -2.5,
                    ELLIPSE_HULL_WIDTH,
                    ELLIPSE_HULL_HEIGHT,
                )?;
                self.stage = 3;
                Ok(ContinuationStep::Finished)
            }
            _ => Err("become continuation resumed after completion".into()),
        }
    }
}

fn assert_layout(
    live: &LiveSession<'_>,
    object: &Mobject,
    center_x: f64,
    center_y: f64,
    width: f64,
    height: f64,
) -> Result<(), String> {
    let layout = live
        .effective_layout(object)
        .map_err(|error| error.to_string())?;
    for (actual, expected, label) in [
        (layout.center.0, center_x, "center x"),
        (layout.center.1, center_y, "center y"),
        (layout.width, width, "width"),
        (layout.height, height, "height"),
    ] {
        if (actual - expected).abs() > 1.0e-6 {
            return Err(format!("unexpected become {label}: {actual} != {expected}"));
        }
    }
    Ok(())
}

fn rectangle(
    scene: &Scene,
    width: f64,
    height: f64,
    x: f64,
    red: f64,
    green: f64,
    blue: f64,
) -> Result<Mobject, String> {
    let mut options = ManimGeometryOptions::rectangle(width, height)?;
    options.set_translation(x, 0.0)?;
    options.set_fill(red, green, blue, 1.0)?;
    options.disable_stroke();
    Mobject::from_manim_geometry(Rc::clone(scene.integration_store()), options)
}

pub fn program() -> Result<LiveProgram<OrdinaryBecomeSemantics>, String> {
    let mut scene = Scene::new();
    let fitted = rectangle(&scene, 3.0, 1.0, -2.0, 0.0, 0.0, 1.0)?;
    let stretched = rectangle(&scene, 3.0, 1.0, 2.0, 0.0, 0.0, 1.0)?;
    let mut ellipse = rectangle(&scene, 0.4, 0.4, 0.0, 0.0, 0.0, 1.0)?;
    ellipse.set_translation(0.0, -2.5)?;
    let fitted_target = rectangle(&scene, 1.0, 2.0, 3.5, 1.0, 1.0, 0.0)?;
    let mut stretched_target_options = ManimGeometryOptions::circle(0.5)?;
    stretched_target_options.set_translation(-3.5, 0.0)?;
    stretched_target_options.set_fill(1.0, 0.0, 1.0, 1.0)?;
    stretched_target_options.disable_stroke();
    let stretched_target = Mobject::from_manim_geometry(
        Rc::clone(scene.integration_store()),
        stretched_target_options,
    )?;
    let mut ellipse_target_options = ManimGeometryOptions::ellipse(4.0, 1.5)?;
    ellipse_target_options.set_translation(0.0, 2.5)?;
    ellipse_target_options.set_rotation(std::f64::consts::PI / 6.0)?;
    ellipse_target_options.set_fill(0.0, 1.0, 1.0, 1.0)?;
    ellipse_target_options.disable_stroke();
    let ellipse_target =
        Mobject::from_manim_geometry(Rc::clone(scene.integration_store()), ellipse_target_options)?;
    let ellipse_target_bounds = ellipse_target
        .layout_bounds()?
        .ok_or("ellipse target has no authored layout bounds")?;
    for (actual, expected, label) in [
        (
            (ellipse_target_bounds.min_x + ellipse_target_bounds.max_x) * 0.5,
            0.0,
            "center x",
        ),
        (
            (ellipse_target_bounds.min_y + ellipse_target_bounds.max_y) * 0.5,
            2.5,
            "center y",
        ),
        (ellipse_target_bounds.width(), ELLIPSE_HULL_WIDTH, "width"),
        (
            ellipse_target_bounds.height(),
            ELLIPSE_HULL_HEIGHT,
            "height",
        ),
    ] {
        if (actual - expected).abs() > 1.0e-5 {
            return Err(format!(
                "unexpected pre-bootstrap ellipse {label}: {actual} != {expected}"
            ));
        }
    }
    scene
        .add_many(&[
            MobjectFamilyMember::Mobject(&fitted),
            MobjectFamilyMember::Mobject(&stretched),
            MobjectFamilyMember::Mobject(&ellipse),
        ])
        .map_err(|error| error.to_string())?;
    scene
        .into_live_program(OrdinaryBecomeSemantics {
            fitted,
            fitted_target,
            stretched,
            stretched_target,
            ellipse,
            ellipse_target,
            stage: 0,
        })
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use crate::{LiveProgramStatus, RustHostCallbackTable};

    #[test]
    fn flagged_become_preserves_source_roots_and_detached_targets() {
        let mut program = super::program().unwrap();
        let mut callbacks = RustHostCallbackTable::new();
        assert_eq!(program.session().frame().objects.len(), 3);
        assert!(matches!(
            program.resume().unwrap(),
            LiveProgramStatus::Awaiting(_)
        ));
        program.take_renderer_publication();
        assert_eq!(
            program.drive_to(&mut callbacks, 0.5).unwrap(),
            LiveProgramStatus::ReadyToResume
        );
        assert!(matches!(
            program.resume().unwrap(),
            LiveProgramStatus::Awaiting(_)
        ));
        assert_eq!(program.session().frame().objects.len(), 3);
        program.take_renderer_publication();
        assert_eq!(
            program.drive_to(&mut callbacks, 0.75).unwrap(),
            LiveProgramStatus::ReadyToResume
        );
        assert_eq!(program.resume().unwrap(), LiveProgramStatus::Finished);
        assert_eq!(program.session().frame().objects.len(), 3);
    }
}
