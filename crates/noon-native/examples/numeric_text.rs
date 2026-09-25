//! `cargo run -p noon-native --features latex --example numeric_text`
//!
//! DecimalNumber compiles through the same pinned MathTex boundary as ordinary
//! LaTeX text. Persistent replacement holds the left edge fixed while updating
//! scene-owned numeric metadata and retained text content in one transaction.

use std::{path::Path, time::Duration};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web/latex/native-host.mjs");
    let mut backend = noon_native::NativeLatexBackend::start(
        Path::new("node"),
        &script,
        Duration::from_secs(45),
    )?;
    let (scene, mut number) = noon::example_scenes::numeric_decimal::build(&mut backend)?;
    number.set_value(
        &mut backend,
        noon::example_scenes::numeric_decimal::DISPLAY_VALUE,
    )?;
    assert_eq!(
        number.value()?,
        noon::example_scenes::numeric_decimal::DISPLAY_VALUE
    );
    assert_eq!(number.text()?, "+12,345.60");
    let _session = scene.execution_session()?;
    Ok(())
}
