//! Shared nested-family arrangement before and after a logical wait boundary.
use crate::{
    Color, FamilyArrangeOptions, LayoutAnchor, LayoutTarget, LiveProgram, ManimNextToArgs,
    MobjectTarget, Scene, SceneScript, ScriptError, Vec2,
};

pub fn program() -> Result<LiveProgram<SceneScript>, ScriptError> {
    Scene::script(|scene| async move {
        let first = scene.circle(0.2)?;
        let second = scene.circle(0.2)?;
        for (object, color) in [(&first, Color::BLUE), (&second, Color::YELLOW)] {
            scene.set_fill(object, color, 1.0)?;
            scene.set_stroke_width(object, 0.0)?;
        }
        scene.shift(&second, Vec2::new(2.0, 0.0))?;

        // One semantic leaf participates in both a direct and a nested member.
        let nested = scene.family([&first, &second])?;
        let family = scene.family([
            MobjectTarget::Object(&first),
            MobjectTarget::Family(&nested),
        ])?;
        let bounds = family.layout_bounds()?.expect("family bounds are empty");
        assert!((bounds.width() - 2.4).abs() < 1e-6);
        assert!((bounds.height() - 0.4).abs() < 1e-6);
        scene.arrange_family_with_options(
            &family,
            &FamilyArrangeOptions::new(1.0, 0.0, 0.2, true),
        )?;
        scene.add(&first)?;
        scene.add(&second)?;
        scene.wait(0.5).await?;

        let family = scene.family([
            MobjectTarget::Object(&first),
            MobjectTarget::Family(&nested),
        ])?;
        scene.remove_family_members(&family, [&nested])?;
        scene.add_family_members(&family, [&nested])?;
        let mut options = FamilyArrangeOptions::new(0.0, 1.0, 0.3, false);
        options.placement.aligned_edge = (-1.0, 0.0);
        scene.arrange_family_with_options(&family, &options)?;
        let layout = scene.effective_family_layout(&nested)?;
        assert!((layout.width - 2.4).abs() < 1e-5);
        assert!(layout.center.0.abs() < 1e-5);
        scene.move_family_to(
            &nested,
            LayoutTarget::Point(0.0, 0.0),
            (0.0, 0.0),
            (1.0, 0.0),
        )?;
        let source = LayoutAnchor::from(&nested);
        scene.next_layout_to_aligned(
            &source,
            LayoutTarget::Point(0.0, 0.0),
            &source.clone().member(-1),
            ManimNextToArgs {
                direction: (0.0, 2.0),
                buff: 0.25,
                aligned_edge: (0.0, 0.0),
                mask: (0.0, 1.0),
            },
        )?;
        scene.align_family_to(&nested, LayoutTarget::Mobject(&first), (0.0, 1.0))?;
        let first_anchor = LayoutAnchor::from(&first);
        scene.next_layout_to_aligned(
            &first_anchor,
            LayoutTarget::Mobject(&second),
            &first_anchor,
            ManimNextToArgs {
                direction: (-1.0, 0.0),
                buff: 0.25,
                aligned_edge: (0.0, 0.0),
                mask: (1.0, 1.0),
            },
        )?;
        let placed = scene.effective_layout(&first)?;
        assert!((placed.center.0 - 0.35).abs() < 1e-6);
        scene.wait(0.5).await?;
        Ok(())
    })
}
