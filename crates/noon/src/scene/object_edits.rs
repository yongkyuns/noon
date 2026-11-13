//! Ordinary persistent object edits through the Scene-owned publication route.

use crate::{
    family_authoring::FamilyTranslation,
    semantic_mobject::{
        authoring_render_f64, edit_color, edit_disable_fill, edit_disable_stroke, edit_fade,
        edit_fill, edit_fill_color, edit_fill_opacity, edit_manim_opacity, edit_object_opacity,
        edit_stroke, edit_stroke_color, edit_stroke_opacity, edit_stroke_width,
        prepare_fill_paint_value,
    },
    AuthoringError, EffectiveMobjectState, Mobject, Scene,
};
use noon_core::{
    Color, HatchPaint, LinearGradient, NoisePaint, RadialGradient, SemanticMutationTransaction,
    SemanticObjectProperty, SemanticPaint, SemanticSignalValue, SemanticStyle, SemanticVec3, Vec2,
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

    pub fn effective_fill_color(&self, mobject: &Mobject) -> Result<Option<Color>, AuthoringError> {
        let effective = self.effective(mobject)?;
        if effective.fill_paint.is_some()
            || matches!(
                mobject.state()?.style.fill,
                Some(noon_core::SemanticPaint::Image(_))
            )
        {
            return Err(crate::AuthoringError::Unsupported(
                crate::UnsupportedAuthoringOperation::ResourcePaintColorQuery,
            ));
        }
        Ok(effective
            .style
            .fill
            .map(crate::semantic_mobject::opaque_paint_color))
    }

    pub fn effective_stroke_color(
        &self,
        mobject: &Mobject,
    ) -> Result<Option<Color>, AuthoringError> {
        mobject.stroke_color()?;
        Ok(self
            .effective(mobject)?
            .style
            .stroke
            .map(crate::semantic_mobject::opaque_paint_color))
    }

    pub fn effective_stroke_width(&self, mobject: &Mobject) -> Result<f64, AuthoringError> {
        Ok(f64::from(self.effective(mobject)?.style.stroke_width))
    }

    pub fn effective_stroke_cap(
        &self,
        mobject: &Mobject,
    ) -> Result<noon_core::StrokeCap, AuthoringError> {
        Ok(self.effective(mobject)?.style.stroke_cap)
    }

    /// Read visible fill RGB, falling back to stroke RGB.
    pub fn effective_manim_color(&self, mobject: &Mobject) -> Result<Color, AuthoringError> {
        effective_manim_color(mobject, self.effective(mobject)?)
    }

    /// Match the donor's authored or effective color while retaining recipient opacity.
    pub fn match_color(&mut self, object: &Mobject, donor: &Mobject) -> Result<(), AuthoringError> {
        let color = if self.running_execution().is_some() {
            self.effective_manim_color(donor)?
        } else {
            self.require_object(donor)?;
            donor.manim_color()?
        };
        self.set_color_rgba(
            object,
            color.red.into(),
            color.green.into(),
            color.blue.into(),
            1.0,
        )
    }

    /// Interpolate authored paint toward a color using Manim's point-bearing rule.
    pub fn fade_to(
        &mut self,
        object: &Mobject,
        target: Color,
        amount: f64,
    ) -> Result<(), AuthoringError> {
        self.require_object(object)?;
        if let Some(execution) = self.running_execution() {
            execution.require_published_store(&self.integration_store().borrow())?;
        }
        if !object.has_fade_to_points()? {
            return self.publish_object_style(object, object.state()?.style);
        }
        self.edit_object_style(object, |style| {
            crate::semantic_mobject::edit_fade_to(style, target, amount)
        })
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
        self.publish_object_style(object, state.style)
    }

    /// Set the planar authored translation while preserving depth.
    pub fn set_translation(
        &mut self,
        object: &Mobject,
        x: f64,
        y: f64,
    ) -> Result<(), AuthoringError> {
        self.require_object(object)?;
        if self.running_execution().is_none() {
            return object.clone().set_translation(x, y);
        }
        let z = object.state()?.transform.translation.z;
        self.set_object_property(
            object,
            SemanticObjectProperty::Translation,
            SemanticVec3::new(x, y, z),
        )
    }

    /// Set the planar authored scale while preserving depth scale.
    pub fn set_scale(&mut self, object: &Mobject, x: f64, y: f64) -> Result<(), AuthoringError> {
        self.require_object(object)?;
        if self.running_execution().is_none() {
            return object.clone().set_scale(x, y);
        }
        let z = object.state()?.transform.scale.z;
        self.set_object_property(
            object,
            SemanticObjectProperty::Scale,
            SemanticVec3::new(x, y, z),
        )
    }

    /// Multiply the object's authored scale in the plane.
    pub fn scale(&mut self, object: &Mobject, x: f64, y: f64) -> Result<(), AuthoringError> {
        let x = authoring_render_f64("scale.x", x)?;
        let y = authoring_render_f64("scale.y", y)?;
        self.require_object(object)?;
        let mut scale = object.state()?.transform.scale;
        scale.x *= x;
        scale.y *= y;
        scale.lower_xy_f32()?;
        self.set_object_property(object, SemanticObjectProperty::Scale, scale)
    }

    /// Set the object's authored planar rotation.
    pub fn set_rotation(&mut self, object: &Mobject, angle: f64) -> Result<(), AuthoringError> {
        self.require_object(object)?;
        if self.running_execution().is_none() {
            return object.clone().set_rotation(angle);
        }
        self.set_object_property(object, SemanticObjectProperty::RotationZ, angle)
    }

    /// Set the nominal value of an Angle, leaving its drawn geometry intact.
    pub fn set_angle_value(
        &mut self,
        object: &Mobject,
        radians: f64,
    ) -> Result<(), AuthoringError> {
        self.require_object(object)?;
        if self.running_execution().is_none() {
            return object.clone().set_angle_value(radians);
        }
        let mut transaction = SemanticMutationTransaction::new();
        transaction.set_angle_value(object.node_id(), radians);
        self.apply_semantic_transaction(transaction).map(|_| ())
    }

    fn set_object_property(
        &mut self,
        object: &Mobject,
        property: SemanticObjectProperty,
        value: impl Into<SemanticSignalValue>,
    ) -> Result<(), AuthoringError> {
        let mut transaction = SemanticMutationTransaction::new();
        transaction.set_property(object.node_id(), property, value);
        self.apply_semantic_transaction(transaction).map(|_| ())
    }

    /// Translate an object's own content and unique descendants in one publication.
    ///
    /// The delta is applied to authored positions. Runtime publication barriers
    /// reject the whole edit before any addressed leaf changes. Repeated aliases
    /// are translated once by the shared semantic traversal.
    pub fn shift(&mut self, object: &Mobject, delta: Vec2) -> Result<(), AuthoringError> {
        self.shift_3d(
            object,
            SemanticVec3::new(f64::from(delta.x), f64::from(delta.y), 0.0),
        )
    }

    /// Translate an object's own content and unique descendants in three dimensions.
    pub fn shift_3d(
        &mut self,
        object: &Mobject,
        delta: SemanticVec3,
    ) -> Result<(), AuthoringError> {
        self.require_object(object)?;
        if self.running_execution().is_some() {
            if !delta.is_finite() {
                let mut translation = object.state()?.transform.translation;
                translation.x += delta.x;
                translation.y += delta.y;
                translation.z += delta.z;
                return self.set_object_property(
                    object,
                    SemanticObjectProperty::Translation,
                    translation,
                );
            }
            return self.with_running_execution(|store, root, execution| {
                crate::family_layout::publish_shift_root(
                    store,
                    root,
                    execution,
                    object.node_id(),
                    delta,
                )
                .map(|_| ())
            });
        }
        let transaction = {
            let store = self.integration_store().borrow();
            FamilyTranslation::begin_3d(&store, object.node_id(), delta)?.transaction(&store)?
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
        self.publish_object_style(object, style)
    }

    fn publish_object_style(
        &mut self,
        object: &Mobject,
        style: SemanticStyle,
    ) -> Result<(), AuthoringError> {
        let mut transaction = SemanticMutationTransaction::new();
        transaction.replace_style(object.node_id(), style);
        self.apply_semantic_transaction(transaction).map(|_| ())
    }
}

pub(crate) fn effective_manim_color(
    mobject: &Mobject,
    effective: EffectiveMobjectState,
) -> Result<Color, AuthoringError> {
    if effective.fill_paint.is_some() && effective.style.fill.is_some_and(|color| color.alpha > 0.0)
    {
        return Err(crate::AuthoringError::Unsupported(
            crate::UnsupportedAuthoringOperation::ResourcePaintColorQuery,
        ));
    }
    if !matches!(
        mobject.state()?.style.fill,
        Some(
            noon_core::SemanticPaint::LinearGradient(_)
                | noon_core::SemanticPaint::RadialGradient(_)
                | noon_core::SemanticPaint::Noise(_)
                | noon_core::SemanticPaint::Hatch(_)
                | noon_core::SemanticPaint::Image(_)
        )
    ) {
        mobject.manim_color()?;
    }
    Ok(crate::semantic_mobject::manim_color_from_effective(
        &effective.style,
    ))
}

#[cfg(test)]
mod tests;
