//! Native family placement with object, point and family targets.
use crate::{
    semantic_mobject::ManimNextToArgs, Color, ExecutionSession, FamilyLayoutTarget as Target,
    Mobject, Scene,
};
use std::rc::Rc;

pub fn session() -> Result<ExecutionSession, String> {
    let mut scene = Scene::new();
    let mut first = Mobject::manim_circle(Rc::clone(scene.integration_store()), 0.2)?;
    let mut second = Mobject::manim_circle(Rc::clone(scene.integration_store()), 0.2)?;
    let mut anchor = Mobject::manim_square(Rc::clone(scene.integration_store()), 1.0)?;
    for (object, color) in [
        (&mut first, Color::BLUE),
        (&mut second, Color::YELLOW),
        (&mut anchor, Color::RED),
    ] {
        object.set_fill(
            f64::from(color.red),
            f64::from(color.green),
            f64::from(color.blue),
            1.0,
        )?;
        object.set_stroke_width(0.0)?;
    }
    second.shift(0.8, 0.0)?;
    let nested = scene.family(&[(&second).into()])?;
    let family = scene.family(&[])?;
    family.add_many(&[(&first).into(), (&nested).into(), (&first).into()])?;
    family.remove_many(&[(&first).into(), (&nested).into()])?;
    family.add_many(&[(&first).into(), (&nested).into()])?;
    let target = scene.family(&[(&anchor).into()])?;
    let source = crate::LayoutAnchor::from(&family);
    let target_member = crate::LayoutAnchor::from(&target).member(0);
    source.next_to_aligned(
        Target::Anchor(&target_member),
        &source.clone().member(0),
        ManimNextToArgs {
            direction: (2.0, 0.0),
            buff: 0.25,
            aligned_edge: (0.0, 0.0),
            mask: (1.0, 1.0),
        },
    )?;
    family
        .layout()?
        .align_to(Target::Point(0.0, 1.0), (0.0, 1.0))?;
    family
        .layout()?
        .move_to(Target::Family(&target.layout()?), (0.0, 0.0), (1.0, 0.0))?;
    family.shift(0.0, 0.2)?;
    for object in [&first, &second, &anchor] {
        scene.add(object)?;
    }
    scene.execution_session().map_err(|e| e.to_string())
}
