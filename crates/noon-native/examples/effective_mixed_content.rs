//! Native presentation of the paired mixed callback-content example.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (program, callbacks) = noon::example_scenes::effective_mixed_content::program()?;
    noon_native::run_live_program_with_callbacks(program, callbacks)?;
    Ok(())
}
