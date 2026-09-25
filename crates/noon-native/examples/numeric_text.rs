//! `cargo run -p noon-native --features latex --example numeric_text`
//!
//! DecimalNumber compiles through the same pinned MathTex boundary as ordinary
//! LaTeX text. Persistent replacement holds the left edge fixed while updating
//! scene-owned numeric metadata and retained text content in one transaction.

use noon::{DecimalFormat, DecimalNumber, Scene};
use std::{path::Path, time::Duration};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web/latex/native-host.mjs");
    let mut backend = noon_native::NativeLatexBackend::start(
        Path::new("node"),
        &script,
        Duration::from_secs(45),
    )?;
    let scene = Scene::new();
    let mut number = DecimalNumber::new(
        std::rc::Rc::clone(scene.integration_store()),
        &mut backend,
        -0.004,
        DecimalFormat {
            include_sign: true,
            ..Default::default()
        },
    )?;
    let left = number.mobject().critical_point(-1.0, 0.0)?;
    number.set_value(&mut backend, 12_345.6)?;
    assert_eq!(number.text()?, "+12,345.60");
    assert_eq!(number.mobject().critical_point(-1.0, 0.0)?, left);
    // Replacing the retained text with the same semantic value must preserve
    // numeric metadata for subsequent getters and updates.
    number.set_value(&mut backend, 12_345.6)?;
    assert_eq!(number.value()?, 12_345.6);
    assert_eq!(number.text()?, "+12,345.60");
    Ok(())
}
