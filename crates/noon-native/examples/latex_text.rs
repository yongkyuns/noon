//! Native Rust LaTeX example using system TeX and explicit retained font assets.
#[cfg(feature = "latex")]
#[path = "support/latex.rs"]
mod latex;

#[cfg(feature = "latex")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut backend = latex::backend()?;
    noon_native::run(noon::example_scenes::latex_text::session(&mut backend)?)?;
    Ok(())
}

#[cfg(not(feature = "latex"))]
fn main() {
    eprintln!("run this example with --features latex");
}
