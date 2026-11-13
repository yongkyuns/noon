//! Ordinary persistent object edits through the Scene-owned publication route.

use crate::{
    family_authoring::FamilyTranslation,
    semantic_mobject::{
        edit_color, edit_fill, edit_fill_opacity, edit_stroke_width, prepare_fill_paint_value,
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
        self.edit_object_style(object, |style| {
            edit_color(
                style,
                f64::from(color.red),
                f64::from(color.green),
                f64::from(color.blue),
                f64::from(color.alpha),
            )
        })
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
        self.edit_object_style(object, |style| {
            edit_fill(
                style,
                f64::from(color.red),
                f64::from(color.green),
                f64::from(color.blue),
                opacity,
            )
        })
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
