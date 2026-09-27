//! Shared Rust Variable composition through the native renderer.
#[cfg(feature = "latex")]
mod support;

#[cfg(feature = "latex")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut backend = support::latex::backend()?;
    noon_native::run(noon::example_scenes::variable::session(&mut backend)?)?;
    Ok(())
}

#[cfg(not(feature = "latex"))]
fn main() {
    eprintln!("run this example with --features latex");
}
