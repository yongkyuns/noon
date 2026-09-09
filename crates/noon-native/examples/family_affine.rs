//! Shared alias-aware family scale/rotation with the ordinary native host.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run(noon::example_scenes::family_affine::session()?)?;
    Ok(())
}
