fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run(noon::example_scenes::canonical_curve_layout::session()?)?;
    Ok(())
}
