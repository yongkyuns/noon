//! Native family placement with object, point and family targets.
use crate::{
    semantic_mobject::ManimNextToArgs, Color, ExecutionSession, FamilyLayoutTarget as Target,
    Mobject, Scene,
};
use std::rc::Rc;

pub fn session() -> Result<ExecutionSession, String> {
    let mut scene = Scene::new();
    let mut first = Mobject::manim_circle(Rc::clone(scene.integration_store()), 0.2)
        .map_err(|error| error.to_string())?;
    let mut second = Mobject::manim_circle(Rc::clone(scene.integration_store()), 0.2)
        .map_err(|error| error.to_string())?;
    let mut anchor = Mobject::manim_square(Rc::clone(scene.integration_store()), 1.0)
        .map_err(|error| error.to_string())?;
    for (object, color) in [
        (&mut first, Color::BLUE),
        (&mut second, Color::YELLOW),
        (&mut anchor, Color::RED),
    ] {
        object
            .set_fill(
                f64::from(color.red),
                f64::from(color.green),
                f64::from(color.blue),
                1.0,
            )
            .map_err(|error| error.to_string())?;
        object
            .set_stroke_width(0.0)
            .map_err(|error| error.to_string())?;
    }
    second.shift(0.8, 0.0).map_err(|error| error.to_string())?;
    let nested = scene
        .family(&[(&second).into()])
        .map_err(|error| error.to_string())?;
    let family = scene.family(&[]).map_err(|error| error.to_string())?;
    family
        .add_many(&[(&first).into(), (&nested).into(), (&first).into()])
        .map_err(|error| error.to_string())?;
    family
        .remove_many(&[(&first).into(), (&nested).into()])
        .map_err(|error| error.to_string())?;
    family
        .add_many(&[(&first).into(), (&nested).into()])
        .map_err(|error| error.to_string())?;
    let target = scene
        .family(&[(&anchor).into()])
        .map_err(|error| error.to_string())?;
    let first_anchor = crate::LayoutAnchor::from(&first);
    let target_anchor = crate::LayoutAnchor::from(&target);
    first_anchor
        .layout()
        .map_err(|error| error.to_string())?
        .move_to(Target::Anchor(&target_anchor), (0.0, 1.0), (1.0, 1.0))
        .map_err(|error| error.to_string())?;
    first_anchor
        .layout()
        .map_err(|error| error.to_string())?
        .align_to(Target::Anchor(&target_anchor), (0.0, -1.0))
        .map_err(|error| error.to_string())?;
    assert!(
        (first_anchor
            .layout()
            .map_err(|error| error.to_string())?
            .critical_point(0.0, -1.0)
            .1
            - target
                .layout()
                .map_err(|error| error.to_string())?
                .critical_point(0.0, -1.0)
                .1)
            .abs()
            < 1e-6
    );
    let source = crate::LayoutAnchor::from(&family);
    let target_member = crate::LayoutAnchor::from(&target).member(0);
    source
        .next_to_aligned(
            Target::Anchor(&target_member),
            &source.clone().member(0),
            ManimNextToArgs {
                direction: (2.0, 0.0),
                buff: 0.25,
                aligned_edge: (0.0, 0.0),
                mask: (1.0, 1.0),
            },
        )
        .map_err(|error| error.to_string())?;
    family
        .layout()
        .map_err(|error| error.to_string())?
        .align_to(Target::Point(0.0, 1.0), (0.0, 1.0))
        .map_err(|error| error.to_string())?;
    family
        .layout()
        .map_err(|error| error.to_string())?
        .move_to(
            Target::Family(&target.layout().map_err(|error| error.to_string())?),
            (0.0, 0.0),
            (1.0, 0.0),
        )
        .map_err(|error| error.to_string())?;
    family.shift(0.0, 0.2).map_err(|error| error.to_string())?;
    for object in [&first, &second, &anchor] {
        scene.add(object).map_err(|error| error.to_string())?;
    }
    scene.execution_session().map_err(|e| e.to_string())
}
