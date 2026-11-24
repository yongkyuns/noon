//! Native counterpart of the Python sketch text demo.
use crate::{
    AnimationCompositionRequest as Animation, AnimationOptions, ContinuationEffects,
    ContinuationScene, ContinuationStep, LatexBackend, LiveContinuation, LiveProgram, MathTex,
    Mobject, MobjectFamily, RateFunction, Scene, ScriptError, StyleUpdate, Text, Vec2, BLUE_E,
    TEAL, YELLOW,
};

pub struct SketchText {
    title: Mobject,
    equation: MobjectFamily,
    step: u8,
}

impl LiveContinuation for SketchText {
    const EFFECTS: ContinuationEffects = ContinuationEffects::SceneOnly;
    type Error = ScriptError;

    fn resume(&mut self, scene: &mut ContinuationScene) -> Result<ContinuationStep, ScriptError> {
        let options = AnimationOptions::new().rate_func(RateFunction::Linear);
        let segment = match self.step {
            0 => scene.live().declare_and_activate_composition(
                &Animation::Composition {
                    kind: crate::SemanticAnimationCompositionKind::Parallel,
                    children: vec![
                        Animation::TextReveal {
                            target: &self.title,
                            reverse: false,
                            options,
                        },
                        Animation::FamilyTextWrite {
                            target: &self.equation,
                            reverse_member_order: false,
                            options: options.lag_ratio(0.12),
                        },
                    ],
                    options,
                },
                options.run_time(2.4),
            )?,
            1 => scene.live().wait_segment(0.6)?,
            _ => return Ok(ContinuationStep::Finished),
        };
        self.step += 1;
        Ok(ContinuationStep::Await(segment))
    }
}

pub fn program(backend: &mut impl LatexBackend) -> Result<LiveProgram<SketchText>, ScriptError> {
    let mut scene = Scene::new();
    let title = scene.text(Text::new("Noon").with_font_size(120.0).color(BLUE_E))?;
    scene.shift(&title, Vec2::new(0.0, 1.35))?;
    scene.set_style(
        &title,
        StyleUpdate::sketch_stroke(TEAL, Some(0.035), None, 1.1, 7)?,
    )?;
    let equation = scene.math_tex_parts(
        MathTex::new(r"\frac{a^2+b^2}{2}")?
            .with_font_size(110.0)
            .color(BLUE_E)
            .move_to(Vec2::new(0.0, -1.10)),
        backend,
    )?;
    scene.set_family_style(
        equation.family(),
        StyleUpdate::sketch_stroke(YELLOW, Some(0.028), None, 1.1, 12)?,
    )?;
    Ok(scene.into_live_program(SketchText {
        title,
        equation: equation.family().clone(),
        step: 0,
    })?)
}
