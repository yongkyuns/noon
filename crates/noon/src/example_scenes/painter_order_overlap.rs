//! Filled analytic and path geometry share one semantic painter order.
use crate::{
    AnimationOptions, ExecutionSession, RateFunction, Scene, SemanticPaint, SemanticStyle, Vec2,
    VectorPath, BLUE, GREEN, RED,
};

/// Paired with `web/python/examples/painter_order_overlap.py` on native and WASM.
pub fn session() -> Result<ExecutionSession, String> {
    let mut scene = Scene::new();
    let mut circle = scene.circle(1.25)?;
    let mut rectangle = scene.rectangle(2.1, 2.1)?;
    for (object, color) in [(&mut circle, RED), (&mut rectangle, BLUE)] {
        object.set_fill(
            f64::from(color.red),
            f64::from(color.green),
            f64::from(color.blue),
            1.0,
        )?;
        object.disable_stroke()?;
    }
    scene.add(&circle)?;
    scene.add(&rectangle)?;
    let path = scene.path(
        VectorPath::new()
            .move_to(Vec2::new(-0.8, -0.8))
            .line_to(Vec2::new(0.8, -0.8))
            .line_to(Vec2::new(0.8, 0.8))
            .line_to(Vec2::new(-0.8, 0.8))
            .close(),
        SemanticStyle {
            fill: Some(SemanticPaint::Solid(GREEN)),
            fill_opacity: 1.0,
            stroke: None,
            ..SemanticStyle::default()
        },
    )?;
    scene.add(&path)?;
    let mut target = rectangle.target_editor()?;
    target.rotate(std::f64::consts::FRAC_PI_2)?;
    let rotation = scene.declare_transform_to(
        &rectangle,
        &target,
        AnimationOptions::new()
            .run_time(1.0)
            .rate_func(RateFunction::Linear),
    )?;
    let mut session = scene.execution_session().map_err(|e| e.to_string())?;
    let mut live = scene.live(&mut session);
    let segment = live.play_animation(&rotation).map_err(|e| e.to_string())?;
    live.advance_segment_to(segment, segment.end_time())
        .map_err(|e| e.to_string())?;
    live.complete_segment(segment).map_err(|e| e.to_string())?;
    Ok(session)
}

#[cfg(test)]
mod tests {
    #[test]
    fn painter_order_and_rotation_survive_direct_seek() {
        let mut session = super::session().unwrap();
        session.seek(0.5).unwrap();
        let objects = &session.frame().objects;
        assert_eq!(objects.len(), 3);
        assert_eq!(objects[0].style.fill, Some(crate::RED));
        assert_eq!(objects[1].style.fill, Some(crate::BLUE));
        assert_eq!(objects[2].style.fill, Some(crate::GREEN));
        assert!((objects[1].transform.rotation - std::f32::consts::FRAC_PI_4).abs() < 1e-6);
    }
}
