use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let program = noon::example_scenes::ordinary_create_then_content_morph_program()?;
    noon_native::run_live_program(program)?;
    Ok(())
}
