//! A nested diagram and progress rail animate as independent groups in one play.
//! Paired with `showcase_group_animation.py`, also available in the demo gallery.
use crate::{AnimationOptions, Color, LiveProgram, Scene, SceneScript, ScriptError, Vec2};

pub fn program() -> Result<LiveProgram<SceneScript>, ScriptError> {
    Scene::script(|scene| async move {
        let lines = |segments: &[([f32; 2], [f32; 2])]| -> Result<Vec<_>, ScriptError> {
            segments
                .iter()
                .map(|&([x1, y1], [x2, y2])| {
                    let line = scene.line(Vec2::new(x1, y1), Vec2::new(x2, y2))?;
                    scene.set_color(&line, Color::BLUE)?;
                    Ok(line)
                })
                .collect()
        };
        let step = |x: f32, label: &[([f32; 2], [f32; 2])]| {
            let card = scene.rectangle(2.0, 1.2)?;
            scene.shift(&card, Vec2::new(x, 0.0))?;
            scene.set_color(&card, Color::BLUE)?;
            scene.set_fill_opacity(&card, 0.0)?;
            scene.family(
                std::iter::once(card)
                    .chain(lines(label)?)
                    .collect::<Vec<_>>()
                    .iter(),
            )
        };
        let first = step(-1.8, &[([-1.8, -0.25], [-1.8, 0.25])])?;
        let second = step(
            1.8,
            &[
                ([1.6, 0.25], [2.0, 0.25]),
                ([2.0, 0.25], [2.0, 0.0]),
                ([2.0, 0.0], [1.6, -0.25]),
                ([1.6, -0.25], [2.0, -0.25]),
            ],
        )?;
        let connector = scene.family(&lines(&[
            ([-0.6, 0.0], [0.6, 0.0]),
            ([0.35, 0.2], [0.6, 0.0]),
            ([0.35, -0.2], [0.6, 0.0]),
        ])?)?;
        let diagram = scene.family([&first, &connector, &second])?;
        scene.add(&diagram)?;
        let mut rail_parts = Vec::new();
        rail_parts.push(scene.line(Vec2::new(-2.4, -1.4), Vec2::new(2.4, -1.4))?);
        for x in [-2.4, -0.8, 0.8, 2.4] {
            rail_parts.push(scene.line(Vec2::new(x, -1.6), Vec2::new(x, -1.2))?);
        }
        for part in &rail_parts {
            scene.set_color(part, Color::BLUE)?;
        }
        let rail = scene.family(&rail_parts)?;
        scene.add(&rail)?;
        // Each group has its own pivot; the marker advances between rail ticks.
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
                    diagram
                        .animate()
                        .shift(Vec2::new(0.0, 0.5))
                        .scale(0.8)
                        .with_options(AnimationOptions::new().lag_ratio(0.08)),
                    rail.animate().scale(0.8),
                    marker.animate().shift(Vec2::new(1.76, 0.0)),
                ),
                timing,
            )
            .await?;
        scene.wait(0.5).await?;
        scene
            .play_with(
                (
                    diagram.animate().rotate(-0.25),
                    rail.animate().shift(Vec2::new(0.0, -0.25)),
                    marker.animate().shift(Vec2::new(1.28, -0.25)),
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
                    rail.animate().scale(1.25).shift(Vec2::new(0.0, 0.25)),
                    marker
                        .animate()
                        .shift(Vec2::new(1.76, 0.25))
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
    fn native_diagram_rail_and_marker_complete_in_one_session() {
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
            assert_eq!(program.session().frame().objects.len(), 16);
        }
        assert_eq!(program.resume().unwrap(), LiveProgramStatus::Finished);
        let marker = program.session().frame().objects.last().unwrap();
        assert!((marker.transform.translation.x - 2.4).abs() < 1e-5);
        assert!((marker.transform.translation.y + 1.4).abs() < 1e-5);
        assert_eq!(marker.style.fill, Some(Color::BLUE));
    }
}
