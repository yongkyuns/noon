//! Native presentation of the shared Rust/Python scalar continuation example.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let program = noon::example_scenes::ordinary_value_tracker_continuation_program()?;
    noon_native::run_live_program(program)?;
    Ok(())
}
