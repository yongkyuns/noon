fn main() -> Result<(), Box<dyn std::error::Error>> {
    let session = noon::example_scenes::ordinary_paint_play()?;
    noon_native::run(session)?;
    Ok(())
}
