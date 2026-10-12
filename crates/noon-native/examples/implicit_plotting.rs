fn main() -> Result<(), Box<dyn std::error::Error>> {
    let session =
        noon::example_scenes::implicit_plotting::session().map_err(std::io::Error::other)?;
    noon_native::run(session)?;
    Ok(())
}
