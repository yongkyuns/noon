//! Direct counterpart of the paired Python MovingAround gallery example.
use crate::{Color, LiveProgram, Scene, SceneScript, ScriptError, Vec2};

pub fn program() -> Result<LiveProgram<SceneScript>, ScriptError> {
    Scene::script(|scene| async move {
        let square = scene.square(2.0)?;
        scene.set_color(&square, Color::BLUE)?;
        scene.set_fill_opacity(&square, 1.0)?;

        scene
            .play(square.animate().shift(Vec2::new(-1.0, 0.0)))
            .await?;
        scene.play(square.animate().set_fill(Color::ORANGE)).await?;
        scene.play(square.animate().scale(0.3)).await?;
        scene.play(square.animate().rotate(0.4)).await?;
        Ok(())
    })
}
