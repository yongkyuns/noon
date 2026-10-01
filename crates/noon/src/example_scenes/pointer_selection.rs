//! Shared native/direct-WASM counterpart of the Python pointer selection fixture.
//! Selection is an explicit host/session policy, never an authored color mutation.
use crate::{AnimationOptions, ExecutionSession, Mobject, RateFunction, Scene, BLUE, GREEN};

pub fn scene() -> Result<Scene, String> {
    build(false).map(|(scene, _)| scene)
}

/// Same geometry with source-authored Rust click-to-Indicate declarations.
pub fn click_indicate_scene() -> Result<Scene, String> {
    build(true).map(|(scene, _)| scene)
}

/// A moving version of the same fixture for paired displayed-state selection.
pub fn moving_selection_session() -> Result<ExecutionSession, String> {
    moving_session(false)
}

/// Moving counterpart with source-authored click-to-Indicate declarations.
pub fn moving_click_indicate_session() -> Result<ExecutionSession, String> {
    moving_session(true)
}

fn moving_session(animated: bool) -> Result<ExecutionSession, String> {
    let (scene, circle) = build(animated)?;
    let mut target = circle.target_editor().map_err(|error| error.to_string())?;
    target.shift(1.8, 0.0).map_err(|error| error.to_string())?;
    let animation = scene.declare_transform_to(
        &circle,
        &target,
        AnimationOptions::new()
            .run_time(2.0)
            .rate_func(RateFunction::Linear),
    )?;
    let mut session = scene
        .execution_session()
        .map_err(|error| error.to_string())?;
    scene
        .live(&mut session)
        .play_animation(&animation)
        .map_err(|error| error.to_string())?;
    Ok(session)
}

fn build(animated: bool) -> Result<(Scene, Mobject), String> {
    let build = || -> Result<_, Box<dyn std::error::Error>> {
        let mut scene = Scene::new();
        let mut circle = scene.circle(0.9)?;
        circle.set_fill(BLUE.red.into(), BLUE.green.into(), BLUE.blue.into(), 1.0)?;
        circle.set_stroke_width(0.0)?;
        circle.set_scale(-1.3, 0.65)?;
        circle.set_rotation(0.45)?;
        circle.set_translation(-1.6, 0.2)?;
        let mut rectangle = scene.rectangle(1.7, 1.2)?;
        rectangle.set_fill(GREEN.red.into(), GREEN.green.into(), GREEN.blue.into(), 1.0)?;
        rectangle.set_stroke_width(0.0)?;
        rectangle.set_rotation(-0.5)?;
        rectangle.set_translation(1.5, -0.2)?;
        scene.add(&circle)?;
        scene.add(&rectangle)?;
        if animated {
            for shape in [&circle, &rectangle] {
                scene.on_click_indicate(
                    shape,
                    crate::IndicateOptions::default(),
                    crate::AnimationOptions::new().run_time(1.0),
                )?;
            }
        }
        #[cfg(all(feature = "native-text", feature = "bundled-fonts"))]
        {
            let label = scene.text(
                crate::Text::new(if animated {
                    "Click a filled shape; it restores automatically"
                } else {
                    "Click a filled shape; background clears"
                })
                .with_font_size(24.0)
                .shift(crate::Vec2::new(0.0, 2.8)),
            )?;
            scene.add_many(&[(&label).into()])?;
        }
        Ok((scene, circle))
    };
    build().map_err(|e| e.to_string())
}

pub fn session() -> Result<ExecutionSession, String> {
    scene()?.execution_session().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    #[test]
    fn moving_fixture_exposes_the_effective_midpoint_for_selection() {
        let mut session = super::moving_selection_session().unwrap();
        let start = session.frame().objects[0].transform.translation.x;
        session.seek(1.0).unwrap();
        let middle = session.frame().objects[0].transform.translation.x;
        assert!((middle - start - 0.9).abs() < 1e-5);
        session.seek(1.5).unwrap();
        let later = session.frame().objects[0].transform.translation.x;
        assert!((later - start - 1.35).abs() < 1e-5);
    }
}
