use std::collections::BTreeSet;

use noon_compile::ExecutionPatch;
use noon_core::{SemanticMutationTransaction, SemanticNodeId, SemanticStore};
use noon_runtime::{EffectivePropertyWrite, FrameState, RuntimeIdentity};

use crate::{
    CallbackTermination, ExecutionSegment, ExecutionSegmentToken, ExecutionSession,
    ExecutionSessionPublicationError,
};

use super::callback::{CALLBACK_STYLE_DOMAIN, CALLBACK_TRANSFORM_DOMAIN};

/// Failure to atomically release one completed animation segment into authored state.
#[derive(Clone, Debug, PartialEq)]
pub enum ExecutionSegmentCompletionError {
    ForeignSegment {
        expected: RuntimeIdentity,
        actual: RuntimeIdentity,
    },
    NoPendingCompletion,
    StaleSegment {
        expected: ExecutionSegmentToken,
        actual: ExecutionSegmentToken,
    },
    NotAtBoundary {
        expected: f64,
        actual: f64,
    },
    RequiredCallbackPending,
    CallbackNotCoherent,
    CallbackTerminated(CallbackTermination),
    /// A host-modified domain cannot be released without guessing whether it
    /// should persist. The first reconciliation slice supports only callbacks
    /// that remain active at the endpoint.
    UnsupportedHostDriverRelease(SemanticNodeId),
    Publication(ExecutionSessionPublicationError),
}

impl std::fmt::Display for ExecutionSegmentCompletionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ForeignSegment { expected, actual } => write!(
                formatter,
                "execution segment belongs to runtime {actual:?}, expected {expected:?}"
            ),
            Self::NoPendingCompletion => {
                formatter.write_str("execution segment has no pending completion")
            }
            Self::StaleSegment { expected, actual } => write!(
                formatter,
                "execution segment token {actual:?} is stale; expected {expected:?}"
            ),
            Self::NotAtBoundary { expected, actual } => write!(
                formatter,
                "execution segment must complete at exact boundary {expected}, current time is {actual}"
            ),
            Self::RequiredCallbackPending => {
                formatter.write_str("a required callback publication is pending")
            }
            Self::CallbackNotCoherent => formatter.write_str(
                "required callbacks have not published a coherent endpoint frame",
            ),
            Self::CallbackTerminated(termination) => {
                write!(formatter, "required callback progression terminated: {termination:?}")
            }
            Self::UnsupportedHostDriverRelease(object) => write!(
                formatter,
                "host driver release for semantic object {}:{} is ambiguous",
                object.slot(),
                object.generation()
            ),
            Self::Publication(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for ExecutionSegmentCompletionError {}

impl From<ExecutionSessionPublicationError> for ExecutionSegmentCompletionError {
    fn from(value: ExecutionSessionPublicationError) -> Self {
        Self::Publication(value)
    }
}

impl ExecutionSession {
    /// Publish one segment's exact authored endpoint, timeline release metadata,
    /// and any continuing callback-owned effective domains as one coherent frame.
    ///
    /// Callers first advance the segment to its exact endpoint through the callback
    /// barrier. Unsupported driver-release cases fail before semantic or runtime
    /// mutation, leaving the endpoint frame and authored store unchanged.
    pub fn complete_segment(
        &mut self,
        store: &mut SemanticStore,
        segment: ExecutionSegment,
    ) -> Result<&FrameState, ExecutionSegmentCompletionError> {
        let actual_time = self.frame().time;
        if actual_time != segment.end_time() {
            return Err(ExecutionSegmentCompletionError::NotAtBoundary {
                expected: segment.end_time(),
                actual: actual_time,
            });
        }
        if let Some(termination) = self.callback_termination {
            return Err(ExecutionSegmentCompletionError::CallbackTerminated(
                termination,
            ));
        }
        if self.pending_callback.is_some() {
            return Err(ExecutionSegmentCompletionError::RequiredCallbackPending);
        }
        if !self.callback_progression_is_coherent_at(actual_time) {
            return Err(ExecutionSegmentCompletionError::CallbackNotCoherent);
        }

        let Some(token) = segment.token() else {
            return Ok(self.frame());
        };
        let runtime = self.runtime.runtime_identity();
        if token.runtime() != runtime {
            return Err(ExecutionSegmentCompletionError::ForeignSegment {
                expected: runtime,
                actual: token.runtime(),
            });
        }
        let pending = self
            .pending_segment_completion
            .clone()
            .ok_or(ExecutionSegmentCompletionError::NoPendingCompletion)?;
        if pending.token != token {
            return Err(ExecutionSegmentCompletionError::StaleSegment {
                expected: pending.token,
                actual: token,
            });
        }
        if store.scene_revision() != pending.activation_scene_revision {
            return Err(ExecutionSegmentCompletionError::Publication(
                ExecutionSessionPublicationError::StaleSceneRevision {
                    expected: pending.activation_scene_revision,
                    actual: store.scene_revision(),
                },
            ));
        }

        let mut semantic = SemanticMutationTransaction::new();
        let mut release = Vec::with_capacity(pending.entries.len());
        for entry in &pending.entries {
            semantic.set_property(
                entry.semantic_object,
                entry.semantic_property,
                entry.completion_value.clone(),
            );
            release.push(ExecutionPatch::ReconcileTrack {
                track: entry.track,
                object: entry.execution_object,
                property: entry.property,
                end_time: entry.end_time,
            });
        }

        let mut effective = Vec::new();
        let mut seen = BTreeSet::new();
        for entry in &pending.entries {
            if !seen.insert(entry.semantic_object) {
                continue;
            }
            let Some(domains) = self.last_callback_receipt.as_ref().and_then(|receipt| {
                receipt.domains_at(
                    entry.semantic_object,
                    actual_time,
                    self.publication_context(),
                )
            }) else {
                continue;
            };
            if !self
                .callback_schedule
                .continues_for_target(entry.semantic_object)
            {
                return Err(
                    ExecutionSegmentCompletionError::UnsupportedHostDriverRelease(
                        entry.semantic_object,
                    ),
                );
            }
            let object = self
                .effective_semantic_object(store, entry.semantic_object)?
                .object;
            if domains & CALLBACK_TRANSFORM_DOMAIN != 0 {
                effective.push(EffectivePropertyWrite::Transform {
                    object: entry.execution_object,
                    transform: object.transform,
                });
            }
            if domains & CALLBACK_STYLE_DOMAIN != 0 {
                effective.push(EffectivePropertyWrite::Style {
                    object: entry.execution_object,
                    style: object.style,
                });
            }
        }
        let effective = self
            .runtime
            .prepare_effective_property_batch(&effective)
            .map_err(|error| {
                ExecutionSegmentCompletionError::Publication(
                    ExecutionSessionPublicationError::Runtime(error.into()),
                )
            })?;

        self.apply_semantic_transaction_with_execution(store, semantic, release, Some(effective))?;
        self.pending_segment_completion = None;
        self.last_callback_receipt = None;
        let publication = self.publication_context();
        self.callback_schedule
            .carry_completed_publication(actual_time, publication);
        Ok(self.frame())
    }
}

#[cfg(test)]
mod tests {
    use noon_core::{
        AnimationOptions, HostCallbackId, RateFunction, SemanticMutationTransaction,
        SemanticObjectProperty, SemanticObjectState, SemanticStore, SemanticVec3, StoredGeometry,
        Transform2D, Vec2,
    };

    use super::*;

    #[test]
    fn completion_publishes_endpoint_and_releases_history_for_later_authored_writes() {
        let mut store = SemanticStore::new();
        let object =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        store.attach_to_scene(object).unwrap();
        let mut target_state = store.semantic_object_state_checked(object).unwrap().clone();
        target_state.transform.translation = SemanticVec3::new(4.0, 0.0, 0.0);
        let target = store.insert_semantic_object(target_state);
        let animation = store
            .insert_semantic_transform_animation(object, target, AnimationOptions::new())
            .unwrap();
        let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
        let segment = session
            .activate_animation_segment(
                &store,
                animation,
                AnimationOptions::new()
                    .run_time(1.0)
                    .rate_func(RateFunction::Linear),
            )
            .unwrap();

        session.advance_segment_to(segment, 1.0).unwrap();
        assert!(!session.segment_state(segment).is_complete());
        session.complete_segment(&mut store, segment).unwrap();
        assert!(session.segment_state(segment).is_complete());
        assert_eq!(
            store
                .semantic_object_state_checked(object)
                .unwrap()
                .transform
                .translation,
            SemanticVec3::new(4.0, 0.0, 0.0)
        );

        let mut set_after_play = SemanticMutationTransaction::new();
        set_after_play.set_property(
            object,
            SemanticObjectProperty::Translation,
            SemanticVec3::new(9.0, 0.0, 0.0),
        );
        session
            .apply_semantic_transaction(&mut store, set_after_play)
            .unwrap();
        session.advance_to(2.0).unwrap();
        assert_eq!(session.frame().objects[0].transform.translation.x, 9.0);

        session.seek(0.5).unwrap();
        assert_eq!(session.frame().objects[0].transform.translation.x, 2.0);
        session.seek(2.0).unwrap();
        assert_eq!(session.frame().objects[0].transform.translation.x, 9.0);
    }

    #[test]
    fn cloned_session_rejects_an_original_segment_without_mutation() {
        let mut store = SemanticStore::new();
        let object =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        store.attach_to_scene(object).unwrap();
        let mut target_state = store.semantic_object_state_checked(object).unwrap().clone();
        target_state.transform.translation = SemanticVec3::new(1.0, 0.0, 0.0);
        let target = store.insert_semantic_object(target_state);
        let animation = store
            .insert_semantic_transform_animation(object, target, AnimationOptions::new())
            .unwrap();
        let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
        let segment = session
            .activate_animation_segment(
                &store,
                animation,
                AnimationOptions::new().rate_func(RateFunction::Linear),
            )
            .unwrap();
        session
            .advance_segment_to(segment, segment.end_time())
            .unwrap();
        let mut clone = session.clone();
        let before = clone.frame().clone();

        assert!(matches!(
            clone.complete_segment(&mut store, segment),
            Err(ExecutionSegmentCompletionError::ForeignSegment { .. })
        ));
        assert_eq!(clone.frame(), &before);
    }

    #[test]
    fn active_callback_endpoint_is_carried_with_authored_timeline_completion() {
        let mut store = SemanticStore::new();
        let object =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        store.attach_to_scene(object).unwrap();
        let mut updater = SemanticMutationTransaction::new();
        updater.add_updater(object, HostCallbackId::new(1), 0.0, None);
        updater.apply(&mut store).unwrap();
        let mut target_state = store.semantic_object_state_checked(object).unwrap().clone();
        target_state.transform.translation = SemanticVec3::new(4.0, 0.0, 0.0);
        let target = store.insert_semantic_object(target_state);
        let animation = store
            .insert_semantic_transform_animation(object, target, AnimationOptions::new())
            .unwrap();
        let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
        let segment = session
            .activate_animation_segment(
                &store,
                animation,
                AnimationOptions::new()
                    .run_time(1.0)
                    .rate_func(RateFunction::Linear),
            )
            .unwrap();

        let initial = match session.advance_to_callback_barrier(1.0).unwrap() {
            crate::CallbackAdvance::HostRequired { overlay, .. } => overlay,
            crate::CallbackAdvance::Ready(_) => panic!("time-zero callback phase is required"),
        };
        session
            .commit_required_callback_phase(initial.finish())
            .unwrap();
        let mut endpoint = match session.advance_to_callback_barrier(1.0).unwrap() {
            crate::CallbackAdvance::HostRequired { overlay, .. } => overlay,
            crate::CallbackAdvance::Ready(_) => panic!("endpoint callback phase is required"),
        };
        endpoint
            .set_transform(
                object,
                Transform2D {
                    translation: Vec2::new(5.0, 1.0),
                    ..Transform2D::IDENTITY
                },
            )
            .unwrap();
        session
            .commit_required_callback_phase(endpoint.finish())
            .unwrap();

        session.complete_segment(&mut store, segment).unwrap();
        assert_eq!(
            store
                .semantic_object_state_checked(object)
                .unwrap()
                .transform
                .translation,
            SemanticVec3::new(4.0, 0.0, 0.0)
        );
        assert_eq!(
            session.frame().objects[0].transform.translation,
            Vec2::new(5.0, 1.0)
        );
        assert!(session.segment_state(segment).is_complete());
        assert!(matches!(
            session.advance_to_callback_barrier(1.0).unwrap(),
            crate::CallbackAdvance::Ready(_)
        ));
    }
}
