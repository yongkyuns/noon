//! Short authoring operations over the Scene temporarily loaned to a source poll.

use std::{cell::RefCell, rc::Weak};

use noon_core::{HostCallbackId, SemanticMutationTransaction};

use super::{ScriptBoundary, ScriptBridge, ScriptError, ScriptScene};
use crate::{
    AnimationOptions, Color, ContinuationStep, EffectiveMobjectState, IntoAnimationRequest,
    ManimGeometryOptions, Mobject, RustHostCallbackContext, SemanticObjectState, SemanticStyle,
    Vec2, VectorPath,
};

/// A scene-owned updater registration. Removing it is an explicit authored action.
///
/// Dropping this value does not stop the updater. Its callable stays owned by the
/// script until the program is dropped, including after removal, so existing
/// semantic references never point at a reused callback identity.
pub struct UpdaterRegistration {
    scene: Weak<RefCell<ScriptBridge>>,
    target: Mobject,
    callback: HostCallbackId,
}

/// Time observed by one ordered updater invocation, in authored seconds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UpdaterFrame {
    time: f64,
    dt: f64,
}

impl UpdaterFrame {
    pub const fn time(self) -> f64 {
        self.time
    }

    pub const fn dt(self) -> f64 {
        self.dt
    }
}

/// The updater's target in the current effective callback phase.
///
/// These methods read earlier writes in the same ordered phase and stage effective
/// writes for its atomic publication. They do not edit persistent authored state.
pub struct UpdaterTarget<'context, 'phase> {
    context: &'context mut RustHostCallbackContext<'phase>,
}

impl UpdaterTarget<'_, '_> {
    /// Rotate around the scene origin using the current effective transform.
    pub fn rotate_about_origin(&mut self, angle: f64) -> Result<(), ScriptError> {
        self.rotate_about_point(angle, Vec2::ZERO)
    }

    /// Rotate around an explicit scene-space pivot.
    pub fn rotate_about_point(&mut self, angle: f64, pivot: Vec2) -> Result<(), ScriptError> {
        let transform = self
            .context
            .target_transform_rotated_about_point(angle, pivot)?;
        self.context.set_target_transform(transform)?;
        Ok(())
    }

    /// Recolor the effective fill and stroke while retaining their layer opacity.
    pub fn set_color(&mut self, color: Color) -> Result<(), ScriptError> {
        let style = self.context.target_style_with_color(
            f64::from(color.red),
            f64::from(color.green),
            f64::from(color.blue),
            f64::from(color.alpha),
        )?;
        self.context.set_target_style(style)?;
        Ok(())
    }

    pub fn set_fill_opacity(&mut self, opacity: f64) -> Result<(), ScriptError> {
        let style = self.context.target_style_with_fill_opacity(opacity)?;
        self.context.set_target_style(style)?;
        Ok(())
    }
}

impl ScriptScene {
    /// Construct detached geometry through this Scene's atomic publication route.
    pub fn geometry(&self, options: ManimGeometryOptions) -> Result<Mobject, ScriptError> {
        self.with_scene(|scene| Ok(scene.geometry(options)?))
    }

    pub fn circle(&self, radius: f64) -> Result<Mobject, ScriptError> {
        self.with_scene(|scene| Ok(scene.circle(radius)?))
    }

    pub fn square(&self, side: f64) -> Result<Mobject, ScriptError> {
        self.with_scene(|scene| Ok(scene.square(side)?))
    }

    pub fn rectangle(&self, width: f64, height: f64) -> Result<Mobject, ScriptError> {
        self.with_scene(|scene| Ok(scene.rectangle(width, height)?))
    }

    pub fn line(&self, start: Vec2, end: Vec2) -> Result<Mobject, ScriptError> {
        self.with_scene(|scene| {
            Ok(scene.line(
                (f64::from(start.x), f64::from(start.y)),
                (f64::from(end.x), f64::from(end.y)),
            )?)
        })
    }

    pub fn path(&self, path: VectorPath, style: SemanticStyle) -> Result<Mobject, ScriptError> {
        self.with_scene(|scene| Ok(scene.path(path, style)?))
    }

    pub fn add(&self, object: &Mobject) -> Result<(), ScriptError> {
        self.with_scene(|scene| Ok(scene.add(object)?))
    }

    pub fn remove(&self, object: &Mobject) -> Result<(), ScriptError> {
        self.with_scene(|scene| Ok(scene.remove(object)?))
    }

    /// Read the persistent base declaration, independent of animation/updaters.
    pub fn authored(&self, object: &Mobject) -> Result<SemanticObjectState, ScriptError> {
        self.with_scene(|scene| Ok(scene.authored(object)?))
    }

    /// Read the coherent current value, including completed animations/updaters.
    pub fn effective(&self, object: &Mobject) -> Result<EffectiveMobjectState, ScriptError> {
        self.with_scene(|scene| Ok(scene.effective(object)?))
    }

    /// Current published authored time; after an await this is its exact boundary.
    pub fn time(&self) -> Result<f64, ScriptError> {
        self.with_scene(|scene| Ok(authoring_time(scene)))
    }

    /// Publish a persistent recoloring without changing enabled layer opacity.
    pub fn set_color(&self, object: &Mobject, color: Color) -> Result<(), ScriptError> {
        self.with_scene(|scene| {
            scene.require_object(object)?;
            if scene.running_execution().is_none() {
                object.clone().set_color(
                    f64::from(color.red),
                    f64::from(color.green),
                    f64::from(color.blue),
                    f64::from(color.alpha),
                )?;
                return Ok(());
            }
            scene.owned_live().set_color(
                object,
                f64::from(color.red),
                f64::from(color.green),
                f64::from(color.blue),
                f64::from(color.alpha),
            )?;
            Ok(())
        })
    }

    /// Publish a persistent fill color and explicit layer opacity.
    pub fn set_fill(
        &self,
        object: &Mobject,
        color: Color,
        opacity: f64,
    ) -> Result<(), ScriptError> {
        self.with_scene(|scene| {
            scene.require_object(object)?;
            if scene.running_execution().is_none() {
                object.clone().set_fill(
                    f64::from(color.red),
                    f64::from(color.green),
                    f64::from(color.blue),
                    opacity,
                )?;
                return Ok(());
            }
            scene.owned_live().set_fill(
                object,
                f64::from(color.red),
                f64::from(color.green),
                f64::from(color.blue),
                opacity,
            )?;
            Ok(())
        })
    }

    pub fn set_fill_opacity(&self, object: &Mobject, opacity: f64) -> Result<(), ScriptError> {
        self.with_scene(|scene| {
            scene.require_object(object)?;
            if scene.running_execution().is_none() {
                object.clone().set_fill_opacity(opacity)?;
                return Ok(());
            }
            scene.owned_live().set_fill_opacity(object, opacity)?;
            Ok(())
        })
    }

    /// Publish a persistent scene-space translation.
    pub fn shift(&self, object: &Mobject, delta: Vec2) -> Result<(), ScriptError> {
        self.with_scene(|scene| {
            scene.require_object(object)?;
            if scene.running_execution().is_none() {
                object
                    .clone()
                    .shift(f64::from(delta.x), f64::from(delta.y))?;
                return Ok(());
            }
            scene
                .owned_live()
                .shift(object, f64::from(delta.x), f64::from(delta.y))?;
            Ok(())
        })
    }

    /// Play one request, or a tuple/collection of requests in parallel.
    ///
    /// The returned future is inert until awaited. Method builders capture each
    /// target's effective state at admission, then publish the entire composition
    /// atomically. The await completes after logical completion and the existing
    /// renderer-publication admission barrier.
    pub fn play(&self, request: impl IntoAnimationRequest) -> ScriptBoundary {
        self.play_with(request, AnimationOptions::default())
    }

    /// Play a composition with explicit duration, rate, and other shared options.
    pub fn play_with(
        &self,
        request: impl IntoAnimationRequest,
        options: AnimationOptions,
    ) -> ScriptBoundary {
        let request = request.into_animation_request();
        self.boundary(move |scene| {
            Ok(ContinuationStep::Await(crate::animation_request::activate(
                &mut scene.owned_live(),
                request,
                options,
            )?))
        })
    }

    /// Wait in authored time through the same bounded execution/completion path.
    pub fn wait(&self, seconds: f64) -> ScriptBoundary {
        self.boundary(move |scene| {
            Ok(ContinuationStep::Await(
                scene.owned_live().wait_segment(seconds)?,
            ))
        })
    }

    /// Register an ordered updater at the current scene time.
    ///
    /// The scene owns its callable and registration. The target argument is a
    /// short-lived effective view; returning an error aborts the whole callback
    /// phase. Use [`Self::remove_updater`] to stop this exact registration.
    pub fn add_updater<F>(
        &self,
        target: &Mobject,
        mut callback: F,
    ) -> Result<UpdaterRegistration, ScriptError>
    where
        F: for<'context, 'phase> FnMut(
                &mut UpdaterTarget<'context, 'phase>,
                UpdaterFrame,
            ) -> Result<(), ScriptError>
            + 'static,
    {
        self.with_scene_and_callbacks(|scene, callbacks| {
            scene.require_object(target)?;
            let id = callbacks
                .next_script_id()
                .ok_or(ScriptError::CallbackIdentityExhausted)?;
            callbacks.insert(id, move |context| {
                let frame = UpdaterFrame {
                    time: context.time(),
                    dt: context.delta_time(),
                };
                callback(&mut UpdaterTarget { context }, frame)
            })?;
            let mut transaction = SemanticMutationTransaction::new();
            transaction.add_updater(target.node_id(), id, authoring_time(scene), None);
            if let Err(error) = scene.apply_semantic_transaction(transaction) {
                callbacks.discard_script_callback(id);
                return Err(error.into());
            }
            Ok(UpdaterRegistration {
                scene: self.bridge.clone(),
                target: target.clone(),
                callback: id,
            })
        })
    }

    /// Stop this registration at the current authored time.
    ///
    /// Previously committed effective writes remain visible. The semantic
    /// registration owns activation timing; this action does not replay source.
    pub fn remove_updater(&self, registration: &UpdaterRegistration) -> Result<(), ScriptError> {
        if !Weak::ptr_eq(&self.bridge, &registration.scene) {
            return Err(ScriptError::ForeignUpdaterRegistration);
        }
        self.with_scene(|scene| {
            scene.require_object(&registration.target)?;
            let mut transaction = SemanticMutationTransaction::new();
            transaction.remove_updater(
                registration.target.node_id(),
                registration.callback,
                authoring_time(scene),
            );
            scene.apply_semantic_transaction(transaction)?;
            Ok(())
        })
    }
}

fn authoring_time(scene: &crate::Scene) -> f64 {
    scene
        .running_execution()
        .map_or_else(|| scene.time(), crate::ExecutionSession::effective_time)
}
