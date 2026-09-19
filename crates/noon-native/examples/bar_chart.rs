//! Native host for the shared BarChart gallery scene.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run(noon::example_scenes::bar_chart::session()?)?;
    Ok(())
}
