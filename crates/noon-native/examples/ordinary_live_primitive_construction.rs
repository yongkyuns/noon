//! Native presentation of the shared Rust/Python live primitive construction example.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let program = noon::example_scenes::ordinary_live_primitive_construction_program()?;
    noon_native::run_live_program(program)?;
    Ok(())
}
