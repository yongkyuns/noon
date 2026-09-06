//! Native presentation of the paired ordinary callback continuation example.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (program, callbacks) = noon::example_scenes::ordinary_callback_continuation_program()?;
    noon_native::run_live_program_with_callbacks(program, callbacks)?;
    Ok(())
}
