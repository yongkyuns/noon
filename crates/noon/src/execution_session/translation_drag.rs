//! Native, session-owned translation dragging over the existing pointer ingress.

use std::collections::BTreeSet;

use noon_core::{
    NativePointerId, NativePointerInput, NativePointerInputKind, SemanticMutationTransaction,
    SemanticNodeId, SemanticObjectProperty, SemanticStore, SemanticVec3, Vec2,
};

use super::{
    ExecutionSession, ExecutionSessionInputError, NativePointerInputPublication,
    NativePointerInputToken, PointerFillOutcome,
};
use noon_runtime::{EffectivePropertyWrite, PreparedEffectivePropertyBatch};

#[derive(Clone, Copy, Debug, PartialEq)]
struct ActiveDrag {
    pointer: NativePointerId,
    node: SemanticNodeId,
    object: noon_core::ObjectId,
    press_scene: Vec2,
    authored_before: SemanticVec3,
    effective_before: Vec2,
    translation: Vec2,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct TranslationDragState {
    targets: BTreeSet<SemanticNodeId>,
    active: Option<ActiveDrag>,
}

impl TranslationDragState {
    pub(super) const fn is_active(&self) -> bool {
        self.active.is_some()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum TranslationDragError {
    Input(ExecutionSessionInputError),
    ForeignStore,
    NotTarget,
    DriverConflict,
    RetiredTarget,
    StaleUndo,
    Semantic(String),
}
impl std::fmt::Display for TranslationDragError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Input(error) => error.fmt(f),
            Self::ForeignStore => f.write_str("semantic store does not own this drag session"),
            Self::NotTarget => f.write_str("pointer press did not hit a configured drag target"),
            Self::DriverConflict => {
                f.write_str("translation drag conflicts with an active effective driver")
            }
            Self::RetiredTarget => f.write_str("captured drag target is no longer live"),
            Self::StaleUndo => {
                f.write_str("translation drag undo no longer matches the live scene")
            }
            Self::Semantic(error) => f.write_str(error),
        }
    }
}
impl std::error::Error for TranslationDragError {}
impl From<ExecutionSessionInputError> for TranslationDragError {
    fn from(value: ExecutionSessionInputError) -> Self {
        Self::Input(value)
    }
}

/// One committed authored translation with its inverse data. Hosts retain this
/// value as their undo action; applying it uses the normal semantic publication.
#[derive(Clone, Debug, PartialEq)]
pub struct TranslationDragUndo {
    store: noon_core::SemanticStoreIdentity,
    runtime: noon_runtime::RuntimeIdentity,
    scene_revision: noon_core::SceneRevision,
    node: SemanticNodeId,
    before: SemanticVec3,
    after: SemanticVec3,
}
impl TranslationDragUndo {
    pub fn undo(
        self,
        session: &mut ExecutionSession,
        store: &mut SemanticStore,
    ) -> Result<(), TranslationDragError> {
        if store.identity() != self.store
            || session.runtime_identity() != self.runtime
            || store.scene_revision() != self.scene_revision
            || store
                .semantic_object_state_checked(self.node)
                .map_err(|_| TranslationDragError::StaleUndo)?
                .transform
                .translation
                != self.after
        {
            return Err(TranslationDragError::StaleUndo);
        }
        session.apply_drag_authored_translation(store, self.node, self.before)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TranslationDragReceipt {
    pub input: NativePointerInputPublication,
    pub undo: Option<TranslationDragUndo>,
}

impl ExecutionSession {
    /// Configure the complete native target policy. Frontends supply only typed
    /// pointer occurrences; they never choose an object identity per event.
    pub fn set_translation_drag_targets(
        &mut self,
        targets: impl IntoIterator<Item = SemanticNodeId>,
    ) {
        self.translation_drag.targets = targets.into_iter().collect();
        self.cancel_translation_drag();
    }

    pub fn translation_drag_active(&self) -> bool {
        self.translation_drag.active.is_some()
    }

    pub fn cancel_translation_drag(&mut self) {
        if let Some(active) = self.translation_drag.active.take() {
            self.runtime
                .suspend_translation_drag(active.object)
                .expect("active drag retains a live runtime lease");
            let effective = self.empty_effective_batch();
            // Cancellation without a pointer occurrence still needs to restore
            // the base frame; use the existing prepared input evaluation rather
            // than mutating the live row directly.
            let time = self.frame().time;
            let frame = self
                .runtime
                .prepare_advance_to(time)
                .expect("current authored time always prepares a drag restoration");
            self.runtime
                .commit_prepared_frame(frame, effective)
                .expect("suspended drag restoration was prepared against this frame");
        }
    }

    /// The drag entry uses the same typed, ordered pointer ingress as ordinary
    /// native input. A held stationary pointer has no scheduler/waker work.
    pub fn submit_translation_drag_input(
        &mut self,
        store: &mut SemanticStore,
        token: &NativePointerInputToken,
        input: NativePointerInput,
    ) -> Result<TranslationDragReceipt, TranslationDragError> {
        self.require_published_store(store)
            .map_err(|_| TranslationDragError::ForeignStore)?;
        self.preflight_native_pointer_input(token, input)?;
        let press_target = if matches!(
            input.kind(),
            NativePointerInputKind::Press { button: 0, .. }
        ) && self.translation_drag.active.is_none()
        {
            let targets = self.translation_drag.targets.clone();
            match self
                .pick_native_pointer_fill(token, input, |node| targets.contains(&node))?
                .outcome()
            {
                PointerFillOutcome::Hit(node) => Some(node),
                PointerFillOutcome::Miss => None,
                PointerFillOutcome::NoPosition | PointerFillOutcome::Unsupported { .. } => None,
            }
        } else {
            None
        };
        let start = if let Some(node) = press_target {
            let object = self
                .execution_object_id(node)
                .ok_or(TranslationDragError::RetiredTarget)?;
            let state = store
                .semantic_object_state_checked(node)
                .map_err(|_| TranslationDragError::RetiredTarget)?;
            if state
                .signal_bindings()
                .iter()
                .any(|binding| binding.property() == SemanticObjectProperty::Translation)
                || self.runtime.translation_drag_conflicts(object)
            {
                return Err(TranslationDragError::DriverConflict);
            }
            let position = input
                .position()
                .ok_or(TranslationDragError::NotTarget)?
                .scene();
            let effective = self
                .runtime
                .effective_transform(object)
                .ok_or(TranslationDragError::RetiredTarget)?;
            self.prepared_drag_batch(object, effective.translation)?;
            Some(ActiveDrag {
                pointer: input.pointer(),
                node,
                object,
                press_scene: position,
                authored_before: state.transform.translation,
                effective_before: effective.translation,
                translation: effective.translation,
            })
        } else {
            None
        };
        let active = self.translation_drag.active;
        let update = active
            .filter(|active| active.pointer == input.pointer())
            .and_then(|active| {
                let point = match input.kind() {
                    NativePointerInputKind::Move(position) => position,
                    _ => return None,
                };
                let translation = active.effective_before + (point.scene() - active.press_scene);
                Some((active, translation))
            });
        let effective = if let Some(start) = start {
            self.prepared_drag_batch(start.object, start.translation)?
        } else if let Some((active, translation)) = update {
            self.prepared_drag_batch(active.object, translation)?
        } else {
            self.empty_effective_batch()
        };
        let release = active.filter(|active| {
            active.pointer == input.pointer()
                && matches!(
                    input.kind(),
                    NativePointerInputKind::Release { button: 0, .. }
                )
        });
        if let Some(active) = release {
            self.preflight_drag_authored_translation(store, active.node, active.translation)?;
        }

        let cancellation = active.filter(|active| {
            active.pointer == input.pointer()
                && matches!(input.kind(), NativePointerInputKind::Cancel(_))
        });
        let suspended = cancellation.and_then(|active| {
            self.runtime
                .suspend_translation_drag(active.object)
                .map(|translation| (active, translation))
        });
        let publication =
            match self.submit_native_pointer_input_with_effective(token, input, effective, true) {
                Ok(publication) => publication,
                Err(error) => {
                    if let Some((active, translation)) = suspended {
                        self.runtime
                            .restore_translation_drag(active.object, translation);
                    }
                    return Err(error.into());
                }
            };
        if let Some(start) = start {
            self.runtime
                .adopt_translation_drag(start.object, start.translation);
            self.translation_drag.active = Some(start);
        }
        if let Some((active, translation)) = update {
            self.runtime
                .adopt_translation_drag(active.object, translation);
            self.translation_drag.active = Some(ActiveDrag {
                translation,
                ..active
            });
        }
        let undo = if let Some(active) = release {
            // Keep the scoped lease until semantic publication succeeds.  If an
            // unexpected publication failure occurs, the captured target and its
            // effective position remain intact for an explicit cancel or retry.
            match self.apply_drag_authored_translation(
                store,
                active.node,
                SemanticVec3::new(
                    f64::from(active.translation.x),
                    f64::from(active.translation.y),
                    0.0,
                ),
            ) {
                Ok(()) => {
                    self.translation_drag.active = None;
                    self.runtime.release_translation_drag(active.object);
                    let after = SemanticVec3::new(
                        f64::from(active.translation.x),
                        f64::from(active.translation.y),
                        0.0,
                    );
                    Some(TranslationDragUndo {
                        store: store.identity().clone(),
                        runtime: self.runtime_identity(),
                        scene_revision: store.scene_revision(),
                        node: active.node,
                        before: active.authored_before,
                        after,
                    })
                }
                Err(error) => return Err(error),
            }
        } else {
            if cancellation.is_some() {
                self.translation_drag.active = None;
            }
            None
        };
        Ok(TranslationDragReceipt {
            input: publication,
            undo,
        })
    }

    fn apply_drag_authored_translation(
        &mut self,
        store: &mut SemanticStore,
        node: SemanticNodeId,
        value: SemanticVec3,
    ) -> Result<(), TranslationDragError> {
        let mut transaction = SemanticMutationTransaction::new();
        transaction.set_property(node, SemanticObjectProperty::Translation, value);
        // The scoped lease deliberately blocks ordinary source edits.  Release
        // is its one reconciliation, so temporarily remove only the session
        // marker while retaining the runtime lease until publication succeeds.
        let active = self.translation_drag.active.take();
        let result = self
            .apply_semantic_transaction(store, transaction)
            .map(|_| ())
            .map_err(|error| TranslationDragError::Semantic(error.to_string()));
        if result.is_err() {
            self.translation_drag.active = active;
        }
        result
    }

    fn preflight_drag_authored_translation(
        &self,
        store: &mut SemanticStore,
        node: SemanticNodeId,
        value: Vec2,
    ) -> Result<(), TranslationDragError> {
        let mut transaction = SemanticMutationTransaction::new();
        transaction.set_property(
            node,
            SemanticObjectProperty::Translation,
            SemanticVec3::new(f64::from(value.x), f64::from(value.y), 0.0),
        );
        transaction
            .prepare(store)
            .map(|_| ())
            .map_err(|error| TranslationDragError::Semantic(error.to_string()))
    }

    fn empty_effective_batch(&self) -> PreparedEffectivePropertyBatch {
        self.runtime
            .prepare_effective_property_batch(&[])
            .expect("an empty effective-property batch is always valid")
    }

    fn prepared_drag_batch(
        &self,
        object: noon_core::ObjectId,
        translation: Vec2,
    ) -> Result<PreparedEffectivePropertyBatch, TranslationDragError> {
        self.runtime
            .prepare_effective_property_batch(&[EffectivePropertyWrite::Translation {
                object,
                translation,
            }])
            .map_err(|_| TranslationDragError::DriverConflict)
    }
}
