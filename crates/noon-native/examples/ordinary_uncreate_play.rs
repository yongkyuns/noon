//! Native presentation of the shared detached Uncreate example.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run_live_program(noon::example_scenes::ordinary_uncreate_continuation_program()?)?;
    Ok(())
}
