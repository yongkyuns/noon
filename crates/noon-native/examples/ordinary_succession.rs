//! Native presentation of the shared Rust/Python Succession tutorial.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run_live_program(noon::example_scenes::ordinary_succession_program()?)?;
    Ok(())
}
