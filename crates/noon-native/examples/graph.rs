//! Native host for explicit retained Graph/DiGraph construction.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run(noon::example_scenes::graph::session()?)?;
    Ok(())
}
