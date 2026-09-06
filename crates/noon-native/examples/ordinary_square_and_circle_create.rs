//! Native presentation of the shared Rust/Python parallel Create example.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let program = noon::example_scenes::ordinary_square_and_circle_create_continuation_program()?;
    noon_native::run_live_program(program)?;
    Ok(())
}
