//! One ordinary retained scene replayed through a moving inset camera.
use crate::{
    AnimationOptions, ExecutionSession, ManimGeometryOptions, RateFunction, Scene,
    ZoomedSceneOptions, BLUE, YELLOW,
};

pub fn session() -> Result<ExecutionSession, String> {
    let build = || -> Result<_, Box<dyn std::error::Error>> {
        let mut scene = Scene::new();
        // ZoomedScene includes the ordinary invisible main-camera frame.
        scene.camera_frame()?;
        let mut focus_options = ManimGeometryOptions::circle(0.22)?;
        focus_options.set_color(
            YELLOW.red.into(),
            YELLOW.green.into(),
            YELLOW.blue.into(),
            1.0,
        )?;
        focus_options.set_fill(
            YELLOW.red.into(),
            YELLOW.green.into(),
            YELLOW.blue.into(),
            1.0,
        )?;
        focus_options.set_stroke_width(0.0)?;
        let mut focus = scene.geometry(focus_options)?;
        focus.shift(0.45, 0.15)?;

        let mut context_options = ManimGeometryOptions::circle(0.85)?;
        context_options.set_fill(BLUE.red.into(), BLUE.green.into(), BLUE.blue.into(), 0.18)?;
        context_options.set_stroke(BLUE.red.into(), BLUE.green.into(), BLUE.blue.into(), 1.0)?;
        context_options.set_stroke_width(0.035)?;
        let context = scene.geometry(context_options)?;
        scene.add_many(&[(&context).into(), (&focus).into()])?;

        let zoom = scene.zoomed_view(ZoomedSceneOptions::default())?;
        scene.activate_zooming(&zoom)?;
        let mut camera_target = zoom.camera_frame().target_editor()?;
        camera_target.shift(0.9, 0.3)?;
        let animation = scene.declare_transform_to(
            zoom.camera_frame(),
            &camera_target,
            AnimationOptions::new()
                .run_time(1.0)
                .rate_func(RateFunction::Linear),
        )?;
        let mut session = scene.execution_session()?;
        scene.live(&mut session).play_animation(&animation)?;
        Ok(session)
    };
    build().map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    #[test]
    fn shared_zoomed_scene_demo_uses_one_runtime() {
        let mut session = super::session().unwrap();
        assert_eq!(session.inset_2d_views().unwrap().len(), 1);
        session.seek(0.5).unwrap();
        let view = session.inset_2d_views().unwrap()[0];
        assert_eq!(view.camera.center, noon_core::Vec2::new(0.45, 0.15));
        assert!((view.zoom_factor() - 0.15).abs() < 1.0e-6);
    }
}
