//! Native, session-owned translation dragging over the existing pointer ingress.

use std::{collections::BTreeSet, sync::Arc};

use noon_core::{
    NativePointerId, NativePointerInput, NativePointerInputKind, SemanticMutationTransaction,
    SemanticNodeId, SemanticObjectProperty, SemanticStore, SemanticVec3, Vec2,
};

use super::publication::{PreparedRuntimePublication, SemanticPublicationPurpose};
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
    targets: Arc<BTreeSet<SemanticNodeId>>,
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
    ) -> Result<(), TranslationDragError> {
        self.cancel_translation_drag()?;
        self.translation_drag.targets = Arc::new(targets.into_iter().collect());
        Ok(())
    }

    pub fn translation_drag_active(&self) -> bool {
        self.translation_drag.active.is_some()
    }

    pub fn cancel_translation_drag(&mut self) -> Result<(), TranslationDragError> {
        if let Some(active) = self.translation_drag.active {
            self.ensure_direct_input_ingress_available()?;
            let base = self
                .runtime
                .translation_drag_base(active.object)
                .ok_or(TranslationDragError::RetiredTarget)?;
            let effective = self.prepared_drag_batch(active.object, base)?;
            // Cancellation without a pointer occurrence still needs to restore
            // the base frame; use the existing prepared input evaluation rather
            // than mutating the live row directly.
            let time = self.frame().time;
            let frame = self
                .runtime
                .prepare_advance_to(time)
                .map_err(ExecutionSessionInputError::Evaluation)?;
            let held = self
                .runtime
                .suspend_translation_drag(active.object)
                .ok_or(TranslationDragError::RetiredTarget)?;
            if let Err(error) = self.runtime.commit_prepared_frame(frame, effective) {
                self.runtime.restore_translation_drag(active.object, held);
                return Err(ExecutionSessionInputError::PreparedCommit(error).into());
            }
            self.runtime
                .clear_translation_drag_effective_driver(active.object);
            self.translation_drag.active = None;
        }
        Ok(())
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
            let targets = Arc::clone(&self.translation_drag.targets);
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
        } else if let Some(active) = active.filter(|active| {
            active.pointer == input.pointer()
                && matches!(input.kind(), NativePointerInputKind::Cancel(_))
        }) {
            self.prepared_drag_batch(
                active.object,
                self.runtime
                    .translation_drag_base(active.object)
                    .ok_or(TranslationDragError::RetiredTarget)?,
            )?
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
            return self.commit_drag_release(store, token, input, active);
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
            self.runtime.invalidate_replay_domain();
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
        if cancellation.is_some() {
            self.translation_drag.active = None;
            self.runtime
                .clear_translation_drag_effective_driver(cancellation.expect("checked").object);
        }
        Ok(TranslationDragReceipt {
            input: publication,
            undo: None,
        })
    }

    fn commit_drag_release(
        &mut self,
        store: &mut SemanticStore,
        token: &NativePointerInputToken,
        input: NativePointerInput,
        active: ActiveDrag,
    ) -> Result<TranslationDragReceipt, TranslationDragError> {
        let translation = input.position().map_or(active.translation, |position| {
            active.effective_before + (position.scene() - active.press_scene)
        });
        let after = SemanticVec3::new(
            f64::from(translation.x),
            f64::from(translation.y),
            active.authored_before.z,
        );
        let mut transaction = SemanticMutationTransaction::new();
        transaction.set_property(active.node, SemanticObjectProperty::Translation, after);
        let prepared_semantic = transaction
            .prepare(store)
            .map_err(|error| TranslationDragError::Semantic(error.to_string()))?;
        let native = self.prepare_native_pointer_input_with_effective(
            token,
            input,
            self.empty_effective_batch(),
            true,
        )?;
        let (frame, effective, timeline, metadata) = native.into_parts();
        self.translation_drag.active = None;
        let publication = self.apply_prepared_semantic_transaction_with_execution_contract(
            prepared_semantic,
            Vec::new(),
            PreparedRuntimePublication { effective, frame },
            SemanticPublicationPurpose::AuthoredMutation,
            None,
            None,
        );
        if let Err(error) = publication {
            self.translation_drag.active = Some(active);
            return Err(TranslationDragError::Semantic(error.to_string()));
        }
        let input = self
            .commit_prepared_native_pointer_metadata(timeline, metadata)
            .expect(
                "prepared drag release suppresses click actions and cannot fail after publication",
            );
        self.runtime.release_translation_drag(active.object);
        Ok(TranslationDragReceipt {
            input,
            undo: Some(TranslationDragUndo {
                store: store.identity().clone(),
                runtime: self.runtime_identity(),
                scene_revision: store.scene_revision(),
                node: active.node,
                before: active.authored_before,
                after,
            }),
        })
    }

    fn apply_drag_authored_translation(
        &mut self,
        store: &mut SemanticStore,
        node: SemanticNodeId,
        value: SemanticVec3,
    ) -> Result<(), TranslationDragError> {
        if self.translation_drag.active.is_some() {
            return Err(TranslationDragError::DriverConflict);
        }
        let mut transaction = SemanticMutationTransaction::new();
        transaction.set_property(node, SemanticObjectProperty::Translation, value);
        self.apply_semantic_transaction(store, transaction)
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
