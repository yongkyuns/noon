//! Native presentation of the shared highlighted-code scene.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run(noon::example_scenes::code_block::session()?)?;
    Ok(())
}
