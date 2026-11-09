//! Two annotated steps move as one diagram through ordinary group authoring.
//! Paired with `showcase_group_animation.py`, also available in the demo gallery.
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
        // The progress marker stays independent of the diagram's shared pivot.
        let marker = scene.circle(0.16)?;
        scene.shift(&marker, Vec2::new(-2.4, -1.4))?;
        scene.set_color(&marker, Color::ORANGE)?;
        scene.set_fill_opacity(&marker, 1.0)?;
        scene.add(&marker)?;
        scene.wait(0.75).await?;
        let timing = AnimationOptions::new().run_time(1.25);
        scene
            .play_with(
                (
                    diagram.animate().shift(Vec2::new(0.0, 0.5)).scale(0.8),
                    marker.animate().shift(Vec2::new(1.6, 0.0)),
                ),
                timing,
            )
            .await?;
        scene.wait(0.5).await?;
        scene
            .play_with(
                (
                    diagram.animate().rotate(-0.25),
                    marker.animate().shift(Vec2::new(1.6, 0.0)),
                ),
                timing,
            )
            .await?;
        scene.wait(0.5).await?;
        scene
            .play_with(
                (
                    diagram
                        .animate()
                        .rotate(0.25)
                        .scale(1.25)
                        .shift(Vec2::new(0.0, -0.5))
                        .set_color(Color::ORANGE),
                    marker
                        .animate()
                        .shift(Vec2::new(1.6, 0.0))
                        .set_color(Color::BLUE),
                ),
                timing,
            )
            .await?;
        scene.wait(1.0).await?;
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LiveProgramStatus, RustHostCallbackTable};

    #[test]
    fn native_diagram_and_marker_complete_in_one_session() {
        let mut program = program().unwrap();
        let mut callbacks = RustHostCallbackTable::new();
        let identity = program.session().runtime_identity();
        for end in [0.75, 2.0, 2.5, 3.75, 4.25, 5.5, 6.5] {
            assert!(matches!(
                program.resume().unwrap(),
                LiveProgramStatus::Awaiting(_)
            ));
            if let LiveProgramStatus::PublicationPending(expected) =
                program.drive_to(&mut callbacks, end).unwrap()
            {
                let publication = program.take_renderer_publication().context();
                assert_eq!(expected, publication);
                program.admit_publication(publication).unwrap();
            }
            assert_eq!(program.session().runtime_identity(), identity);
            assert_eq!(program.session().frame().objects.len(), 11);
        }
        assert_eq!(program.resume().unwrap(), LiveProgramStatus::Finished);
        let marker = program.session().frame().objects.last().unwrap();
        assert!((marker.transform.translation.x - 2.4).abs() < 1e-5);
        assert!((marker.transform.translation.y + 1.4).abs() < 1e-5);
        assert_eq!(marker.style.fill, Some(Color::BLUE));
    }
}
