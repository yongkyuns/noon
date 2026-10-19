//! Continuous periodic strokes: two ordinary lines and two normal tracks.
pub fn session() -> Result<crate::ExecutionSession, String> {
    let build = || -> Result<_, Box<dyn std::error::Error>> {
        let mut scene = crate::Scene::new();
        let mut dashes = scene.line((-3.0, 1.0), (3.0, 1.0))?;
        dashes.set_stroke_color(0.0, 1.0, 1.0, 1.0)?;
        dashes.set_stroke_width(0.12)?;
        dashes.set_stroke_pattern(Some(crate::StrokePattern::dashed(0.3, 0.2)?))?;
        scene.add(&dashes)?;
        let mut dots = scene.line((-3.0, -1.0), (3.0, -1.0))?;
        dots.set_stroke_color(1.0, 0.8, 0.0, 1.0)?;
        dots.set_stroke_width(0.14)?;
        dots.set_stroke_pattern(Some(crate::StrokePattern::dotted(0.5)?))?;
        scene.add(&dots)?;
        let forward = scene.flow_stroke(&dashes, 0.75, 4.0)?;
        let reverse = scene.flow_stroke(&dots, -0.5, 4.0)?;
        scene.wait(4.0)?;
        let root = scene.declare_animation(
            noon_core::SemanticAnimationIntent::Composition {
                kind: noon_core::SemanticAnimationCompositionKind::Parallel,
                children: vec![forward.node_id(), reverse.node_id()],
            },
            crate::AnimationOptions::new(),
        )?;
        Ok(scene.execution_session_with_animation_root(&root)?)
    };
    build().map_err(|error| error.to_string())
}
