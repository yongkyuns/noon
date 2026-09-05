//! Typed native execution of one shared semantic scene containing geometry and text.

use noon::{Scene, Text};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::new();

    let mut circle = scene.circle(0.5)?;
    circle.shift(-2.0, 0.0)?;
    scene.add(&circle)?;

    let mut label = scene.text(Text::new("Noon").with_font_size(48.0))?;
    label.shift(1.0, 0.0)?;
    scene.add(&label)?;

    let session = scene.execution_session()?;
    noon_native::run(session)?;
    Ok(())
}
