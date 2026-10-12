fn main() -> Result<(), Box<dyn std::error::Error>> {
    let session = noon::example_scenes::area_helpers::session().map_err(std::io::Error::other)?;
    noon_native::run(session)?;
    Ok(())
}
