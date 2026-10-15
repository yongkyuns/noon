//! Shared plain/markup Text constructor values for native and WASM hosts.
use super::AuthoringFailure;

pub fn native_text_options(
    source: &str,
    font_family: &str,
    font_size: f64,
    line_spacing: f64,
    font_face: Option<crate::NativeFontFace>,
    markup: bool,
) -> Result<crate::Text, AuthoringFailure> {
    let font_size = super::authoring_render_f64("font size", font_size)? as f32;
    let line_spacing = super::authoring_render_f64("line spacing", line_spacing)? as f32;
    if line_spacing < -1.0 {
        return Err("line spacing must be -1 or greater than -1".into());
    }
    let text = if markup {
        crate::MarkupText::new(source).into()
    } else {
        crate::Text::new(source)
    };
    let text = match font_face {
        Some(face) => text.with_font_face(face),
        None => text.with_font(font_family),
    };
    Ok(text
        .with_font_size(font_size)
        .with_line_spacing(line_spacing))
}

pub fn text_color_channel(value: f64) -> Result<f32, AuthoringFailure> {
    if value.is_finite() && (0.0..=1.0).contains(&value) {
        Ok(value as f32)
    } else {
        Err(AuthoringFailure::new(
            "invalid_input",
            "text.invalid_color",
            "text color channels must be finite and between zero and one",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_nonfinite_or_narrowing_overflow_before_text_admission() {
        for value in [f64::NAN, f64::INFINITY, f64::MAX] {
            assert!(
                native_text_options("x", "DejaVu Sans Mono", value, -1.0, None, false).is_err()
            );
            assert!(native_text_options("x", "DejaVu Sans Mono", 48.0, value, None, true).is_err());
        }
        assert!(native_text_options("x", "DejaVu Sans Mono", 48.0, -2.0, None, false).is_err());
        assert!(native_text_options("x", "DejaVu Sans Mono", 48.0, -1.0, None, false).is_ok());
    }
    #[test]
    fn color_channel_rejects_before_any_batch_mutation() {
        for value in [f64::NAN, f64::INFINITY, -0.1, 1.1] {
            assert!(text_color_channel(value).is_err());
        }
        assert_eq!(text_color_channel(0.5).unwrap(), 0.5);
    }
}
