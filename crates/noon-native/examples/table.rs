//! Native counterpart of the retained plain/math/composite Table example.
#[cfg(feature = "latex")]
mod support;

#[cfg(feature = "latex")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut backend = support::latex::backend()?;
    noon_native::run(noon::example_scenes::table::session(&mut backend)?)
}

#[cfg(not(feature = "latex"))]
fn main() {
    eprintln!("run this example with --features latex");
}
