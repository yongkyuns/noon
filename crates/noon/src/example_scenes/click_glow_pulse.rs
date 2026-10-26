//! Click either filled shape to pulse its glow while authored time stays still.
//! Paired with `web/python/examples/click_glow_pulse.py`.

use crate::{
    effects::{GlowUpdate, Pixels},
    AnimationOptions, Color, Scene, SceneExecution,
};

pub fn execution() -> Result<SceneExecution, String> {
    let mut scene = Scene::new();
    let mut persistent = scene.circle(0.55).map_err(message)?;
    persistent.set_fill(1.0, 1.0, 1.0, 1.0).map_err(message)?;
    persistent.disable_stroke().map_err(message)?;
    persistent.set_translation(-2.0, 0.0).map_err(message)?;
    persistent
        .set_glow(
            GlowUpdate::default()
                .color(Color::BLUE)
                .radius(Pixels(12.0))
                .intensity(0.35),
        )
        .map_err(message)?;

    let mut temporary = scene.rectangle(1.1, 1.1).map_err(message)?;
    temporary.set_fill(1.0, 1.0, 1.0, 1.0).map_err(message)?;
    temporary.disable_stroke().map_err(message)?;
    temporary.set_translation(2.0, 0.0).map_err(message)?;
    scene.add(&persistent).map_err(message)?;
    scene.add(&temporary).map_err(message)?;

    for target in [&persistent, &temporary] {
        let pulse = scene
            .declare_glow_pulse(target, 2.0, AnimationOptions::new().run_time(1.0))
            .map_err(message)?;
        scene.on_click(target, Some(&pulse)).map_err(message)?;
    }
    // The right shape has no effect until an admitted click. Its temporary
    // attachment retires at pulse completion; the left keeps its original glow.
    scene.into_execution().map_err(message)
}

fn message(error: impl std::fmt::Display) -> String {
    error.to_string()
}
