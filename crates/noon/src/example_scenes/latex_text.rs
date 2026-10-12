//! The same real-LaTeX scene for native, direct WASM, and Python authoring.
use crate::{ExecutionSession, LatexBackend, MathTex, Scene, Tex, Vec2, BLUE, YELLOW};

pub fn session(backend: &mut impl LatexBackend) -> Result<ExecutionSession, String> {
    scene(backend).and_then(|scene| scene.execution_session().map_err(|error| error.to_string()))
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
