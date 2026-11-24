//! Experimental source-owner admission; presentation remains independent.
use super::{ExecutionSession, SourceExecutionRequirement};
use crate::ExecutionSegment;
use noon_runtime::{PrefixBatch, PrefixError, PrefixPublisher};

mod reader;
pub use reader::PrefixReader;

#[cfg(test)]
#[test]
fn reserve_and_append_sampled_tracker_suffix_while_paused() {
    use crate::Scene;
    use noon_core::{RateFunction, Vec2};
    use noon_runtime::ReplayLimits;

    let mut scene = Scene::new();
    let target = scene.circle(0.5).unwrap();
    let tracker = scene.value_tracker(0.0).unwrap();
    scene.add(&target).unwrap();
    scene
        .bind_sampled(
            &target,
            &tracker,
            "position",
            true,
            vec![0.0, 0.0, 0.0, 2.0, 2.0, 0.0],
        )
        .unwrap();
    let mut source = scene.execution_session().unwrap();
    source
        .begin_replay_retention(ReplayLimits::default())
        .unwrap();
    let wait = source.wait_segment(1.0).unwrap();
    source.advance_to(wait.end_time()).unwrap();
    scene.live(&mut source).complete_segment(wait).unwrap();
    let (mut publisher, mut reader) = source.create_prepared_prefix(wait).unwrap();
    reader.seek(0.5).unwrap();
    let before = reader.frame().clone();

    let segment = scene
        .live(&mut source)
        .declare_and_activate_value_tracker(&tracker, 2.0, 1.0, RateFunction::Linear)
        .unwrap();
    source.advance_to(segment.end_time()).unwrap();
    scene.live(&mut source).complete_segment(segment).unwrap();
    let batch = source
        .publish_prepared_prefix(segment, &mut publisher)
        .unwrap();
    reader.reserve_append_capacity(&batch).unwrap();
    reader.append(batch).unwrap();
    assert_eq!(reader.frame(), &before);
    assert_eq!(reader.end_time(), None);
    assert_eq!(
        reader.seek(1.5).unwrap().objects[0].transform.translation,
        Vec2::new(1.0, 0.0)
    );
    assert_eq!(reader.seek(0.5).unwrap(), &before);
}

#[cfg(test)]
#[test]
fn observed_prefix_handoff_rejects_another_stream_before_releasing_history() {
    use crate::Scene;
    use noon_core::{HostCallbackId, SemanticMutationTransaction};
    use noon_runtime::ReplayLimits;
    let mut scene = Scene::new();
    let target = scene.circle(0.5).unwrap();
    scene.add(&target).unwrap();
    let mut source = scene.execution_session().unwrap();
    source
        .begin_replay_retention(ReplayLimits::default())
        .unwrap();
    let segment = source.wait_segment(1.0).unwrap();
    source.advance_to(1.0).unwrap();
    let (publisher, reader) = source.create_prepared_prefix(segment).unwrap();
    let (_, other_stream) = source.create_prepared_prefix(segment).unwrap();
    let mut transaction = SemanticMutationTransaction::new();
    transaction.add_updater(target.node_id(), HostCallbackId::new(71), 1.0, None);
    crate::integration::scene_execution::publish_at_root(
        scene.integration_store(),
        scene.root(),
        &mut source,
        transaction,
    )
    .unwrap();
    source.wait_segment(1.0).unwrap();
    let frame = source.frame().clone();
    assert_eq!(
        source.begin_observed_prefix_source(&publisher, &other_stream),
        Err(PrefixError::SourceChanged)
    );
    assert_eq!(source.frame(), &frame);
    source.validate_prepared_publisher(&publisher).unwrap();
    assert_eq!(
        source
            .begin_observed_prefix_source(&publisher, &reader)
            .unwrap(),
        Some(SourceExecutionRequirement::HostCallbacks)
    );
    assert_eq!(source.frame(), &frame);
    assert!(source.validate_prepared_publisher(&publisher).is_err());
}

#[cfg(test)]
#[test]
fn opaque_continuation_reports_callbacks_only_when_the_schedule_exists() {
    use crate::Scene;
    use noon_core::{HostCallbackId, SemanticMutationTransaction};

    let mut scene = Scene::new();
    let target = scene.circle(0.5).unwrap();
    scene.add(&target).unwrap();
    let mut source = scene.execution_session().unwrap();
    assert_eq!(
        source.continuation_execution_requirement(crate::ContinuationEffects::Opaque),
        Some(SourceExecutionRequirement::HostEffects)
    );

    let mut transaction = SemanticMutationTransaction::new();
    transaction.add_updater(target.node_id(), HostCallbackId::new(72), 1.0, None);
    crate::integration::scene_execution::publish_at_root(
        scene.integration_store(),
        scene.root(),
        &mut source,
        transaction,
    )
    .unwrap();
    assert_eq!(
        source.continuation_execution_requirement(crate::ContinuationEffects::Opaque),
        Some(SourceExecutionRequirement::HostCallbacks)
    );
}

#[cfg(all(test, feature = "native-text", feature = "bundled-fonts"))]
mod backdrop_tests;
#[cfg(all(test, feature = "native-text", feature = "bundled-fonts"))]
mod camera_prefix_tests;
#[cfg(all(test, feature = "typst", feature = "bundled-fonts"))]
mod spatial_prefix_tests;

impl ExecutionSession {
    /// Classify host source effects separately from the scene's replay domain.
    /// A quiet scene cannot prove that an arbitrary continuation is safe to run ahead.
    pub fn continuation_execution_requirement(
        &self,
        effects: crate::ContinuationEffects,
    ) -> Option<SourceExecutionRequirement> {
        if !self.callback_schedule.is_empty() {
            return Some(SourceExecutionRequirement::HostCallbacks);
        }
        match effects {
            crate::ContinuationEffects::Opaque => Some(SourceExecutionRequirement::HostEffects),
            crate::ContinuationEffects::SceneOnly => self.source_execution_requirement(),
        }
    }

    /// Check the existing source scope and (when present) its delivered cursor
    /// before completing a segment. Capability denial leaves execution untouched.
    pub fn preflight_prepared_source(
        &self,
        publisher: Option<&PrefixPublisher>,
    ) -> Result<Option<SourceExecutionRequirement>, PrefixError> {
        if let Some(requirement) = self.source_execution_requirement() {
            return Ok(Some(requirement));
        }
        match self.runtime.preflight_prefix_source(publisher) {
            Ok(()) => Ok(None),
            Err(PrefixError::Replay(
                error @ (noon_runtime::ReplayError::UnsupportedDomain
                | noon_runtime::ReplayError::UnrecordedInput
                | noon_runtime::ReplayError::RetentionLimit),
            )) => Ok(Some(SourceExecutionRequirement::ReplayUnavailable(error))),
            Err(error) => Err(error),
        }
    }

    /// Preparation keeps the canonical runtime at its unsealed authored
    /// frontier. Retiring prefix publication must not reset input/selection state,
    /// rebuild the spatial index or select a historical frame during handoff.
    pub(crate) fn retire_prepared_source_publication(&mut self) {
        assert!(
            !self.runtime.replay_is_sealed(),
            "prepared source cannot transfer sealed history"
        );
        self.runtime.retire_prefix_publication();
        if !self.runtime.replay_retention_valid() {
            self.release_replay_slots();
        }
    }

    /// Before creating any reader, retain this same source for observed execution
    /// when preparation needs callbacks, input or another unsupported domain.
    /// Retain ordinary bounded history; never sample, complete or reset input.
    pub fn begin_observed_prepared_source(
        &mut self,
        effects: crate::ContinuationEffects,
    ) -> Result<Option<SourceExecutionRequirement>, PrefixError> {
        if self.runtime.replay_is_sealed() {
            return Err(PrefixError::NotReady);
        }
        let requirement = match self.continuation_execution_requirement(effects) {
            Some(requirement) => Some(requirement),
            None => self.preflight_prepared_source(None)?,
        };
        if requirement.is_some() {
            self.retire_prepared_source_publication();
        }
        Ok(requirement)
    }

    /// Transfer preparation back to its canonical source. The matching reader
    /// contributes inspection only; contacts, clocks and input receipts stay local.
    pub fn begin_observed_prefix_source(
        &mut self,
        publisher: &PrefixPublisher,
        reader: &PrefixReader,
    ) -> Result<Option<SourceExecutionRequirement>, PrefixError> {
        self.validate_prepared_publisher(publisher)?;
        if !reader.matches_publisher(publisher)
            || reader.end_time().is_some()
            || self.frame().time != publisher.frontier()
            || self.runtime.replay_is_sealed()
        {
            return Err(PrefixError::SourceChanged);
        }
        let requirement = self.preflight_prepared_source(Some(publisher))?;
        if requirement.is_some() {
            self.inherit_prepared_inspection(reader.session());
            self.retire_prepared_source_publication();
        }
        Ok(requirement)
    }

    /// Pure completed-prefix admission remains separate from observed execution.
    pub fn validate_prepared_source_domain(&self) -> Result<(), PrefixError> {
        if self.source_execution_requirement().is_some() {
            return Err(noon_runtime::ReplayError::UnsupportedDomain.into());
        }
        Ok(())
    }

    /// Check an existing stream against this source before headless completion.
    /// A matching stream does not grant permission to bypass segment/domain gates.
    pub fn validate_prepared_publisher(
        &self,
        publisher: &PrefixPublisher,
    ) -> Result<(), PrefixError> {
        self.runtime.validate_prefix_publisher(publisher)
    }

    fn ensure_prefix_boundary(&self, boundary: ExecutionSegment) -> Result<(), PrefixError> {
        // Check the original session, never a clone (which clears pending tokens).
        // Quiet waits also need the actual source-owned deadline supplied here.
        self.ensure_replay_completion()?;
        // Historical inset roles and reactive routes need their own admission;
        // do not silently drop source-only orchestration from an interactive reader.
        self.validate_prepared_source_domain()?;
        if self.frame().time != boundary.end_time() || !self.segment_state(boundary).is_complete() {
            return Err(PrefixError::NotReady);
        }
        Ok(())
    }

    /// Experimental, opt-in initial reader at an actual completed source segment.
    /// The source owner must supply its real continuation boundary, including a
    /// wait. Presentation-only pointer input uses the existing shared session.
    /// Arbitrary callbacks, causal edits and a complete realtime player remain
    /// outside admission. The initial range is half-open.
    pub fn create_prepared_prefix(
        &self,
        boundary: ExecutionSegment,
    ) -> Result<(PrefixPublisher, PrefixReader), PrefixError> {
        self.create_prepared_prefix_with_resource_budget(
            boundary,
            noon_runtime::SceneInstance::DEFAULT_PREFIX_RESOURCE_BUDGET_BYTES,
        )
    }

    /// Bootstrap the same reader with a host-selected immutable-resource byte cap.
    /// This only bounds retained resource payloads, not total process/GPU memory.
    /// Source completion/domain admission is identical to the default bootstrap.
    pub fn create_prepared_prefix_with_resource_budget(
        &self,
        boundary: ExecutionSegment,
        resource_budget_bytes: usize,
    ) -> Result<(PrefixPublisher, PrefixReader), PrefixError> {
        self.ensure_prefix_boundary(boundary)?;
        let (mut publisher, runtime, receiver) = self
            .runtime
            .create_prefix_projection_with_resource_budget(resource_budget_bytes)?;
        publisher.initialize_scalar_timeline(&self.signal_timeline);
        Ok((publisher, self.prepared_reader(runtime, receiver)?))
    }

    /// Query availability after an actual completed source boundary, without
    /// cloning a reader. Bootstrap retains its full domain and budget admission.
    pub fn has_prepared_prefix_interval(
        &self,
        boundary: ExecutionSegment,
    ) -> Result<bool, PrefixError> {
        self.ensure_prefix_boundary(boundary)?;
        self.runtime.has_prefix_interval()
    }

    /// Create the same historical reader after explicit source completion.
    /// A sealed zero-duration scene has the exact readable range `[0, 0]`;
    /// neither an invented segment nor another source invocation is needed.
    pub fn create_finished_prepared_reader(&self) -> Result<PrefixReader, PrefixError> {
        self.ensure_replay_completion()?;
        self.validate_prepared_source_domain()?;
        let (runtime, receiver) = self.runtime.create_finished_prefix_projection()?;
        self.prepared_reader(runtime, receiver)
    }

    fn prepared_reader(
        &self,
        runtime: noon_runtime::SceneInstance,
        receiver: noon_runtime::PrefixReceiver,
    ) -> Result<PrefixReader, PrefixError> {
        // Pin every retained physical identity, including rows not live at the
        // bootstrap time. Selected membership is still proved by the runtime.
        let mut slots = noon_runtime::ExecutionSlotTable::new();
        slots
            .reserve_object_capacity(runtime.frame().objects.len())
            .map_err(PrefixError::Slots)?;
        for row in &runtime.frame().objects {
            slots.insert_object(row.id).map_err(PrefixError::Slots)?;
        }
        let mut session = Self::from_runtime_projection(
            self.store_identity.clone(),
            Default::default(),
            Default::default(),
            slots,
            self.reactive_projection.clone(),
            runtime,
            self.camera_object,
            None,
            super::super::callback::CallbackSchedule::new(Default::default()),
        );
        session.signal_timeline = self
            .signal_timeline
            .clone_for_runtime(session.runtime_identity());
        let schedule_at_reader_time = session.signal_timeline.preview_seek(session.frame().time);
        session
            .runtime
            .seek_with_scalar_timeline(&schedule_at_reader_time)?;
        session.signal_timeline.commit(schedule_at_reader_time);
        Ok(PrefixReader::new(session, receiver))
    }

    /// Publish a completed extension through the same Rust source admission.
    /// Only the new suffix is transferred; failure does not consume the cursor.
    pub fn publish_prepared_prefix(
        &self,
        boundary: ExecutionSegment,
        publisher: &mut PrefixPublisher,
    ) -> Result<PrefixBatch, PrefixError> {
        self.ensure_prefix_boundary(boundary)?;
        self.runtime
            .prepare_prefix_update_with_scalar_timeline(publisher, &self.signal_timeline)
    }

    /// Publish the exact endpoint after the source has explicitly sealed replay.
    /// The caller declares that no further source continuation is expected, calls
    /// `seal_replay`, then supplies the actual completed final boundary here.
    /// This never seals implicitly, resumes source code, or acknowledges rendering.
    ///
    /// The terminal batch includes every still-undelivered write, including writes
    /// at the previous frontier's timestamp. Only its successful adoption admits
    /// that endpoint in the reader; the current frame and effects stay unchanged.
    /// There is no additional full-scene snapshot or terminal frame evaluation.
    pub fn finish_prepared_prefix(
        &self,
        boundary: ExecutionSegment,
        publisher: &mut PrefixPublisher,
    ) -> Result<PrefixBatch, PrefixError> {
        self.ensure_prefix_boundary(boundary)?;
        self.runtime
            .prepare_prefix_finish_with_scalar_timeline(publisher, &self.signal_timeline)
    }
}

#[cfg(test)]
#[test]
fn paused_viewer_drag_follows_retained_binding_and_same_time_seek_restores_authored_frame() {
    use crate::Scene;
    use noon_core::{
        NativeInputModifiers, NativePointerCancellation, NativePointerId, NativePointerInput,
        NativePointerInputKind, NativePointerPosition, SemanticMutationTransaction, Vec2,
    };
    use noon_runtime::ReplayLimits;

    let mut scene = Scene::new();
    let mut shape = scene.circle(1.0).unwrap();
    shape.set_fill(0.0, 1.0, 0.0, 1.0).unwrap();
    scene.add(&shape).unwrap();
    let mut source = scene.execution_session().unwrap();
    source
        .begin_replay_retention(ReplayLimits::default())
        .unwrap();
    let intro = source.wait_segment(1.0).unwrap();
    source.advance_to(intro.end_time()).unwrap();
    scene.live(&mut source).complete_segment(intro).unwrap();
    let (mut publisher, mut reader) = source.create_prepared_prefix(intro).unwrap();

    let mut declaration = SemanticMutationTransaction::new();
    declaration.set_viewer_draggable(shape.node_id(), true);
    crate::integration::scene_execution::publish_at_root(
        scene.integration_store(),
        scene.root(),
        &mut source,
        declaration,
    )
    .unwrap();
    let hold = source.wait_segment(1.0).unwrap();
    source.advance_to(hold.end_time()).unwrap();
    scene.live(&mut source).complete_segment(hold).unwrap();
    let batch = source
        .publish_prepared_prefix(hold, &mut publisher)
        .unwrap();
    reader.reserve_append_capacity(&batch).unwrap();
    reader.append(batch).unwrap();
    let authored_revision = scene.integration_store().borrow().scene_revision();

    let pointer = NativePointerId {
        source: 7,
        pointer: 1,
    };
    let mut sequence = 0;
    let mut send = |reader: &mut PrefixReader, kind| {
        sequence += 1;
        let token = reader.native_pointer_input_token().unwrap();
        let event = NativePointerInput::new(
            sequence,
            pointer,
            token.context(),
            NativeInputModifiers::default(),
            kind,
        );
        reader.submit_native_pointer_input(&token, event).unwrap();
    };
    let position =
        |x| NativePointerPosition::new(Vec2::new(x, 0.0), Vec2::new(x * 10.0, 100.0)).unwrap();
    let down = |x| NativePointerInputKind::Press {
        position: position(x),
        button: 0,
    };
    let up = |x| NativePointerInputKind::Release {
        position: position(x),
        button: 0,
    };

    reader.configure_native_pointer_input(pointer, 1).unwrap();
    reader.seek(0.5).unwrap();
    send(&mut reader, down(0.0));
    assert!(!reader.translation_drag_active());
    send(&mut reader, up(0.0));

    reader.seek(1.5).unwrap();
    let authored = reader.frame().clone();
    send(&mut reader, down(0.0));
    assert!(reader.translation_drag_active());
    send(&mut reader, NativePointerInputKind::Move(position(2.0)));
    assert_eq!(
        reader.frame().objects[0].transform.translation,
        Vec2::new(2.0, 0.0)
    );
    send(
        &mut reader,
        NativePointerInputKind::Cancel(NativePointerCancellation::Cancelled),
    );
    assert_eq!(reader.frame(), &authored);
    assert!(!reader.can_undo_translation_drag());

    send(&mut reader, down(0.0));
    send(&mut reader, up(2.0));
    assert!(reader.can_undo_translation_drag());
    assert!(reader.undo_translation_drag().unwrap());
    assert_eq!(reader.frame(), &authored);

    send(&mut reader, down(0.0));
    send(&mut reader, up(1.0));
    assert!(reader.seek(2.0).is_err(), "open frontier remains exclusive");
    assert!(reader.can_undo_translation_drag());
    let publication = reader.publication_context();
    reader.seek(1.5).unwrap();
    assert_ne!(reader.publication_context(), publication);
    assert_eq!(reader.frame(), &authored);
    assert!(!reader.can_undo_translation_drag());

    send(&mut reader, down(0.0));
    send(&mut reader, up(1.0));
    reader.advance_to(1.75).unwrap();
    assert_eq!(reader.frame().objects[0].transform.translation, Vec2::ZERO);
    assert!(!reader.can_undo_translation_drag());
    assert_eq!(
        scene.integration_store().borrow().scene_revision(),
        authored_revision
    );
    assert_eq!(source.frame().objects[0].transform.translation, Vec2::ZERO);
}
