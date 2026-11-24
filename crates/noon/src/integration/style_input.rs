//! Inert style arguments shared by the optional language adapters.
use super::family_color;

#[allow(clippy::too_many_arguments)]
pub fn style_update(
    fill_enabled: bool,
    fill_red: f64,
    fill_green: f64,
    fill_blue: f64,
    fill_alpha: f64,
    fill_opacity: Option<f64>,
    stroke_enabled: bool,
    stroke_red: f64,
    stroke_green: f64,
    stroke_blue: f64,
    stroke_alpha: f64,
    stroke_width: Option<f64>,
    stroke_opacity: Option<f64>,
) -> Result<crate::StyleUpdate, String> {
    Ok(crate::StyleUpdate {
        fill_color: family_color(fill_enabled, fill_red, fill_green, fill_blue, fill_alpha)?,
        fill_opacity,
        stroke_color: family_color(
            stroke_enabled,
            stroke_red,
            stroke_green,
            stroke_blue,
            stroke_alpha,
        )?,
        stroke_width,
        stroke_opacity,
        ..crate::StyleUpdate::default()
    })
}

pub fn style_update_with_background_stroke(
    mut update: crate::StyleUpdate,
    enabled: bool,
    rgba: [f64; 4],
    width: Option<f64>,
    opacity: Option<f64>,
) -> Result<crate::StyleUpdate, String> {
    let [red, green, blue, alpha] = rgba;
    update.background_stroke_color = family_color(enabled, red, green, blue, alpha)?;
    update.background_stroke_width = width;
    update.background_stroke_opacity = opacity;
    Ok(update)
}
