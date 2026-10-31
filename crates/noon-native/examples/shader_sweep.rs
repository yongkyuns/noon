fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args().any(|arg| arg == "--time") {
        noon_native::run_live_program(noon::example_scenes::shader_sweep::authored_time_program()?)?;
        return Ok(());
    }
    let program = if std::env::args().any(|arg| arg == "--late") {
        noon::example_scenes::shader_sweep::late_program()?
    } else {
        noon::example_scenes::shader_sweep::program()?
    };
    noon_native::run_live_program(program)?;
    Ok(())
}
