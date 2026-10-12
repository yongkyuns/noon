//! Explicit native LaTeX assets shared by the runnable text examples.
use noon_native::{NativeLatexBackend, NativeLatexConfig};

pub fn backend() -> Result<NativeLatexBackend, Box<dyn std::error::Error>> {
    let font_directory = std::env::var_os("NOON_LATEX_FONT_DIR")
        .ok_or("set NOON_LATEX_FONT_DIR to a directory of explicit BaKoMa TTF assets")?;
    let resource_identity = std::env::var("NOON_LATEX_RESOURCE_IDENTITY")
        .map_err(|_| "set NOON_LATEX_RESOURCE_IDENTITY to identify those exact assets")?;
    let latex = std::env::var_os("NOON_LATEX_EXECUTABLE")
        .unwrap_or_else(|| "/Library/TeX/texbin/latex".into());
    let kpsewhich = std::env::var_os("NOON_KPSEWHICH_EXECUTABLE")
        .unwrap_or_else(|| "/Library/TeX/texbin/kpsewhich".into());
    Ok(NativeLatexBackend::new(NativeLatexConfig::new(
        latex,
        kpsewhich,
        font_directory,
        resource_identity,
    ))?)
}
