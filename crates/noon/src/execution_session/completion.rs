use std::collections::{BTreeMap, BTreeSet};

use noon_compile::{
    ExecutionPatch, PreparedScalarSignalTimelineError, SemanticAnimationCompletion,
};
use noon_core::{
    ReactiveValue, SemanticFadeDirection, SemanticMutationTransaction, SemanticNodeId,
    SemanticObjectProperty, SemanticSignalValue, SemanticStore,
};
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
    MissingLifecycleRoot(SemanticNodeId),
    /// A host-modified domain cannot be released without guessing whether it
    /// should persist. The first reconciliation slice supports only callbacks
    /// that remain active at the endpoint.
    UnsupportedHostDriverRelease(SemanticNodeId),
    ScalarEffectiveValue {
        signal: SemanticNodeId,
        expected: f32,
        actual: Option<ReactiveValue>,
    },
    PreparedScalarTimeline(PreparedScalarSignalTimelineError),
    ScalarTimeline(super::SignalTimelineAppendError),
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
            Self::MissingLifecycleRoot(object) => write!(
                formatter,
                "fade completion for semantic object {}:{} has no execution root",
                object.slot(),
                object.generation()
            ),
            Self::UnsupportedHostDriverRelease(object) => write!(
                formatter,
                "host driver release for semantic object {}:{} is ambiguous",
                object.slot(),
                object.generation()
            ),
            Self::ScalarEffectiveValue {
                signal,
                expected,
                actual,
            } => write!(
                formatter,
                "scalar signal {}:{} must equal endpoint {expected} before completion, found {actual:?}",
                signal.slot(),
                signal.generation()
            ),
            Self::PreparedScalarTimeline(error) => error.fmt(formatter),
            Self::ScalarTimeline(error) => error.fmt(formatter),
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
        let token = segment.token();
        if let Some(token) = token {
            let runtime = self.runtime.runtime_identity();
            if token.runtime() != runtime {
                return Err(ExecutionSegmentCompletionError::ForeignSegment {
                    expected: runtime,
                    actual: token.runtime(),
                });
            }
        }
        if store.identity() != self.store_identity {
            return Err(ExecutionSegmentCompletionError::Publication(
                ExecutionSessionPublicationError::ForeignSemanticStore,
            ));
        }
        let expected_revision = self.publication_context().scene_revision();
        let actual_revision = store.scene_revision();
        if actual_revision != expected_revision {
            return Err(ExecutionSegmentCompletionError::Publication(
                ExecutionSessionPublicationError::StaleSceneRevision {
                    expected: expected_revision,
                    actual: actual_revision,
                },
            ));
        }
        if token.is_some_and(|token| self.segment_was_completed(token)) {
            return Ok(self.frame());
        }
        if token.is_none()
            && actual_time >= segment.end_time()
            && self.callback_progression_is_coherent_at(actual_time)
        {
            return Ok(self.frame());
        }
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

        let Some(token) = token else {
            return Ok(self.frame());
        };
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

        if let crate::execution_segment::PendingSegmentCompletionKind::ScalarTrack {
            signal,
            authored_endpoint,
            runtime_endpoint,
            end_time,
        } = &pending.kind
        {
            let (signal, authored_endpoint, runtime_endpoint, end_time) =
                (*signal, *authored_endpoint, *runtime_endpoint, *end_time);
            if end_time != actual_time {
                return Err(ExecutionSegmentCompletionError::NotAtBoundary {
                    expected: end_time,
                    actual: actual_time,
                });
            }
            let effective = self.effective_signal_value(signal).cloned();
            if effective != Some(ReactiveValue::Scalar(runtime_endpoint)) {
                return Err(ExecutionSegmentCompletionError::ScalarEffectiveValue {
                    signal,
                    expected: runtime_endpoint,
                    actual: effective,
                });
            }
            let mut semantic = SemanticMutationTransaction::new();
            semantic.set_scalar_signal_at(signal, authored_endpoint, actual_time);
            let prepared = semantic.prepare(store).map_err(|error| {
                ExecutionSegmentCompletionError::Publication(
                    ExecutionSessionPublicationError::Semantic(error),
                )
            })?;
            let timeline_entry = noon_compile::lower_prepared_scalar_signal_timeline_entry(
                &prepared,
                &self.reactive_projection,
            )
            .map_err(ExecutionSegmentCompletionError::PreparedScalarTimeline)?;
            let schedule = self
                .signal_timeline
                .prepare_append(timeline_entry, actual_time)
                .map_err(ExecutionSegmentCompletionError::ScalarTimeline)?;
            let runtime_publication = self
                .runtime
                .prepare_authored_plan_change(
                    self.publication_context(),
                    prepared.proposed_scene_revision(),
                )
                .map_err(|error| {
                    ExecutionSegmentCompletionError::Publication(
                        ExecutionSessionPublicationError::Runtime(error),
                    )
                })?;

            let (_result, store) = prepared.commit_with_store();
            self.signal_timeline.commit_append(schedule);
            self.runtime
                .apply_prepared_authored_plan_change(runtime_publication)
                .expect("scalar completion publication was preflighted under exclusive ownership");
            debug_assert_eq!(
                store.scene_revision(),
                self.publication_context().scene_revision()
            );
            self.pending_segment_completion = None;
            self.completed_segment_sequence = Some(token.sequence());
            self.last_callback_receipt = None;
            let publication = self.publication_context();
            self.callback_schedule
                .carry_completed_publication(actual_time, publication);
            return Ok(self.frame());
        }

        let crate::execution_segment::PendingSegmentCompletionKind::ObjectTracks {
            lifecycle_root,
            entries,
        } = &pending.kind
        else {
            unreachable!("scalar completion returned above")
        };

        let mut semantic = SemanticMutationTransaction::new();
        let mut release = Vec::with_capacity(entries.len());
        for entry in entries {
            if matches!(
                &entry.completion,
                SemanticAnimationCompletion::Fade {
                    direction: SemanticFadeDirection::Out
                }
            ) {
                let root = lifecycle_root.ok_or(
                    ExecutionSegmentCompletionError::MissingLifecycleRoot(entry.semantic_object),
                )?;
                semantic.remove_member(root, entry.semantic_object);
            }
        }

        // Paint channels carry only their exact semantic fields. Start from the
        // current authored style once per affected object, merge every completed
        // style domain, then emit one replacement so parallel paint channels cannot
        // overwrite each other or unrelated style fields.
        let mut completed_styles = BTreeMap::new();
        for entry in entries {
            let is_style_domain = matches!(
                &entry.completion,
                SemanticAnimationCompletion::Fill { .. }
                    | SemanticAnimationCompletion::Stroke { .. }
                    | SemanticAnimationCompletion::Property {
                        property: SemanticObjectProperty::ObjectOpacity,
                        ..
                    }
            );
            if !is_style_domain {
                continue;
            }
            let style = completed_styles
                .entry(entry.semantic_object)
                .or_insert_with(|| {
                    store
                        .semantic_object_state_checked(entry.semantic_object)
                        .expect("pending completion object remains live at the same scene revision")
                        .style
                        .clone()
                });
            match &entry.completion {
                SemanticAnimationCompletion::Fill { paint, opacity } => {
                    style.fill = paint.clone();
                    style.fill_opacity = *opacity;
                }
                SemanticAnimationCompletion::Stroke { paint, opacity } => {
                    style.stroke = paint.clone();
                    style.stroke_opacity = *opacity;
                }
                SemanticAnimationCompletion::Property {
                    property: SemanticObjectProperty::ObjectOpacity,
                    value: SemanticSignalValue::Scalar(value),
                } => style.object_opacity = *value,
                _ => unreachable!("style-domain completion was classified above"),
            };
        }
        for (object, style) in &completed_styles {
            semantic.replace_style(*object, style.clone());
        }

        for entry in entries {
            match &entry.completion {
                SemanticAnimationCompletion::Property { property, value } => {
                    if *property != SemanticObjectProperty::ObjectOpacity {
                        semantic.set_property(entry.semantic_object, *property, value.clone());
                    }
                }
                SemanticAnimationCompletion::ContentMorph { content } => {
                    semantic.replace_content(entry.semantic_object, *content);
                }
                SemanticAnimationCompletion::Fill { .. }
                | SemanticAnimationCompletion::Stroke { .. }
                | SemanticAnimationCompletion::Fade { .. }
                | SemanticAnimationCompletion::Create => {}
            }
            release.push(ExecutionPatch::ReconcileTrack {
                track: entry.track,
                object: entry.execution_object,
                property: entry.property,
                end_time: entry.end_time,
            });
        }

        let mut effective = Vec::new();
        let mut seen = BTreeSet::new();
        for entry in entries {
            if !seen.insert(entry.semantic_object) {
                continue;
            }
            let domains = self.last_callback_receipt.as_ref().and_then(|receipt| {
                receipt.domains_at(
                    entry.semantic_object,
                    actual_time,
                    self.publication_context(),
                )
            });
            if self
                .runtime
                .object_has_effective_driver(entry.execution_object)
                && domains.is_none()
            {
                return Err(
                    ExecutionSegmentCompletionError::UnsupportedHostDriverRelease(
                        entry.semantic_object,
                    ),
                );
            }
            let Some(domains) = domains else {
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
        self.completed_segment_sequence = Some(token.sequence());
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
        AnimationOptions, Color, HostCallbackId, RateFunction, SemanticMutationTransaction,
        SemanticObjectProperty, SemanticObjectState, SemanticPaint, SemanticStore, SemanticStyle,
        SemanticVec3, StoredGeometry, TrackTiming, Transform2D, Vec2,
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
        let repeated_publication = session.publication_context();
        session.complete_segment(&mut store, segment).unwrap();
        assert_eq!(session.frame().time, 2.0);
        assert_eq!(session.publication_context(), repeated_publication);

        session.seek(0.5).unwrap();
        assert_eq!(session.frame().objects[0].transform.translation.x, 2.0);
        assert!(session.segment_state(segment).is_complete());
        session.seek(2.0).unwrap();
        assert_eq!(session.frame().objects[0].transform.translation.x, 9.0);
    }

    #[test]
    fn analytic_content_morph_replaces_authored_endpoint_and_releases_render_pair() {
        let mut store = SemanticStore::new();
        let object =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        store.attach_to_scene(object).unwrap();
        let mut target_state = SemanticObjectState::new(StoredGeometry::Rectangle {
            size: Vec2::new(2.0, 2.0),
        });
        target_state.transform.translation = SemanticVec3::new(3.0, -1.0, 0.0);
        target_state.style.fill = Some(SemanticPaint::Solid(Color::RED));
        target_state.style.fill_opacity = 0.5;
        let expected = target_state.clone();
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
                    .run_time(2.0)
                    .rate_func(RateFunction::Linear),
            )
            .unwrap();

        session.advance_segment_to(segment, 1.0).unwrap();
        assert!((session.frame().morph(0) - 0.5).abs() < 1e-6);
        assert!(matches!(
            session.frame().render_geometry(0),
            Some(noon_core::GeometryRef::VectorPath(path)) if path.morph_target().is_some()
        ));
        assert!(matches!(
            store.semantic_object_state_checked(object).unwrap().content,
            noon_core::SemanticObjectContent::Geometry(StoredGeometry::Circle { .. })
        ));

        session.advance_segment_to(segment, 2.0).unwrap();
        session.complete_segment(&mut store, segment).unwrap();
        let authored = store.semantic_object_state_checked(object).unwrap();
        assert_eq!(authored.content, expected.content);
        assert_eq!(authored.transform, expected.transform);
        assert_eq!(authored.style, expected.style);
        assert_eq!(session.frame().morph(0), 0.0);
        assert!(session.frame().render_geometries[0].is_none());
        assert!(session.frame().render_transforms[0].is_none());
        assert!(matches!(
            session.frame().render_geometry(0),
            Some(noon_core::GeometryRef::Rectangle { .. })
        ));
    }

    #[test]
    fn paint_completion_coalesces_exact_fields_and_releases_all_channels() {
        let mut store = SemanticStore::new();
        let object =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        store.attach_to_scene(object).unwrap();
        let mut target_state = store.semantic_object_state_checked(object).unwrap().clone();
        target_state.style.fill = Some(SemanticPaint::Solid(Color::RED));
        target_state.style.fill_opacity = 0.4;
        target_state.style.stroke = Some(SemanticPaint::Solid(Color::GREEN));
        target_state.style.stroke_opacity = 0.25;
        target_state.style.object_opacity = 0.5;
        let expected = target_state.style.clone();
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
                    .run_time(2.0)
                    .rate_func(RateFunction::Linear),
            )
            .unwrap();

        session.advance_segment_to(segment, 1.0).unwrap();
        let style = session.frame().objects[0].style;
        let fill = style.fill.unwrap();
        let stroke = style.stroke.unwrap();
        assert!((fill.alpha - 0.7).abs() < 1e-6);
        assert_eq!(
            stroke,
            Color {
                alpha: 0.125,
                ..Color::GREEN
            }
        );
        assert!((style.opacity - 0.75).abs() < 1e-6);

        session.advance_segment_to(segment, 2.0).unwrap();
        session.complete_segment(&mut store, segment).unwrap();
        assert_eq!(
            store.semantic_object_state_checked(object).unwrap().style,
            expected
        );

        let mut after = expected.clone();
        after.fill = Some(SemanticPaint::Solid(Color::BLUE));
        let mut authored = SemanticMutationTransaction::new();
        authored.replace_style(object, after);
        session
            .apply_semantic_transaction(&mut store, authored)
            .unwrap();
        assert_eq!(
            session.frame().objects[0].style.fill,
            Some(Color {
                alpha: 0.4,
                ..Color::BLUE
            })
        );
        assert_eq!(session.frame().objects[0].style.opacity, 0.5);
        assert_eq!(
            session.frame().objects[0].style.stroke,
            Some(Color {
                alpha: 0.25,
                ..Color::GREEN
            })
        );
    }

    #[test]
    fn fill_completion_preserves_bound_opacity_and_stroke_siblings() {
        let mut store = SemanticStore::new();
        let mut source = SemanticObjectState::new(StoredGeometry::Circle { radius: 1.0 });
        source.style.stroke = Some(SemanticPaint::Solid(Color::WHITE));
        source.style.stroke_opacity = 1.0;
        let object = store.insert_semantic_object(source);
        store.attach_to_scene(object).unwrap();
        let object_opacity = store.insert_semantic_input_signal(0.65_f64).unwrap();
        store
            .bind_semantic_signal(
                object_opacity,
                object,
                SemanticObjectProperty::ObjectOpacity,
            )
            .unwrap();

        let mut target = store.semantic_object_state_checked(object).unwrap().clone();
        target.style.fill = Some(SemanticPaint::Solid(Color::RED));
        target.style.fill_opacity = 0.4;
        let target = store.insert_semantic_object(target);
        let animation = store
            .insert_semantic_transform_animation(object, target, AnimationOptions::new())
            .unwrap();
        let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
        let segment = session
            .activate_animation_segment(
                &store,
                animation,
                AnimationOptions::new()
                    .run_time(2.0)
                    .rate_func(RateFunction::Linear),
            )
            .unwrap();

        session.advance_segment_to(segment, 2.0).unwrap();
        session.complete_segment(&mut store, segment).unwrap();

        let authored = &store.semantic_object_state_checked(object).unwrap().style;
        assert_eq!(authored.fill, Some(SemanticPaint::Solid(Color::RED)));
        assert_eq!(authored.fill_opacity, 0.4);
        assert_eq!(authored.stroke, Some(SemanticPaint::Solid(Color::WHITE)));
        assert_eq!(authored.stroke_opacity, 1.0);
        let effective = session.frame().objects[0].style;
        assert_eq!(
            effective.fill,
            Some(Color {
                alpha: 0.4,
                ..Color::RED
            })
        );
        assert_eq!(authored.object_opacity, 1.0);
        assert_eq!(effective.opacity, 0.65);
        assert_eq!(effective.stroke, Some(Color::WHITE));
    }

    #[test]
    fn sequence_completion_uses_mapped_finish_and_releases_disjoint_style_channels() {
        let mut store = SemanticStore::new();
        let object =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        store.attach_to_scene(object).unwrap();

        let mut fill_target = store.semantic_object_state_checked(object).unwrap().clone();
        fill_target.style.fill = Some(SemanticPaint::Solid(Color::RED));
        fill_target.style.fill_opacity = 0.4;
        let fill_target = store.insert_semantic_object(fill_target);
        let fill = store
            .insert_semantic_transform_animation(
                object,
                fill_target,
                AnimationOptions::new()
                    .run_time(1.0)
                    .rate_func(RateFunction::Linear),
            )
            .unwrap();

        let mut opacity_target = store.semantic_object_state_checked(object).unwrap().clone();
        opacity_target.style.object_opacity = 0.5;
        let opacity_target = store.insert_semantic_object(opacity_target);
        let opacity = store
            .insert_semantic_transform_animation(
                object,
                opacity_target,
                AnimationOptions::new()
                    .run_time(1.0)
                    .rate_func(RateFunction::Linear),
            )
            .unwrap();
        let sequence = store
            .insert_semantic_sequence_animation(
                &[fill, opacity],
                AnimationOptions::new().rate_func(RateFunction::Linear),
            )
            .unwrap();

        let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
        let authored = store
            .semantic_object_state_checked(object)
            .unwrap()
            .style
            .clone();
        let segment = session
            .activate_animation_segment(
                &store,
                sequence,
                AnimationOptions::new()
                    .run_time(2.0)
                    .rate_func(RateFunction::Linear),
            )
            .unwrap();
        assert_eq!(segment.end_time(), 2.0);

        session.advance_segment_to(segment, 1.0).unwrap();
        let first_boundary = session.frame().objects[0].style;
        assert_eq!(
            first_boundary.fill,
            Some(Color {
                alpha: 0.4,
                ..Color::RED
            })
        );
        assert_eq!(first_boundary.opacity, 1.0);
        let before_early_completion = session.publication_context();
        assert!(matches!(
            session.complete_segment(&mut store, segment),
            Err(ExecutionSegmentCompletionError::NotAtBoundary {
                expected: 2.0,
                actual: 1.0,
            })
        ));
        assert_eq!(session.publication_context(), before_early_completion);
        assert_eq!(
            store.semantic_object_state_checked(object).unwrap().style,
            authored
        );

        session.seek(0.5).unwrap();
        let half_first = session.frame().objects[0].style;
        assert!((half_first.fill.unwrap().alpha - 0.7).abs() < 1e-6);
        assert_eq!(half_first.opacity, 1.0);
        session.advance_to(1.0).unwrap();
        assert_eq!(session.frame().objects[0].style, first_boundary);

        session.advance_segment_to(segment, 2.0).unwrap();
        let endpoint = session.frame().objects[0].style;
        assert_eq!(
            endpoint.fill,
            Some(Color {
                alpha: 0.4,
                ..Color::RED
            })
        );
        assert_eq!(endpoint.opacity, 0.5);
        let endpoint_publication = session.publication_context();
        session.complete_segment(&mut store, segment).unwrap();
        assert_eq!(session.frame().objects[0].style, endpoint);
        assert_eq!(
            store.semantic_object_state_checked(object).unwrap().style,
            SemanticStyle {
                fill: Some(SemanticPaint::Solid(Color::RED)),
                fill_opacity: 0.4,
                object_opacity: 0.5,
                ..authored
            }
        );
        assert_eq!(
            session.publication_context().scene_revision(),
            endpoint_publication
                .scene_revision()
                .checked_next()
                .unwrap()
        );
        assert_eq!(
            session.publication_context().execution_revision(),
            endpoint_publication
                .execution_revision()
                .checked_next()
                .unwrap()
        );
        assert_eq!(
            session.publication_context().frame_epoch(),
            endpoint_publication.frame_epoch().checked_next().unwrap()
        );
    }

    #[test]
    fn parallel_mapped_leaves_seek_deterministically_and_release_atomically() {
        let mut store = SemanticStore::new();
        let left = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
            radius: 0.5,
        }));
        let right =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 0.5,
            }));
        store.attach_to_scene(left).unwrap();
        store.attach_to_scene(right).unwrap();

        let mut left_target = store.semantic_object_state_checked(left).unwrap().clone();
        left_target.transform.translation = SemanticVec3::new(-2.0, 1.0, 0.0);
        let left_target = store.insert_semantic_object(left_target);
        let mut right_target = store.semantic_object_state_checked(right).unwrap().clone();
        right_target.transform.translation = SemanticVec3::new(2.0, -1.0, 0.0);
        let right_target = store.insert_semantic_object(right_target);
        let leaf_options = AnimationOptions::new()
            .run_time(1.0)
            .rate_func(RateFunction::Linear);
        let left_animation = store
            .insert_semantic_transform_animation(left, left_target, leaf_options)
            .unwrap();
        let right_animation = store
            .insert_semantic_transform_animation(right, right_target, leaf_options)
            .unwrap();
        let parallel = store
            .insert_semantic_parallel_animation(
                &[left_animation, right_animation],
                AnimationOptions::new().rate_func(RateFunction::ThereAndBack),
            )
            .unwrap();

        let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
        let segment = session
            .activate_animation_segment(
                &store,
                parallel,
                // Keep the root's authored ThereAndBack rate function; a play
                // rate override would intentionally replace that mapping.
                AnimationOptions::new().run_time(2.0),
            )
            .unwrap();
        session.advance_segment_to(segment, 0.5).unwrap();
        let forward = [
            session.frame().objects[0].transform,
            session.frame().objects[1].transform,
        ];
        assert_eq!(forward[0].translation, Vec2::new(-1.0, 0.5));
        assert_eq!(forward[1].translation, Vec2::new(1.0, -0.5));

        session.seek(0.25).unwrap();
        session.advance_to(0.5).unwrap();
        assert_eq!(session.frame().objects[0].transform, forward[0]);
        assert_eq!(session.frame().objects[1].transform, forward[1]);

        session.advance_segment_to(segment, 1.0).unwrap();
        assert_eq!(
            session.frame().objects[0].transform.translation,
            Vec2::new(-2.0, 1.0)
        );
        assert_eq!(
            session.frame().objects[1].transform.translation,
            Vec2::new(2.0, -1.0)
        );
        session.advance_segment_to(segment, 1.5).unwrap();
        assert_eq!(session.frame().objects[0].transform, forward[0]);
        assert_eq!(session.frame().objects[1].transform, forward[1]);

        // ThereAndBack ordinarily maps alpha one back to zero. The execution
        // endpoint follows the runtime finish contract and settles both leaves
        // to their exact targets before reconciliation releases the drivers.
        session.advance_segment_to(segment, 2.0).unwrap();
        assert_eq!(
            session.frame().objects[0].transform.translation,
            Vec2::new(-2.0, 1.0)
        );
        assert_eq!(
            session.frame().objects[1].transform.translation,
            Vec2::new(2.0, -1.0)
        );
        let endpoint_publication = session.publication_context();
        session.complete_segment(&mut store, segment).unwrap();
        assert_eq!(
            session.frame().objects[0].transform.translation,
            Vec2::new(-2.0, 1.0)
        );
        assert_eq!(
            session.frame().objects[1].transform.translation,
            Vec2::new(2.0, -1.0)
        );
        assert_eq!(
            store
                .semantic_object_state_checked(left)
                .unwrap()
                .transform
                .translation,
            SemanticVec3::new(-2.0, 1.0, 0.0)
        );
        assert_eq!(
            store
                .semantic_object_state_checked(right)
                .unwrap()
                .transform
                .translation,
            SemanticVec3::new(2.0, -1.0, 0.0)
        );
        let completed = session.publication_context();
        assert_eq!(
            completed.scene_revision(),
            endpoint_publication
                .scene_revision()
                .checked_next()
                .unwrap()
        );
        assert_eq!(
            completed.execution_revision(),
            endpoint_publication
                .execution_revision()
                .checked_next()
                .unwrap()
        );
        assert_eq!(
            completed.frame_epoch(),
            endpoint_publication.frame_epoch().checked_next().unwrap()
        );
    }

    #[test]
    fn scalar_timeline_preserves_forward_seek_after_released_affine_history() {
        let mut store = SemanticStore::new();
        let object =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        store.attach_to_scene(object).unwrap();
        let tracker = store.insert_semantic_input_signal(0.0_f64).unwrap();
        store
            .bind_semantic_signal(tracker, object, SemanticObjectProperty::RotationZ)
            .unwrap();
        let mut scalar_track = SemanticMutationTransaction::new();
        scalar_track.add_scalar_signal_track(
            tracker,
            0.0,
            2.0,
            TrackTiming::new(0.0, 2.0, RateFunction::Linear),
        );
        scalar_track.apply(&mut store).unwrap();

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
        session.complete_segment(&mut store, segment).unwrap();

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

        session.seek(0.5).unwrap();
        assert_eq!(session.frame().objects[0].transform.translation.x, 2.0);
        assert_eq!(session.frame().objects[0].transform.rotation, 0.5);
        session.seek(2.0).unwrap();
        let explicit_seek = session.frame().objects[0].clone();
        assert_eq!(explicit_seek.transform.translation.x, 9.0);
        assert_eq!(explicit_seek.transform.rotation, 2.0);

        session.seek(0.5).unwrap();
        session.advance_to(2.0).unwrap();
        assert_eq!(session.frame().objects[0], explicit_seek);
    }

    #[test]
    fn completed_segment_rejects_an_out_of_band_store_revision() {
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
        session
            .advance_segment_to(segment, segment.end_time())
            .unwrap();
        session.complete_segment(&mut store, segment).unwrap();

        let expected = session.publication_context().scene_revision();
        let mut out_of_band = SemanticMutationTransaction::new();
        out_of_band.set_property(
            object,
            SemanticObjectProperty::Translation,
            SemanticVec3::new(8.0, 0.0, 0.0),
        );
        out_of_band.apply(&mut store).unwrap();
        let actual = store.scene_revision();
        let publication = session.publication_context();
        let frame = session.frame().clone();

        assert_eq!(
            session.complete_segment(&mut store, segment),
            Err(ExecutionSegmentCompletionError::Publication(
                ExecutionSessionPublicationError::StaleSceneRevision { expected, actual }
            ))
        );
        assert_eq!(session.publication_context(), publication);
        assert_eq!(session.frame(), &frame);
    }

    #[test]
    fn wait_completion_rejects_an_out_of_band_store_revision() {
        let mut store = SemanticStore::new();
        let object =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        store.attach_to_scene(object).unwrap();
        let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
        let wait = session.wait_segment(1.0).unwrap();
        session.advance_segment_to(wait, wait.end_time()).unwrap();

        let expected = session.publication_context().scene_revision();
        let mut out_of_band = SemanticMutationTransaction::new();
        out_of_band.set_property(
            object,
            SemanticObjectProperty::Translation,
            SemanticVec3::new(2.0, 0.0, 0.0),
        );
        out_of_band.apply(&mut store).unwrap();
        let actual = store.scene_revision();
        let publication = session.publication_context();
        let frame = session.frame().clone();

        assert_eq!(
            session.complete_segment(&mut store, wait),
            Err(ExecutionSegmentCompletionError::Publication(
                ExecutionSessionPublicationError::StaleSceneRevision { expected, actual }
            ))
        );
        assert_eq!(session.publication_context(), publication);
        assert_eq!(session.frame(), &frame);
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
    fn zero_length_wait_cannot_complete_before_its_required_callback() {
        let mut store = SemanticStore::new();
        let object =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        store.attach_to_scene(object).unwrap();
        let mut updater = SemanticMutationTransaction::new();
        updater.add_updater(object, HostCallbackId::new(1), 0.0, None);
        updater.apply(&mut store).unwrap();
        let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
        let wait = session.wait_segment(0.0).unwrap();

        assert_eq!(
            session.complete_segment(&mut store, wait).unwrap_err(),
            ExecutionSegmentCompletionError::CallbackNotCoherent,
        );
        let overlay = match session.advance_to_callback_barrier(0.0).unwrap() {
            crate::CallbackAdvance::HostRequired { overlay, .. } => overlay,
            crate::CallbackAdvance::Ready(_) => panic!("time-zero callback phase is required"),
        };
        assert_eq!(
            session.complete_segment(&mut store, wait).unwrap_err(),
            ExecutionSegmentCompletionError::RequiredCallbackPending,
        );
        session
            .commit_required_callback_phase(overlay.finish())
            .unwrap();
        session.complete_segment(&mut store, wait).unwrap();
        assert!(session.segment_state(wait).is_complete());
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

    #[test]
    fn unreceipted_effective_driver_rejects_completion_atomically() {
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
        session
            .advance_segment_to(segment, segment.end_time())
            .unwrap();
        let mut overlay = session
            .begin_required_callback_phase(segment.end_time(), [object])
            .unwrap();
        overlay
            .set_transform(
                object,
                Transform2D {
                    translation: Vec2::new(5.0, 1.0),
                    ..Transform2D::IDENTITY
                },
            )
            .unwrap();
        session
            .commit_required_callback_phase(overlay.finish())
            .unwrap();
        session.last_callback_receipt = None;
        let revision = store.scene_revision();
        let publication = session.publication_context();
        let frame = session.frame().clone();

        assert_eq!(
            session.complete_segment(&mut store, segment),
            Err(ExecutionSegmentCompletionError::UnsupportedHostDriverRelease(object))
        );
        assert_eq!(store.scene_revision(), revision);
        assert_eq!(session.publication_context(), publication);
        assert_eq!(session.frame(), &frame);
        assert!(!session.segment_state(segment).is_complete());
    }
}
