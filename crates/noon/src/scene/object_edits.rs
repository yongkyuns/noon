//! Ordinary persistent object edits through the Scene-owned publication route.

use crate::{
    family_authoring::FamilyTranslation,
    semantic_mobject::{
        edit_color, edit_disable_fill, edit_disable_stroke, edit_fade, edit_fill, edit_fill_color,
        edit_fill_opacity, edit_manim_opacity, edit_object_opacity, edit_stroke, edit_stroke_color,
        edit_stroke_opacity, edit_stroke_width, prepare_fill_paint_value,
    },
    AuthoringError, Mobject, Scene,
};
use noon_core::{
    Color, HatchPaint, LinearGradient, NoisePaint, RadialGradient, SemanticMutationTransaction,
    SemanticPaint, SemanticStyle, SemanticVec3, Vec2,
};
use std::sync::Arc;

impl Scene {
    /// Recolor the object's enabled fill and stroke while retaining layer opacity.
    ///
    /// This edits authored paint. Before execution starts, only the semantic
    /// scene changes; while running, the same transaction publishes coherently
    /// after the shared runtime edit barriers have been validated.
    pub fn set_color(&mut self, object: &Mobject, color: Color) -> Result<(), AuthoringError> {
        self.set_color_rgba(
            object,
            color.red.into(),
            color.green.into(),
            color.blue.into(),
            color.alpha.into(),
        )
    }

    /// Set persistent fill RGB and explicit layer opacity, preserving the stroke.
    ///
    /// `opacity` supplies the fill alpha independently of `color.alpha`.
    /// Effective values owned by other runtime drivers are not copied into the
    /// authored style. The shared publication path validates runtime edit barriers.
    pub fn set_fill(
        &mut self,
        object: &Mobject,
        color: Color,
        opacity: f64,
    ) -> Result<(), AuthoringError> {
        self.set_fill_rgb(
            object,
            color.red.into(),
            color.green.into(),
            color.blue.into(),
            opacity,
        )
    }

    /// Set persistent fill opacity without changing the fill color or stroke.
    pub fn set_fill_opacity(
        &mut self,
        object: &Mobject,
        opacity: f64,
    ) -> Result<(), AuthoringError> {
        self.edit_object_style(object, |style| edit_fill_opacity(style, opacity))
    }

    /// Set persistent stroke width through the shared paint and publication rules.
    pub fn set_stroke_width(&mut self, object: &Mobject, width: f64) -> Result<(), AuthoringError> {
        self.edit_object_style(object, |style| edit_stroke_width(style, width))
    }

    pub fn set_fill_rgb(
        &mut self,
        mobject: &Mobject,
        red: f64,
        green: f64,
        blue: f64,
        opacity: f64,
    ) -> Result<(), AuthoringError> {
        self.edit_object_style(mobject, |style| edit_fill(style, red, green, blue, opacity))
    }

    pub fn set_fill_color(
        &mut self,
        mobject: &Mobject,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
    ) -> Result<(), AuthoringError> {
        self.edit_object_style(mobject, |style| {
            edit_fill_color(style, red, green, blue, alpha)
        })
    }

    pub fn disable_fill(&mut self, mobject: &Mobject) -> Result<(), AuthoringError> {
        self.edit_object_style(mobject, |style| {
            edit_disable_fill(style);
            Ok(())
        })
    }

    pub fn set_color_rgba(
        &mut self,
        mobject: &Mobject,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
    ) -> Result<(), AuthoringError> {
        self.edit_object_style(mobject, |style| edit_color(style, red, green, blue, alpha))
    }

    pub fn set_stroke(
        &mut self,
        mobject: &Mobject,
        red: f64,
        green: f64,
        blue: f64,
        opacity: f64,
    ) -> Result<(), AuthoringError> {
        self.edit_object_style(mobject, |style| {
            edit_stroke(style, red, green, blue, opacity)
        })
    }

    pub fn set_stroke_color(
        &mut self,
        mobject: &Mobject,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
    ) -> Result<(), AuthoringError> {
        self.edit_object_style(mobject, |style| {
            edit_stroke_color(style, red, green, blue, alpha)
        })
    }

    pub fn disable_stroke(&mut self, mobject: &Mobject) -> Result<(), AuthoringError> {
        self.edit_object_style(mobject, |style| {
            edit_disable_stroke(style);
            Ok(())
        })
    }

    pub fn set_stroke_opacity(
        &mut self,
        mobject: &Mobject,
        opacity: f64,
    ) -> Result<(), AuthoringError> {
        self.edit_object_style(mobject, |style| edit_stroke_opacity(style, opacity))
    }

    pub fn set_stroke_cap(&mut self, mobject: &Mobject, cap: &str) -> Result<(), AuthoringError> {
        let cap = crate::semantic_mobject::parse_stroke_cap(cap)?;
        self.edit_object_style(mobject, |style| {
            style.stroke_cap = cap;
            Ok(())
        })
    }

    pub fn set_opacity(&mut self, mobject: &Mobject, opacity: f64) -> Result<(), AuthoringError> {
        self.edit_object_style(mobject, |style| edit_manim_opacity(style, opacity))
    }

    pub fn fade(&mut self, mobject: &Mobject, darkness: f64) -> Result<(), AuthoringError> {
        self.edit_object_style(mobject, |style| edit_fade(style, darkness))
    }

    pub fn set_emission(&mut self, mobject: &Mobject, strength: f64) -> Result<(), AuthoringError> {
        self.edit_object_style(mobject, |style| {
            style.emission =
                noon_core::Emission::new(strength).map_err(crate::AuthoringError::Emission)?;
            Ok(())
        })
    }

    pub fn set_object_opacity(
        &mut self,
        mobject: &Mobject,
        opacity: f64,
    ) -> Result<(), AuthoringError> {
        self.edit_object_style(mobject, |style| edit_object_opacity(style, opacity))
    }

    pub fn set_style(
        &mut self,
        object: &Mobject,
        update: crate::StyleUpdate,
    ) -> Result<(), AuthoringError> {
        self.edit_object_style(object, |style| update.apply(style))
    }

    /// Set an immutable linear fill and optional fill opacity.
    pub fn set_fill_gradient(
        &mut self,
        object: &Mobject,
        paint: impl Into<Arc<LinearGradient>>,
        opacity: Option<f64>,
    ) -> Result<(), AuthoringError> {
        self.set_fill_paint(object, SemanticPaint::LinearGradient(paint.into()), opacity)
    }

    /// Set an elliptical radial fill in object-local coordinates.
    pub fn set_fill_radial_gradient(
        &mut self,
        object: &Mobject,
        paint: impl Into<Arc<RadialGradient>>,
        opacity: Option<f64>,
    ) -> Result<(), AuthoringError> {
        self.set_fill_paint(object, SemanticPaint::RadialGradient(paint.into()), opacity)
    }

    /// Set stripes or crosshatching in object-local coordinates.
    pub fn set_fill_hatch(
        &mut self,
        object: &Mobject,
        paint: impl Into<Arc<HatchPaint>>,
        opacity: Option<f64>,
    ) -> Result<(), AuthoringError> {
        self.set_fill_paint(object, SemanticPaint::Hatch(paint.into()), opacity)
    }

    /// Set a seeded noise fill in object-local coordinates.
    pub fn set_fill_noise(
        &mut self,
        object: &Mobject,
        paint: impl Into<Arc<NoisePaint>>,
        opacity: Option<f64>,
    ) -> Result<(), AuthoringError> {
        self.set_fill_paint(object, SemanticPaint::Noise(paint.into()), opacity)
    }

    fn set_fill_paint(
        &mut self,
        object: &Mobject,
        paint: SemanticPaint,
        opacity: Option<f64>,
    ) -> Result<(), AuthoringError> {
        self.require_object(object)?;
        let state = prepare_fill_paint_value(
            &self.integration_store().borrow(),
            object.node_id(),
            object.state()?,
            paint,
            opacity,
        )?;
        let mut transaction = SemanticMutationTransaction::new();
        transaction.replace_style(object.node_id(), state.style);
        self.apply_semantic_transaction(transaction).map(|_| ())
    }

    /// Translate an object's own content and unique descendants in one publication.
    ///
    /// The delta is applied to authored positions. Runtime publication barriers
    /// reject the whole edit before any addressed leaf changes. Repeated aliases
    /// are translated once by the shared semantic traversal.
    pub fn shift(&mut self, object: &Mobject, delta: Vec2) -> Result<(), AuthoringError> {
        self.require_object(object)?;
        let transaction = {
            let store = self.integration_store().borrow();
            FamilyTranslation::begin_3d(
                &store,
                object.node_id(),
                SemanticVec3::new(f64::from(delta.x), f64::from(delta.y), 0.0),
            )?
            .transaction(&store)?
        };
        self.apply_semantic_transaction(transaction).map(|_| ())
    }

    fn edit_object_style(
        &mut self,
        object: &Mobject,
        edit: impl FnOnce(&mut SemanticStyle) -> Result<(), AuthoringError>,
    ) -> Result<(), AuthoringError> {
        self.require_object(object)?;
        let mut style = object.state()?.style;
        edit(&mut style)?;
        let mut transaction = SemanticMutationTransaction::new();
        transaction.replace_style(object.node_id(), style);
        self.apply_semantic_transaction(transaction).map(|_| ())
    }
}

#[cfg(test)]
mod tests;
