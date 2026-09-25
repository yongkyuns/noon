//! Native Rust LaTeX example using system TeX and explicit retained font assets.

#[cfg(feature = "latex")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use noon::{MathTex, Scene, Tex};
    use noon_native::{NativeLatexBackend, NativeLatexConfig};

    let font_directory = std::env::var_os("NOON_LATEX_FONT_DIR")
        .ok_or("set NOON_LATEX_FONT_DIR to a directory of explicit BaKoMa TTF assets")?;
    let resource_identity = std::env::var("NOON_LATEX_RESOURCE_IDENTITY")
        .map_err(|_| "set NOON_LATEX_RESOURCE_IDENTITY to identify those exact assets")?;
    let latex = std::env::var_os("NOON_LATEX_EXECUTABLE")
        .unwrap_or_else(|| "/Library/TeX/texbin/latex".into());
    let kpsewhich = std::env::var_os("NOON_KPSEWHICH_EXECUTABLE")
        .unwrap_or_else(|| "/Library/TeX/texbin/kpsewhich".into());
    let mut backend = NativeLatexBackend::new(NativeLatexConfig::new(
        latex,
        kpsewhich,
        font_directory,
        resource_identity,
    ))?;

    let mut scene = Scene::new();
    let mut title = scene.tex(Tex::new(r"System \LaTeX{} in Noon")?, &mut backend)?;
    title.shift(0.0, 1.0)?;
    scene.math_tex(MathTex::new(r"x^2+\frac{1}{2}")?, &mut backend)?;
    noon_native::run(scene.execution_session()?)?;
    Ok(())
}

#[cfg(not(feature = "latex"))]
fn main() {
    eprintln!("run this example with --features latex");
}
