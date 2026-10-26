//! Native presentation of logical source-row highlights.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run(noon::example_scenes::code_line_highlight::session(true)?)?;
    Ok(())
}
