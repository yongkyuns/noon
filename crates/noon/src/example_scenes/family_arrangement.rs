//! Shared nested-family arrangement before and after a logical wait boundary.
use std::rc::Rc;

use crate::{
    Color, ContinuationStep, LiveContinuation, LiveProgram, LiveSession, Mobject, MobjectFamily,
    Scene,
};

pub struct FamilyArrangement {
    family: MobjectFamily,
    first: Mobject,
    second: Mobject,
    nested: MobjectFamily,
    stage: u8,
}

impl LiveContinuation for FamilyArrangement {
    type Error = String;

    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, String> {
        match self.stage {
            0 => {
                self.stage = 1;
                live.wait_segment(0.5)
                    .map(ContinuationStep::Await)
                    .map_err(|e| e.to_string())
            }
            1 => {
                self.family = live
                    .family(&[(&self.first).into(), (&self.nested).into()])
                    .map_err(|e| e.to_string())?;
                live.remove_family_members(&self.family, &[(&self.nested).into()])
                    .map_err(|e| e.to_string())?;
                live.add_family_members(&self.family, &[(&self.nested).into()])
                    .map_err(|e| e.to_string())?;
                let mut options = crate::FamilyArrangeOptions::new(0.0, 1.0, 0.3, false);
                options.placement.aligned_edge = (-1.0, 0.0);
                live.arrange_family_with_options(&self.family, &options)
                    .map_err(|e| e.to_string())?;
                let layout = live
                    .effective_family_layout(&self.nested)
                    .map_err(|e| e.to_string())?;
                if (layout.width - 2.4).abs() > 1e-5 {
                    return Err("live family layout did not observe the arranged members".into());
                }
                if layout.center.0.abs() > 1e-5 {
                    return Err("aligned arrangement moved the common left edge".into());
                }
                live.move_family_to(
                    &self.nested,
                    crate::LiveLayoutTarget::Point(0.0, 0.0),
                    (0.0, 0.0),
                    (1.0, 0.0),
                )
                .map_err(|e| e.to_string())?;
                let source = crate::LayoutAnchor::from(&self.nested);
                live.next_layout_to_aligned(
                    &source,
                    crate::LiveLayoutTarget::Point(0.0, 0.0),
                    &source.clone().member(-1),
                    crate::semantic_mobject::ManimNextToArgs {
                        direction: (0.0, 2.0),
                        buff: 0.25,
                        aligned_edge: (0.0, 0.0),
                        mask: (0.0, 1.0),
                    },
                )
                .map_err(|e| e.to_string())?;
                live.align_family_to(
                    &self.nested,
                    crate::LiveLayoutTarget::Mobject(&self.first),
                    (0.0, 1.0),
                )
                .map_err(|e| e.to_string())?;
                let first = crate::LayoutAnchor::from(&self.first);
                live.next_layout_to_aligned(
                    &first,
                    crate::LiveLayoutTarget::Mobject(&self.second),
                    &first,
                    crate::semantic_mobject::ManimNextToArgs {
                        direction: (-1.0, 0.0),
                        buff: 0.25,
                        aligned_edge: (0.0, 0.0),
                        mask: (1.0, 1.0),
                    },
                )
                .map_err(|e| e.to_string())?;
                let placed = live
                    .effective_layout(&self.first)
                    .map_err(|e| e.to_string())?;
                if (placed.center.0 - 0.35).abs() > 1e-6 {
                    return Err(
                        "plain live object placement missed its shared target bounds".into(),
                    );
                }
                self.stage = 2;
                live.wait_segment(0.5)
                    .map(ContinuationStep::Await)
                    .map_err(|e| e.to_string())
            }
            2 => {
                self.stage = 3;
                Ok(ContinuationStep::Finished)
            }
            _ => Err("family arrangement resumed after completion".into()),
        }
    }
}

pub fn program() -> Result<LiveProgram<FamilyArrangement>, String> {
    let mut scene = Scene::new();
    let mut first = Mobject::manim_circle(Rc::clone(scene.integration_store()), 0.2)?;
    let mut second = Mobject::manim_circle(Rc::clone(scene.integration_store()), 0.2)?;
    for (object, color) in [(&mut first, Color::BLUE), (&mut second, Color::YELLOW)] {
        object.set_fill(
            f64::from(color.red),
            f64::from(color.green),
            f64::from(color.blue),
            1.0,
        )?;
        object.set_stroke_width(0.0)?;
    }
    second.shift(2.0, 0.0)?;
    let nested = scene.family(&[(&first).into(), (&second).into()])?;
    let family = scene.family(&[(&first).into(), (&nested).into()])?;
    let bounds = family.layout_bounds()?.ok_or("family bounds are empty")?;
    if (bounds.width() - 2.4).abs() > 1e-6 || (bounds.height() - 0.4).abs() > 1e-6 {
        return Err("shared family bounds differ from its authored members".into());
    }
    family.arrange(1.0, 0.0, 0.2, true)?;
    scene.add(&first).map_err(|error| error.to_string())?;
    scene.add(&second).map_err(|error| error.to_string())?;
    scene
        .into_live_program(FamilyArrangement {
            family,
            first,
            second,
            nested,
            stage: 0,
        })
        .map_err(|e| e.to_string())
}
