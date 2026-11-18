//! The same real-LaTeX scene for native, direct WASM, and Python authoring.
use crate::{LatexBackend, MathTex, Scene, Tex, Vec2, BLUE, YELLOW};

pub fn session(backend: &mut impl LatexBackend) -> Result<crate::SceneExecution, String> {
    scene(backend).and_then(|scene| scene.into_execution().map_err(|error| error.to_string()))
}

/// Paired with `showcase_equation_card.py` in the demo gallery.
#[cfg(all(feature = "native-text", feature = "bundled-fonts"))]
pub fn group_program(
    backend: &mut impl LatexBackend,
) -> Result<crate::LiveProgram<crate::SceneScript>, crate::ScriptError> {
    use crate::{AnimationOptions, Text, UP};

    let mut scene = Scene::new();
    let caption = scene.text(Text::new("Highlight a term").with_font_size(32.0))?;
    scene.shift(&caption, 1.9 * UP)?;
    scene.add(&caption)?;

    let offset = -0.6 * UP;
    let equation = scene.math_tex_parts(
        MathTex::from_strings(["x", "+", "1", "=", "4"])?
            .with_font_size(58.0)
            .shift(offset),
        backend,
    )?;
    let term = equation.members()[0].clone();
    let frame = scene.rectangle(3.8, 1.3)?;
    scene.shift(&frame, offset)?;
    scene.set_color(&frame, BLUE)?;
    scene.set_fill_opacity(&frame, 0.0)?;
    let card = scene.family(&[(&frame).into(), equation.family().into()])?;

    scene.into_script(move |scene| async move {
        scene.wait(0.5).await?;
        scene
            .play_with(
                card.animate().shift(0.85 * UP),
                AnimationOptions::new().run_time(0.75),
            )
            .await?;
        scene.wait(0.25).await?;
        scene
            .play_with(
                term.animate().set_color(YELLOW),
                AnimationOptions::new().run_time(0.5),
            )
            .await?;
        scene.wait(0.75).await?;
        Ok(())
    })
}

fn scene(backend: &mut impl LatexBackend) -> Result<Scene, String> {
    let mut scene = Scene::new();
    let title = scene
        .tex(
            Tex::new(r"Real \LaTeX{} in Noon")
                .map_err(|error| error.to_string())?
                .with_font_size(38.0)
                .move_to(Vec2::new(0.0, 2.0)),
            backend,
        )
        .map_err(|error| error.to_string())?;
    let fraction = scene
        .math_tex(
            MathTex::new(r"x^2+\frac{1}{2}")
                .map_err(|error| error.to_string())?
                .with_font_size(64.0)
                .color(BLUE)
                .move_to(Vec2::new(0.0, 0.5)),
            backend,
        )
        .map_err(|error| error.to_string())?;
    let equation = scene
        .math_tex(
            MathTex::new(r"\alpha+\Gamma+\sum_{i=1}^{3}i+\sqrt{2}")
                .map_err(|error| error.to_string())?
                .with_font_size(48.0)
                .color(YELLOW)
                .move_to(Vec2::new(0.0, -1.3)),
            backend,
        )
        .map_err(|error| error.to_string())?;
    scene
        .add_many(&[(&title).into(), (&fraction).into(), (&equation).into()])
        .map_err(|error| error.to_string())?;
    Ok(scene)
}
