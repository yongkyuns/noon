//! Native presentation of the shared Rust/Python ordinary fade lifecycle example.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let program = noon::example_scenes::ordinary_fade_continuation_program()?;
    noon_native::run_live_program(program)?;
    Ok(())
}
