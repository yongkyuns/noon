fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run(noon::example_scenes::paint_queries_gradients::session()?)?;
    Ok(())
}
