fn main() -> Result<(), Box<dyn std::error::Error>> {
    let session = noon::example_scenes::ordinary_style_play()?;
    noon_native::run(session)?;
    Ok(())
}
