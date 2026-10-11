//! Persistent glow through public Scene/LiveSession; paired with the Python example.
//! Runs without a Python/browser host. The native raster test qualifies its shared renderer path.
use noon::{
    effects::{EffectDefinition, GlowUpdate, Pixels},
    AnimationCompositionRequest, AnimationOptions, Color, MathTypst, RateFunction, Scene,
    SemanticAnimationCompositionKind, SemanticPaint, SemanticStyle, TransformToRequest, Vec2,
    VectorPath,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::new();
    let mut dot = scene.circle(0.4)?;
    dot.disable_stroke()?;
    dot.set_fill(1.0, 1.0, 1.0, 1.0)?;
    dot.set_glow(
        GlowUpdate::default()
            .color(Color::RED)
            .radius(Pixels(3.25))
            .intensity(0.4000000000000123),
    )?;
    scene.add(&dot)?;
    let original = dot.get_effect("glow")?;
    let style = SemanticStyle {
        fill: None,
        stroke: Some(SemanticPaint::Solid(Color::BLUE)),
        stroke_width: 0.05,
        stroke_width_mode: noon::StrokeWidthMode::ScreenSpace,
        ..SemanticStyle::default()
    };
    let mut path = scene.path(
        VectorPath::new().move_to(Vec2::new(-2.5, -0.7)).cubic_to(
            Vec2::new(-1.5, 1.2),
            Vec2::new(1.5, -1.2),
            Vec2::new(2.5, 0.7),
        ),
        style,
    )?;
    let mut formula = scene
        .math_typst(MathTypst::new("integral_0^1 x^2 dif x = frac(1, 3)").with_font_size(34.0))?;
    formula.shift(0.0, -0.2)?;
    path.set_glow(
        GlowUpdate::default()
            .color(Color::BLUE)
            .radius(Pixels(7.0))
            .intensity(1.2),
    )?;
    formula.set_glow(
        GlowUpdate::default()
            .color(Color::RED)
            .radius(Pixels(6.0))
            .intensity(0.9),
    )?;
    let mut session = scene.execution_session()?;
    let (composition, path_glow, formula_glow) = {
        let mut live = scene.live(&mut session);
        let target = live.target_editor(&dot)?;
        live.shift(&target, 2.0, 0.0)?;
        live.set_glow(
            &target,
            GlowUpdate::default()
                .color(Color::BLUE)
                .radius(Pixels(6.5))
                .intensity(1.4),
        )?;
        let segment = live.declare_and_activate_transform_to(
            &dot,
            &target,
            AnimationOptions::new()
                .run_time(1.0)
                .rate_func(RateFunction::Linear),
        )?;
        live.advance_segment_to(segment, 1.0)?;
        live.complete_segment(segment)?;
        let EffectDefinition::Glow(complete) = original.authored_definition()?;
        assert_eq!(complete.intensity(), 1.4);
        assert_eq!(complete.radius(), Pixels(6.5).into());
        assert_eq!(live.effective_layout(&dot)?.center.0, 2.0);

        let path_target = live.target_editor(&path)?;
        live.set_glow(
            &path_target,
            GlowUpdate::default().intensity(1.8).radius(Pixels(10.0)),
        )?;
        let formula_target = live.target_editor(&formula)?;
        live.set_glow(
            &formula_target,
            GlowUpdate::default().intensity(1.4).color(Color::BLUE),
        )?;
        let formula_glow = formula.get_effect("glow")?;
        let reveal = AnimationOptions::new()
            .run_time(1.5)
            .rate_func(RateFunction::Linear);
        let composition = live.declare_and_activate_animation_composition(
            SemanticAnimationCompositionKind::Parallel,
            &[
                AnimationCompositionRequest::Create {
                    target: &path,
                    options: reveal,
                },
                AnimationCompositionRequest::TextWrite {
                    target: &formula,
                    reverse_member_order: false,
                    options: reveal,
                },
                AnimationCompositionRequest::TransformTo(TransformToRequest::new(
                    &path,
                    &path_target,
                    reveal,
                )),
                AnimationCompositionRequest::TransformTo(TransformToRequest::new(
                    &formula,
                    &formula_target,
                    reveal,
                )),
            ],
            AnimationOptions::new(),
            reveal,
        )?;
        let path_glow = path.get_effect("glow")?;
        live.advance_segment_to(composition, 1.75)?;
        (composition, path_glow, formula_glow)
    };
    let midpoint = session
        .frame()
        .objects
        .iter()
        .filter_map(|object| object.glow.as_deref())
        .find(|glow| glow.attachment == path_glow.node_id())
        .expect("effective path glow at the midpoint");
    assert!((midpoint.definition.intensity() - 1.5).abs() < 1e-9);
    let mut live = scene.live(&mut session);
    live.advance_segment_to(composition, 2.5)?;
    live.complete_segment(composition)?;
    let EffectDefinition::Glow(final_glow) = path_glow.authored_definition()?;
    assert_eq!(final_glow.intensity(), 1.8);
    assert_eq!(final_glow.radius(), Pixels(10.0).into());
    let EffectDefinition::Glow(final_formula_glow) = formula_glow.authored_definition()?;
    assert_eq!(final_formula_glow.intensity(), 1.4);
    assert_eq!(final_formula_glow.radius(), Pixels(6.0).into());

    let neutral = live.target_editor(&dot)?;
    live.set_glow(&neutral, GlowUpdate::default().intensity(0.0))?;
    let fade = live.declare_and_activate_transform_to(
        &dot,
        &neutral,
        AnimationOptions::new()
            .run_time(0.5)
            .rate_func(RateFunction::Linear),
    )?;
    live.advance_segment_to(fade, 3.0)?;
    live.complete_segment(fade)?;
    let EffectDefinition::Glow(complete) = original.authored_definition()?;
    assert_eq!(complete.intensity(), 0.0);
    live.remove_glow(&dot)?;
    assert!(original.authored_definition().is_err());
    let wait = live.wait_segment(0.25)?;
    live.advance_segment_to(wait, 3.25)?;
    live.complete_segment(wait)?;
    assert_eq!(live.effective_layout(&dot)?.center.0, 2.0);
    println!("Rust public glow motion/completion/removal passed at t=3.25");
    Ok(())
}
