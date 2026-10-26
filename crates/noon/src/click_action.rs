//! Object-owned click declarations over the ordinary semantic transaction path.

use std::{cell::RefCell, rc::Rc};

use noon_core::{
    resolve_glow_pulse_animation_options, AnimationDefaults, AnimationOptions, Glow, GlowUpdate,
    RateFunction, SemanticAnimationIntent, SemanticClickAction, SemanticClickGlowPulse,
    SemanticMutationTransaction, SemanticNodeId, SemanticNodeKind, SemanticStore,
    SemanticStoreError,
};

use crate::{AuthoringError, DeclaredAnimation, LiveSession, LiveSessionError, Mobject, Scene};

fn glow_pulse_declaration(
    intensity: f64,
    options: AnimationOptions,
) -> Result<SemanticClickGlowPulse, AuthoringError> {
    // Share the authored pulse's numerical and timing contracts. This prepares
    // only values; registration never captures or creates an attachment.
    GlowUpdate::default()
        .intensity(intensity)
        .prepare(Glow::default())?;
    let resolved = resolve_glow_pulse_animation_options(
        AnimationDefaults {
            rate_func: RateFunction::ThereAndBack,
            ..AnimationDefaults::MANIM
        },
        options,
        AnimationOptions::new(),
    )
    .map_err(|_| AuthoringError::InvalidClickActionOptions)?;
    if resolved.lag_ratio != 0.0
        || resolved.path_arc != 0.0
        || resolved.remover
        || resolved.introducer
        || !resolved.rate_func.preserves_unit_interval()
    {
        return Err(AuthoringError::InvalidClickActionOptions);
    }
    Ok(SemanticClickGlowPulse::new(
        intensity,
        resolved.run_time,
        resolved.rate_func,
        resolved.reverse_rate_function,
    ))
}

fn declared_click_action(
    store: &Rc<RefCell<SemanticStore>>,
    target: SemanticNodeId,
    animation: &DeclaredAnimation,
) -> Result<SemanticClickAction, AuthoringError> {
    if !Rc::ptr_eq(store, animation.integration_store()) {
        return Err(AuthoringError::ForeignStore);
    }
    let store = store.borrow();
    let node = store
        .node(animation.node_id())
        .ok_or(SemanticStoreError::UnknownNode(animation.node_id()))?;
    let SemanticNodeKind::Animation(state) = node.kind() else {
        return Err(AuthoringError::InvalidClickActionDeclaration {
            target,
            animation: animation.node_id(),
        });
    };
    match state.intent() {
        SemanticAnimationIntent::Indicate {
            target: animated,
            scale_factor,
            color,
            ..
        } if *animated == target => Ok(SemanticClickAction::Indicate(
            crate::scene::click_indicate_declaration(
                crate::IndicateOptions::new(*scale_factor, *color),
                state.options(),
            )?,
        )),
        SemanticAnimationIntent::GlowPulse {
            target: animated,
            intensity,
        } if *animated == target => Ok(SemanticClickAction::GlowPulse(glow_pulse_declaration(
            *intensity,
            state.options(),
        )?)),
        _ => Err(AuthoringError::InvalidClickActionDeclaration {
            target,
            animation: animation.node_id(),
        }),
    }
}

impl Scene {
    /// Replace this object's primary-click action, or clear it with `None`.
    ///
    /// Supported declarations are `Indicate` and `GlowPulse` for this same
    /// single target. Registration copies only authored action parameters;
    /// activation captures current values through the shared input session.
    pub fn on_click(
        &mut self,
        target: &Mobject,
        animation: Option<&DeclaredAnimation>,
    ) -> Result<(), AuthoringError> {
        self.require_object(target)?;
        let binding = animation
            .map(|animation| {
                declared_click_action(self.integration_store(), target.node_id(), animation)
            })
            .transpose()?;
        let mut transaction = SemanticMutationTransaction::new();
        transaction.set_click_action(target.node_id(), binding);
        self.apply_semantic_transaction(transaction).map(|_| ())
    }

    /// Declare a click pulse directly without creating a replay declaration.
    /// Language wrappers use this same Rust validation and publication path.
    pub fn on_click_glow_pulse(
        &mut self,
        target: &Mobject,
        intensity: f64,
        options: AnimationOptions,
    ) -> Result<(), AuthoringError> {
        self.require_object(target)?;
        let binding = glow_pulse_declaration(intensity, options)?;
        let mut transaction = SemanticMutationTransaction::new();
        transaction.set_click_action(
            target.node_id(),
            Some(SemanticClickAction::GlowPulse(binding)),
        );
        self.apply_semantic_transaction(transaction).map(|_| ())
    }
}

impl LiveSession<'_> {
    /// Replace or clear one object-owned action through live semantic publication.
    pub fn on_click(
        &mut self,
        target: &Mobject,
        animation: Option<&DeclaredAnimation>,
    ) -> Result<(), LiveSessionError> {
        self.require_mobject(target)?;
        let binding = animation
            .map(|animation| {
                declared_click_action(self.integration_store(), target.node_id(), animation)
            })
            .transpose()?;
        let mut transaction = SemanticMutationTransaction::new();
        transaction.set_click_action(target.node_id(), binding);
        self.apply(transaction)?;
        Ok(())
    }

    /// Publish a click pulse without allocating an attachment or replay track.
    pub fn on_click_glow_pulse(
        &mut self,
        target: &Mobject,
        intensity: f64,
        options: AnimationOptions,
    ) -> Result<(), LiveSessionError> {
        self.require_mobject(target)?;
        let binding = glow_pulse_declaration(intensity, options)?;
        let mut transaction = SemanticMutationTransaction::new();
        transaction.set_click_action(
            target.node_id(),
            Some(SemanticClickAction::GlowPulse(binding)),
        );
        self.apply(transaction)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
