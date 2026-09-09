//! Family paint uses the same style edits and atomic semantic transaction as leaves.
use crate::AuthoringError;
use crate::{
    semantic_mobject::{
        edit_color, edit_disable_fill, edit_disable_stroke, edit_fill_color, edit_fill_opacity,
        edit_manim_opacity, edit_stroke_color, edit_stroke_opacity, edit_stroke_width,
    },
    Color, MobjectFamily,
};
use noon_core::{SemanticMutationTransaction, SemanticStyle};

pub(crate) fn fill(
    style: &mut SemanticStyle,
    color: Option<Color>,
    opacity: Option<f64>,
) -> Result<(), AuthoringError> {
    if let Some(c) = color {
        edit_fill_color(
            style,
            c.red.into(),
            c.green.into(),
            c.blue.into(),
            c.alpha.into(),
        )?;
    } else if opacity.is_none() {
        edit_disable_fill(style);
    }
    if let Some(opacity) = opacity {
        edit_fill_opacity(style, opacity)?;
    }
    Ok(())
}

pub(crate) fn stroke(
    style: &mut SemanticStyle,
    color: Option<Color>,
    width: Option<f64>,
    opacity: Option<f64>,
) -> Result<(), AuthoringError> {
    if let Some(c) = color {
        edit_stroke_color(
            style,
            c.red.into(),
            c.green.into(),
            c.blue.into(),
            c.alpha.into(),
        )?;
    } else if width.is_none() && opacity.is_none() {
        edit_disable_stroke(style);
    }
    if let Some(width) = width {
        edit_stroke_width(style, width)?;
    }
    if let Some(opacity) = opacity {
        edit_stroke_opacity(style, opacity)?;
    }
    Ok(())
}

impl MobjectFamily {
    /// Recolor each unique leaf's enabled paint channels, preserving their opacity.
    pub fn set_color(
        &self,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
    ) -> Result<(), AuthoringError> {
        self.edit_style(|style| edit_color(style, red, green, blue, alpha))
    }

    /// Change fill color/opacity atomically. With neither argument, disable fill.
    pub fn set_fill(
        &self,
        color: Option<Color>,
        opacity: Option<f64>,
    ) -> Result<(), AuthoringError> {
        self.edit_style(|style| fill(style, color, opacity))
    }

    /// Change stroke in one transaction. With no arguments, disable stroke.
    /// Width is in scene units, as for `Mobject::set_stroke_width`.
    pub fn set_stroke(
        &self,
        color: Option<Color>,
        width: Option<f64>,
        opacity: Option<f64>,
    ) -> Result<(), AuthoringError> {
        self.edit_style(|style| stroke(style, color, width, opacity))
    }

    /// Set enabled fill/stroke opacity, preserving the object-composite multiplier.
    pub fn set_opacity(&self, opacity: f64) -> Result<(), AuthoringError> {
        self.edit_style(|style| edit_manim_opacity(style, opacity))
    }

    fn edit_style(
        &self,
        edit: impl Fn(&mut SemanticStyle) -> Result<(), AuthoringError>,
    ) -> Result<(), AuthoringError> {
        self.style_transaction(edit)?
            .apply(&mut self.integration_store().borrow_mut())
            .map(|_| ())
            .map_err(AuthoringError::from)
    }

    pub(crate) fn style_transaction(
        &self,
        edit: impl Fn(&mut SemanticStyle) -> Result<(), AuthoringError>,
    ) -> Result<SemanticMutationTransaction, AuthoringError> {
        self.validate()?;
        // Validate arguments even for an empty family, before staging any writes.
        edit(&mut SemanticStyle::default())?;
        let store = self.integration_store().borrow();
        let leaves = store
            .ordered_leaf_nodes(self.node_id())
            .map_err(AuthoringError::from)?;
        let mut transaction = SemanticMutationTransaction::new();
        for leaf in leaves {
            let previous = &store
                .semantic_object_state_checked(leaf)
                .map_err(AuthoringError::from)?
                .style;
            let mut next = previous.clone();
            edit(&mut next)?;
            if next != *previous {
                transaction.replace_style(leaf, next);
            }
        }
        Ok(transaction)
    }
}
