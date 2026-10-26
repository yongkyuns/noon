//! Native Code source replacement through ordinary FadeOut and Write.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run_live_program(noon::example_scenes::code_source_update::program()?)?;
    Ok(())
}
