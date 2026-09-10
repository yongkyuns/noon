fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run(noon::example_scenes::family_membership_order::session()?)?;
    Ok(())
}
