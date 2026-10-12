//! Native host for duplicate matching keys and default unmatched fades.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run_live_program(
        noon::example_scenes::family_transform_indicate::matching_shapes::breadth_program()?,
    )?;
    Ok(())
}
