//! Native presentation of the paired ordinary composition continuation example.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let program = noon::example_scenes::ordinary_composition_continuation_program()?;
    noon_native::run_live_program(program)?;
    Ok(())
}
