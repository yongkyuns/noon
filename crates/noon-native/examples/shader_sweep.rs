fn main() -> Result<(), Box<dyn std::error::Error>> {
    let program = if std::env::args().any(|arg| arg == "--late") {
        noon::example_scenes::shader_sweep::late_program()?
    } else {
        noon::example_scenes::shader_sweep::program()?
    };
    noon_native::run_live_program(program)?;
    Ok(())
}
