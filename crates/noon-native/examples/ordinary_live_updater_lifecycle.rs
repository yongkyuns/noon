fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (program, callbacks) = noon::example_scenes::live_updater_lifecycle::program()?;
    noon_native::run_live_program_with_callbacks(program, callbacks)?;
    Ok(())
}
