#[cfg(feature = "latex")]
mod support;

#[cfg(feature = "latex")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut backend = support::latex::backend()?;
    noon_native::run_live_program(noon::example_scenes::sketch_text::program(&mut backend)?)?;
    Ok(())
}

#[cfg(not(feature = "latex"))]
fn main() {
    eprintln!("run this example with --features latex");
}
