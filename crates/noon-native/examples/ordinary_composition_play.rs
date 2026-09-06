use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let session = noon::example_scenes::ordinary_composition_play()?;
    noon_native::run(session)?;
    Ok(())
}
