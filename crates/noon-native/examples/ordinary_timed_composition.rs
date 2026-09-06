//! Native presentation of the paired nested Add/Wait composition example.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run_live_program(noon::example_scenes::timed_composition::program()?)?;
    Ok(())
}
