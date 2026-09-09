//! Shared typed path-morph stress workload; optional first argument is object count.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let count = std::env::args()
        .nth(1)
        .map(|value| value.parse())
        .transpose()?
        .unwrap_or(1000);
    noon_native::run(noon::example_scenes::renderer_fixtures::morph_stress(
        count,
    )?)?;
    Ok(())
}
