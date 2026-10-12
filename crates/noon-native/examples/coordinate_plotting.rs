//! Native host for the same animated plotting program used by direct Rust/WASM.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run_live_program(noon::coordinate_plotting_example::program()?)?;
    Ok(())
}
