//! Per-member RGB interpolation is authoring behavior, not renderer gradient paint.
use crate::{semantic_mobject::edit_color, AuthoringError, Color, Mobject, MobjectFamily};
use noon_core::{SemanticMutationTransaction, SemanticStyle};

fn validate_colors(colors: &[Color]) -> Result<(), AuthoringError> {
    if colors.is_empty() {
        return Err(AuthoringError::EmptyColorGradient);
    }
    for color in colors {
        edit_color(
            &mut SemanticStyle::default(),
            color.red.into(),
            color.green.into(),
            color.blue.into(),
            color.alpha.into(),
        )?;
    }
    Ok(())
}

fn gradient_color(colors: &[Color], index: usize, count: usize) -> Color {
    if count <= 1 || colors.len() == 1 {
        return colors[0];
    }
    let position = index as f64 * (colors.len() - 1) as f64 / (count - 1) as f64;
    let lower = (position.floor() as usize).min(colors.len() - 1);
    let upper = (lower + 1).min(colors.len() - 1);
    let fraction = position - lower as f64;
    let blend = |a: f32, b: f32| (f64::from(a) + (f64::from(b) - f64::from(a)) * fraction) as f32;
    let (a, b) = (colors[lower], colors[upper]);
    Color::rgba(
        blend(a.red, b.red),
        blend(a.green, b.green),
        blend(a.blue, b.blue),
        blend(a.alpha, b.alpha),
    )
}

/// Evenly sample reference colors, including both endpoints when count > 1.
/// RGB interpolation follows pinned Manim's color-gradient utility.
/// Empty reference colors reject even when the requested output is empty.
pub fn color_gradient(colors: &[Color], count: usize) -> Result<Vec<Color>, AuthoringError> {
    validate_colors(colors)?;
    Ok((0..count)
        .map(|index| gradient_color(colors, index, count))
        .collect())
}

impl Mobject {
    /// A single semantic leaf takes the first reference color.
    pub fn set_color_by_gradient(&mut self, colors: &[Color]) -> Result<(), AuthoringError> {
        validate_colors(colors)?;
        let color = colors[0];
        self.set_color(
            color.red.into(),
            color.green.into(),
            color.blue.into(),
            color.alpha.into(),
        )
    }
}

impl MobjectFamily {
    /// Color each unique semantic leaf in family order through one transaction.
    /// Geometry/resources and existing paint opacity remain unchanged.
    pub fn set_color_by_gradient(&self, colors: &[Color]) -> Result<(), AuthoringError> {
        self.gradient_transaction(colors)?
            .apply(&mut self.integration_store().borrow_mut())
            .map(|_| ())
            .map_err(AuthoringError::from)
    }

    pub(crate) fn gradient_transaction(
        &self,
        colors: &[Color],
    ) -> Result<SemanticMutationTransaction, AuthoringError> {
        validate_colors(colors)?;
        self.style_transaction_indexed(|index, count, style| {
            let color = gradient_color(colors, index, count);
            edit_color(
                style,
                color.red.into(),
                color.green.into(),
                color.blue.into(),
                color.alpha.into(),
            )
        })
    }
}
