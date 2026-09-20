//! Shared native/direct-WASM counterpart of the Python pointer selection fixture.
//! Selection is an explicit host/session policy, never an authored color mutation.
use crate::{ExecutionSession, Scene, BLUE, GREEN};

pub fn scene() -> Result<Scene, String> {
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
        #[cfg(all(feature = "native-text", feature = "bundled-fonts"))]
        {
            let label = scene.text(
                crate::Text::new("Click a filled shape; background clears")
                    .with_font_size(24.0)
                    .shift(crate::Vec2::new(0.0, 2.8)),
            )?;
            scene.add_many(&[(&label).into()])?;
        }
        Ok(scene)
    };
    build().map_err(|e| e.to_string())
}

pub fn session() -> Result<ExecutionSession, String> {
    scene()?.execution_session().map_err(|e| e.to_string())
}
