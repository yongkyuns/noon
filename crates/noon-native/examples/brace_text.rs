//! Equivalent to web/python/examples/ordinary_brace_text.py.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run(noon::example_scenes::brace_text::session()?)?;
    Ok(())
}
