//! Native presentation of the shared Rust/Python ordinary continuation example.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let program = noon::example_scenes::ordinary_affine_continuation_program()?;
    noon_native::run_live_program(program)?;
    Ok(())
}
