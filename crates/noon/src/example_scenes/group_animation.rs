//! Two annotated steps move as one diagram through ordinary group authoring.
//! Paired with `ordinary_group_animation.py`, also available in the demo gallery.
use crate::{AnimationOptions, Color, LiveProgram, Scene, SceneScript, ScriptError, Vec2};

pub fn program() -> Result<LiveProgram<SceneScript>, ScriptError> {
    Scene::script(|scene| async move {
        let mut parts = Vec::new();
        for x in [-1.8, 1.8] {
            let card = scene.rectangle(2.0, 1.2)?;
            scene.shift(&card, Vec2::new(x, 0.0))?;
            scene.set_color(&card, Color::BLUE)?;
            scene.set_fill_opacity(&card, 0.0)?;
            parts.push(card);
        }
        // Flat ordinary lines: connector, arrow head, and the step labels 1 / 2.
        for (start, end) in [
            ((-0.6, 0.0), (0.6, 0.0)),
            ((0.35, 0.2), (0.6, 0.0)),
            ((0.35, -0.2), (0.6, 0.0)),
            ((-1.8, -0.25), (-1.8, 0.25)),
            ((1.6, 0.25), (2.0, 0.25)),
            ((2.0, 0.25), (2.0, 0.0)),
            ((2.0, 0.0), (1.6, -0.25)),
            ((1.6, -0.25), (2.0, -0.25)),
        ] {
            let line = scene.line(Vec2::new(start.0, start.1), Vec2::new(end.0, end.1))?;
            scene.set_color(&line, Color::BLUE)?;
            parts.push(line);
        }
        let diagram = scene.family(&parts)?;
        scene.add(&diagram)?;
        scene.wait(0.75).await?;
        let timing = AnimationOptions::new().run_time(1.25);
        scene
            .play_with(
                diagram.animate().shift(Vec2::new(0.0, 0.5)).scale(0.8),
                timing,
            )
            .await?;
        scene.wait(0.5).await?;
        scene
            .play_with(diagram.animate().rotate(-0.25), timing)
            .await?;
        scene.wait(0.5).await?;
        scene
            .play_with(
                diagram
                    .animate()
                    .rotate(0.25)
                    .scale(1.25)
                    .shift(Vec2::new(0.0, -0.5))
                    .set_color(Color::ORANGE),
                timing,
            )
            .await?;
        scene.wait(1.0).await?;
        Ok(())
    })
}
