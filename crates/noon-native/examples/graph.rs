//! Native host for explicit retained Graph/DiGraph construction.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run_live_program(noon::example_scenes::graph::program()?)?;
    Ok(())
}
