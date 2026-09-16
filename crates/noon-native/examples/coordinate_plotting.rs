//! Native host for the same typed plotting scene used by direct Rust/WASM.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run(noon::coordinate_plotting_example::session()?)?;
    Ok(())
}
