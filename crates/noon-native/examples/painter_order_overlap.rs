//! Native counterpart of the shared Python/WebGPU painter-order proof.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run(noon::example_scenes::painter_order_overlap::session()?)?;
    Ok(())
}
