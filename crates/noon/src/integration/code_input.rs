//! Input validation shared by browser and native Python Code bindings.

use super::AuthoringFailure;
use crate::{
    CodeAuthoringError, CodeBackground, CodeOptions, CodeParagraphConfig, Color, NativeFontFace,
};

fn invalid_option(name: &'static str, value: f64) -> AuthoringFailure {
    CodeAuthoringError::InvalidOption {
        name,
        value: value.to_string(),
    }
    .into()
}

fn finite_f32(name: &'static str, value: f64) -> Result<f32, AuthoringFailure> {
    if !value.is_finite() || !(value as f32).is_finite() {
        return Err(invalid_option(name, value));
    }
    Ok(value as f32)
}

pub fn color(red: f64, green: f64, blue: f64, alpha: f64) -> Result<Color, AuthoringFailure> {
    for value in [red, green, blue, alpha] {
        if !value.is_finite() || !(0.0..=1.0).contains(&value) {
            return Err(invalid_option("color component", value));
        }
    }
    Ok(Color::rgba(
        red as f32,
        green as f32,
        blue as f32,
        alpha as f32,
    ))
}

pub fn options(
    language: Option<String>,
    source_path: Option<String>,
    formatter_style: String,
    tab_width: f64,
    add_line_numbers: bool,
    line_numbers_from: f64,
    background: &str,
) -> Result<CodeOptions, AuthoringFailure> {
    // Check the source numbers before converting to the engine's integer types.
    if !tab_width.is_finite()
        || tab_width.fract() != 0.0
        || !(0.0..=f64::from(u32::MAX)).contains(&tab_width)
    {
        return Err(invalid_option("tab_width", tab_width));
    }
    if !line_numbers_from.is_finite()
        || line_numbers_from.fract() != 0.0
        || !(f64::from(i32::MIN)..=f64::from(i32::MAX)).contains(&line_numbers_from)
    {
        return Err(invalid_option("line_numbers_from", line_numbers_from));
    }
    let background = match background {
        "rectangle" => CodeBackground::Rectangle,
        "window" => CodeBackground::Window,
        _ => {
            return Err(AuthoringFailure::new(
                "invalid_input",
                "code.invalid_background",
                "Code background must be 'rectangle' or 'window'",
            ))
        }
    };
    Ok(CodeOptions {
        language,
        source_path,
        formatter_style,
        tab_width: tab_width as usize,
        add_line_numbers,
        line_numbers_from: line_numbers_from as i32,
        background,
        ..Default::default()
    })
}

pub fn paragraph(
    font: String,
    font_size: f64,
    line_spacing: f64,
    disable_ligatures: bool,
    font_face: Option<NativeFontFace>,
) -> Result<CodeParagraphConfig, AuthoringFailure> {
    Ok(CodeParagraphConfig {
        font,
        font_size: finite_f32("font_size", font_size)?,
        line_spacing: finite_f32("line_spacing", line_spacing)?,
        disable_ligatures,
        font_face,
    })
}
