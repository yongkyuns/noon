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
    let first_anchor = crate::LayoutAnchor::from(&first);
    let target_anchor = crate::LayoutAnchor::from(&target);
    first_anchor
        .layout()?
        .move_to(Target::Anchor(&target_anchor), (0.0, 1.0), (1.0, 1.0))?;
    first_anchor
        .layout()?
        .align_to(Target::Anchor(&target_anchor), (0.0, -1.0))?;
    assert!(
        (first_anchor.layout()?.critical_point(0.0, -1.0).1
            - target.layout()?.critical_point(0.0, -1.0).1)
            .abs()
            < 1e-6
    );
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
    // Exercise shared frame placement, then restore the demonstration layout.
    let center = family.layout()?.center();
    family.layout()?.align_on_frame((2.0, 1.0), 0.25)?;
    let corner = family.layout()?.critical_point(1.0, 1.0);
    assert!((corner.0 - (f64::from(noon_core::DEFAULT_FRAME_WIDTH) * 0.5 - 0.5)).abs() < 1e-6);
    assert!((corner.1 - 3.75).abs() < 1e-6);
    family
        .layout()?
        .move_to(Target::Point(center.0, center.1), (0.0, 0.0), (1.0, 1.0))?;
    family.shift(0.0, 0.2)?;
    for object in [&first, &second, &anchor] {
        scene.add(object).map_err(|error| error.to_string())?;
    }
    let mut execution = scene.execution_session().map_err(|e| e.to_string())?;
    {
        let mut live = scene.live(&mut execution);
        let center = live
            .effective_family_layout(&family)
            .map_err(|e| e.to_string())?
            .center;
        live.align_family_on_frame(&family, (0.0, -1.0), 0.5)
            .map_err(|e| e.to_string())?;
        let layout = live
            .effective_family_layout(&family)
            .map_err(|e| e.to_string())?;
        assert!((layout.center.1 - layout.height * 0.5 + 3.5).abs() < 1e-6);
        live.move_family_to(
            &family,
            crate::LiveLayoutTarget::Point(center.0, center.1),
            (0.0, 0.0),
            (1.0, 1.0),
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(execution)
}
