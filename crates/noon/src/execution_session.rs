mod publication;
pub use publication::*;

use std::collections::{BTreeMap, HashMap};

use crate::execution_segment::{ExecutionSegment, ExecutionSegmentError};
use noon_compile::{
    lower_semantic_affine_animation_tracks, lower_semantic_animation_schedule,
    lower_semantic_execution, lower_semantic_execution_root, CompilePatchError,
    ExecutionMutationTransaction, ExecutionPatch, SemanticAffineAnimationTrackError,
    SemanticAnimationScheduleError, SemanticExecutionIndex, SemanticExecutionLoweringError,
    SemanticExecutionLoweringOutput, SemanticExecutionReachability, SemanticReactiveProjection,
};
use noon_core::{
    AnimationOptions, Camera2DState, NativeEventOccurrence, NativeInputRuntimeError,
    NativeInputValue, NativeStateSource, NativeStateUpdate, ObjectId, ReactiveError, ReactiveValue,
    Rect, SemanticNodeId, SemanticStore, TrackId,
};
use noon_runtime::{
    EvaluationError, ExecutionSpatialIndex, FrameChanges, FrameState, RendererPublication,
    RuntimeWakeState, SceneInstance, SpatialIndexUpdateStats, SpatialQueryStats,
};

const NATIVE_EVENT_SEQUENCE_WRAP: f32 = 1_000_000.0;

/// Candidate-sized viewport result in canonical frame painter order.
///
/// Execution slots remain the index identity; frame indices are a transient
/// projection for the renderer's currently published dense compatibility view.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExecutionViewportQuery {
    object_indices: Vec<usize>,
    spatial_stats: SpatialQueryStats,
}

impl ExecutionViewportQuery {
    pub fn object_indices(&self) -> &[usize] {
        &self.object_indices
    }

    pub const fn spatial_stats(&self) -> SpatialQueryStats {
        self.spatial_stats
    }
}

/// Error produced when semantic/native reactive input cannot be applied to this execution session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExecutionSessionInputError {
    UnknownSemanticSignal(SemanticNodeId),
    NativeInput(NativeInputRuntimeError),
    NativeEventOutOfOrder { previous: u64, next: u64 },
    Reactive(ReactiveError),
}

impl std::fmt::Display for ExecutionSessionInputError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownSemanticSignal(signal) => write!(
                formatter,
                "semantic signal {}:{} is not present in this execution session",
                signal.slot(),
                signal.generation()
            ),
            Self::NativeInput(error) => error.fmt(formatter),
            Self::NativeEventOutOfOrder { previous, next } => write!(
                formatter,
                "native input event sequence must increase: previous {previous}, next {next}"
            ),
            Self::Reactive(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for ExecutionSessionInputError {}

impl From<NativeInputRuntimeError> for ExecutionSessionInputError {
    fn from(value: NativeInputRuntimeError) -> Self {
        Self::NativeInput(value)
    }
}

impl From<ReactiveError> for ExecutionSessionInputError {
    fn from(value: ReactiveError) -> Self {
        Self::Reactive(value)
    }
}

/// Error produced when the canonical semantic camera cannot be derived from the
/// current effective runtime frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExecutionSessionCameraError {
    object: ObjectId,
}

impl ExecutionSessionCameraError {
    pub const fn object(self) -> ObjectId {
        self.object
    }
}

impl std::fmt::Display for ExecutionSessionCameraError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "execution camera object {} is missing or not a valid 2D frame",
            self.object.get()
        )
    }
}

impl std::error::Error for ExecutionSessionCameraError {}

/// Error produced while activating one authoritative semantic animation declaration.
#[derive(Clone, Debug, PartialEq)]
pub enum ExecutionSessionAnimationError {
    ForeignSemanticStore,
    StaleSceneRevision {
        expected: noon_core::SceneRevision,
        actual: noon_core::SceneRevision,
    },
    Schedule(SemanticAnimationScheduleError),
    Segment(ExecutionSegmentError),
    Payload(SemanticAffineAnimationTrackError),
    Publication(CompilePatchError),
    TrackIdExhausted,
}

impl std::fmt::Display for ExecutionSessionAnimationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ForeignSemanticStore => {
                formatter.write_str("semantic store does not own this execution session")
            }
            Self::StaleSceneRevision { expected, actual } => write!(
                formatter,
                "semantic scene revision {} does not match execution scene revision {}",
                actual.get(),
                expected.get()
            ),
            Self::Schedule(error) => {
                write!(formatter, "semantic animation scheduling failed: {error}")
            }
            Self::Segment(error) => write!(
                formatter,
                "semantic animation continuation segment failed: {error}"
            ),
            Self::Payload(error) => write!(
                formatter,
                "semantic animation payload lowering failed: {error}"
            ),
            Self::Publication(error) => {
                write!(formatter, "semantic animation publication failed: {error}")
            }
            Self::TrackIdExhausted => {
                formatter.write_str("execution animation track ID space exhausted")
            }
        }
    }
}

impl std::error::Error for ExecutionSessionAnimationError {}

impl From<SemanticAnimationScheduleError> for ExecutionSessionAnimationError {
    fn from(value: SemanticAnimationScheduleError) -> Self {
        Self::Schedule(value)
    }
}

impl From<ExecutionSegmentError> for ExecutionSessionAnimationError {
    fn from(value: ExecutionSegmentError) -> Self {
        Self::Segment(value)
    }
}

impl From<SemanticAffineAnimationTrackError> for ExecutionSessionAnimationError {
    fn from(value: SemanticAffineAnimationTrackError) -> Self {
        Self::Payload(value)
    }
}

impl From<CompilePatchError> for ExecutionSessionAnimationError {
    fn from(value: CompilePatchError) -> Self {
        Self::Publication(value)
    }
}

/// Thin typed orchestration from authoritative semantic state into Noon's existing runtime.
///
/// The session does not own or mirror the [`SemanticStore`]. It retains only the
/// compiler-owned semantic/execution identity mappings needed by hosts plus the
/// existing [`SceneInstance`] runtime. Renderer and platform lifecycle remain outside
/// this type. The canonical camera is retained only as the execution identity of its
/// ordinary semantic frame object; its value is always derived from the effective
/// runtime frame.
///
/// Runtime mutation is intentionally not exposed as a mutable escape hatch here:
/// authored/live structural mutation remains owned by semantic transactions and
/// incremental lowering rather than the migration-era runtime patch surface.
#[derive(Clone, Debug)]
pub struct ExecutionSession {
    store_identity: noon_core::SemanticStoreIdentity,
    execution_index: SemanticExecutionIndex,
    reachability: SemanticExecutionReachability,
    painter_order: SemanticPainterOrderIndex,
    slots: noon_runtime::ExecutionSlotTable,
    spatial_index: ExecutionSpatialIndex,
    last_spatial_update: SpatialIndexUpdateStats,
    reactive_projection: SemanticReactiveProjection,
    runtime: SceneInstance,
    camera_object: Option<ObjectId>,
    next_activation_track_id: Option<u64>,
    last_native_event_sequence: Option<u64>,
    last_structural_publication: StructuralPublicationStats,
}

impl ExecutionSession {
    /// Lower one authoritative semantic snapshot and instantiate the existing runtime.
    pub fn from_semantic_store(
        store: &SemanticStore,
    ) -> Result<Self, SemanticExecutionLoweringError> {
        let mut execution_index = SemanticExecutionIndex::new();
        let reachability = SemanticExecutionReachability::from_store(store)?;
        let painter_order = semantic_painter_order(store, &reachability);
        let lowered = lower_semantic_execution(store, &mut execution_index)?;
        Ok(Self::from_lowered(
            store.identity(),
            execution_index,
            reachability,
            painter_order,
            lowered,
        ))
    }

    /// Instantiate the existing runtime for one semantic scene family.
    ///
    /// Detached families are valid initial scene roots. Other families in the
    /// shared store do not enter this execution domain. Membership remains owned
    /// by the store; this constructor neither mutates it nor establishes a live
    /// structural mutation contract. `root` must originate from `store`.
    pub fn from_semantic_root(
        store: &SemanticStore,
        root: SemanticNodeId,
    ) -> Result<Self, SemanticExecutionLoweringError> {
        let mut execution_index = SemanticExecutionIndex::new();
        let reachability = SemanticExecutionReachability::from_root(store, root)?;
        let painter_order = semantic_painter_order(store, &reachability);
        let lowered = lower_semantic_execution_root(store, root, &mut execution_index)?;
        Ok(Self::from_lowered(
            store.identity(),
            execution_index,
            reachability,
            painter_order,
            lowered,
        ))
    }

    fn from_lowered(
        store_identity: noon_core::SemanticStoreIdentity,
        execution_index: SemanticExecutionIndex,
        reachability: SemanticExecutionReachability,
        painter_order: SemanticPainterOrderIndex,
        lowered: SemanticExecutionLoweringOutput,
    ) -> Self {
        let camera_object = lowered.camera_object();
        let next_activation_track_id = lowered
            .compiled()
            .tracks_iter()
            .map(|track| track.id.get())
            .max()
            .map_or(Some(0), |id| id.checked_add(1));
        let slots = noon_runtime::ExecutionSlotTable::from_compiled(lowered.compiled());
        let reactive_projection = lowered.reactive().clone();
        let mut runtime = SceneInstance::from_semantic_execution(lowered);
        let mut spatial_index = ExecutionSpatialIndex::default();
        let live_slots =
            runtime
                .frame()
                .objects
                .iter()
                .enumerate()
                .filter_map(|(index, object)| {
                    if !runtime.object_slot_is_live(index) {
                        return None;
                    }
                    slots.slot_for_object(object.id).map(|slot| (slot, index))
                });
        let last_spatial_update = spatial_index.rebuild(runtime.frame(), live_slots);
        let _ = runtime.take_spatial_changes();
        Self {
            store_identity,
            execution_index,
            reachability,
            painter_order,
            slots,
            spatial_index,
            last_spatial_update,
            reactive_projection,
            runtime,
            camera_object,
            next_activation_track_id,
            last_native_event_sequence: None,
            last_structural_publication: StructuralPublicationStats::default(),
        }
    }

    /// Current renderer-facing runtime frame.
    pub fn frame(&self) -> &FrameState {
        self.runtime.frame()
    }

    /// Work performed by the most recent incremental execution-plan patch.
    pub const fn last_patch_stats(&self) -> noon_runtime::RuntimePatchStats {
        self.runtime.last_patch_stats()
    }

    /// Read-only text resources projected with this execution session.
    pub fn text_resources(&self) -> &impl noon_core::TextResourceLookup {
        self.runtime.text_resources()
    }

    /// Read-only font resources projected with this execution session.
    pub fn font_resources(&self) -> &impl noon_core::FontResourceLookup {
        self.runtime.font_resources()
    }

    /// Read-only geometry resources projected with this execution session.
    pub fn geometry_resources(&self) -> &impl noon_core::GeometryResourceLookup {
        self.runtime.geometry_resources()
    }

    /// Stable runtime identity for a current frame row. Structural publication
    /// retires and allocates durable slots without renumbering unrelated identities.
    pub fn execution_slot_for_frame_index(
        &self,
        index: usize,
    ) -> Option<noon_runtime::ExecutionSlotId> {
        if !self.runtime.object_slot_is_live(index) {
            return None;
        }
        let object = self.frame().objects.get(index)?;
        self.slots.slot_for_object(object.id)
    }

    /// Query current visible rows through the session-owned execution-slot index.
    pub fn query_viewport(&mut self, bounds: Rect) -> ExecutionViewportQuery {
        self.sync_spatial_index();
        let query = self.spatial_index.query_rect(bounds);
        let object_indices: Vec<_> = query
            .slots()
            .iter()
            .filter_map(|&slot| {
                let object = self.slots.object_for_slot(slot)?;
                self.runtime.frame_index_for_object(object)
            })
            .collect();
        debug_assert_eq!(
            object_indices.len(),
            query.stats().results,
            "live spatial candidates must resolve through execution identity"
        );
        ExecutionViewportQuery {
            object_indices,
            spatial_stats: query.stats(),
        }
    }

    pub const fn last_spatial_update_stats(&self) -> SpatialIndexUpdateStats {
        self.last_spatial_update
    }

    /// Exact authored/executable/effective publication context of this session.
    ///
    /// Hosts can pin coherent live reads and cross-context work to this typed context
    /// without receiving mutable runtime authority or substituting identity generations
    /// for revision clocks.
    pub const fn publication_context(&self) -> noon_core::PublicationContext {
        self.runtime.publication_context()
    }

    /// Current canonical 2D camera derived from the effective runtime frame object.
    ///
    /// Scenes without an authored camera use the shared Manim-compatible default.
    /// An authored camera never falls back silently: if its effective frame becomes
    /// invalid, the host receives an error just as the transport encoder does.
    pub fn camera(&self) -> Result<Camera2DState, ExecutionSessionCameraError> {
        let Some(camera_object) = self.camera_object else {
            return Ok(Camera2DState::default());
        };
        let object =
            self.runtime
                .effective_object(camera_object)
                .ok_or(ExecutionSessionCameraError {
                    object: camera_object,
                })?;
        object
            .geometry()
            .and_then(|geometry| Camera2DState::from_frame_object(geometry, object.transform))
            .ok_or(ExecutionSessionCameraError {
                object: camera_object,
            })
    }

    fn sync_spatial_index(&mut self) {
        let changes = self.runtime.take_spatial_changes();
        if changes.is_empty() {
            self.last_spatial_update = SpatialIndexUpdateStats::default();
            return;
        }
        if changes.is_all() {
            let live_slots =
                self.runtime
                    .frame()
                    .objects
                    .iter()
                    .enumerate()
                    .filter_map(|(index, object)| {
                        if !self.runtime.object_slot_is_live(index) {
                            return None;
                        }
                        self.slots
                            .slot_for_object(object.id)
                            .map(|slot| (slot, index))
                    });
            self.last_spatial_update = self.spatial_index.rebuild(self.runtime.frame(), live_slots);
            return;
        }
        let mut stats = SpatialIndexUpdateStats::default();
        for &index in changes.object_indices() {
            let Some(object) = self.runtime.frame().objects.get(index) else {
                continue;
            };
            if self.runtime.object_slot_is_live(index) {
                if let Some(slot) = self.slots.slot_for_object(object.id) {
                    stats.merge_from(self.spatial_index.upsert_frame_slot(
                        self.runtime.frame(),
                        slot,
                        index,
                        index as u64,
                    ));
                }
            } else {
                stats.merge_from(self.spatial_index.remove_object(object.id));
            }
        }
        self.last_spatial_update = stats;
    }

    /// Read runtime-owned presentation dirtiness and timeline cadence without
    /// exposing the runtime scheduler or introducing a host-side timing model.
    pub fn wake_state(&self) -> RuntimeWakeState {
        self.runtime.wake_state()
    }

    /// Consume renderer-facing invalidation state accumulated by the runtime.
    pub fn take_frame_changes(&mut self) -> FrameChanges {
        self.runtime.take_frame_changes()
    }

    /// Consume one coherent renderer publication from this typed session.
    pub fn take_renderer_publication(&mut self) -> RendererPublication<'_> {
        self.runtime.take_renderer_publication()
    }

    /// Evaluate deterministically at an absolute time.
    pub fn evaluate(&mut self, time: f64) -> Result<&FrameState, EvaluationError> {
        self.runtime.evaluate(time)
    }

    /// Seek deterministically to an absolute time.
    pub fn seek(&mut self, time: f64) -> Result<&FrameState, EvaluationError> {
        self.runtime.seek(time)
    }

    /// Advance to an absolute time, falling back to deterministic seek when time moves backward.
    pub fn advance_to(&mut self, time: f64) -> Result<&FrameState, EvaluationError> {
        self.runtime.advance_to(time)
    }

    /// Activate one semantic animation root at the session's current deterministic time.
    ///
    /// This compatibility surface preserves the existing frame-returning API. Continuation
    /// consumers should use [`Self::activate_animation_segment`] so the logical segment comes
    /// from the same compiler-resolved scheduling/publication pass rather than re-resolving
    /// duration in an authoring facade.
    pub fn activate_animation(
        &mut self,
        store: &SemanticStore,
        root: SemanticNodeId,
        play_options: AnimationOptions,
    ) -> Result<&FrameState, ExecutionSessionAnimationError> {
        self.activate_animation_segment(store, root, play_options)?;
        Ok(self.runtime.frame())
    }

    /// Activate one semantic animation and return its canonical logical continuation segment.
    ///
    /// The authoritative semantic store remains caller-owned. Scheduling reads the current
    /// declaration once; the segment is constructed directly from that resolved projection's
    /// `start_time` / `run_time`; affine payload lowering captures each target's effective
    /// runtime transform at most once; and the session attaches execution-local track identity.
    /// All emitted tracks are preflighted and published as one execution transaction,
    /// so a failed activation cannot expose a partial timeline or continuation boundary.
    pub fn activate_animation_segment(
        &mut self,
        store: &SemanticStore,
        root: SemanticNodeId,
        play_options: AnimationOptions,
    ) -> Result<ExecutionSegment, ExecutionSessionAnimationError> {
        if store.identity() != self.store_identity {
            return Err(ExecutionSessionAnimationError::ForeignSemanticStore);
        }
        let expected = self.publication_context().scene_revision();
        let actual = store.scene_revision();
        if actual != expected {
            return Err(ExecutionSessionAnimationError::StaleSceneRevision { expected, actual });
        }
        let schedule = lower_semantic_animation_schedule(
            store,
            &self.execution_index,
            root,
            self.runtime.frame().time,
            play_options,
        )?;
        let segment = ExecutionSegment::from_duration(schedule.start_time(), schedule.run_time())?;
        let tracks = lower_semantic_affine_animation_tracks(store, &schedule, |object| {
            self.runtime.effective_transform(object)
        })?;
        if tracks.is_empty() {
            return Ok(segment);
        }

        let mut next_track_id = self.next_activation_track_id;
        let mut mutations = Vec::with_capacity(tracks.len());
        for track in tracks.tracks() {
            let raw_id = next_track_id.ok_or(ExecutionSessionAnimationError::TrackIdExhausted)?;
            let definition = track.with_track_id(TrackId::new(raw_id))?;
            mutations.push(ExecutionPatch::AddTrack(definition));
            next_track_id = raw_id.checked_add(1);
        }

        let transaction = ExecutionMutationTransaction::from_mutations(mutations);
        self.runtime.apply_execution_transaction(&transaction)?;
        self.next_activation_track_id = next_track_id;
        Ok(segment)
    }

    /// Apply one native-reactive input using authoritative semantic signal identity.
    ///
    /// The execution VM key remains an internal lowering detail. Current native
    /// reactive values use the compact execution domain (`bool`, `f32`, `Vec2`);
    /// semantic structural/property mutation remains outside this method.
    pub fn set_reactive_input(
        &mut self,
        signal: SemanticNodeId,
        value: impl Into<ReactiveValue>,
    ) -> Result<&FrameState, ExecutionSessionInputError> {
        let execution_signal = self
            .reactive_projection
            .execution_signal_id(signal)
            .ok_or(ExecutionSessionInputError::UnknownSemanticSignal(signal))?;
        Ok(self.runtime.set_reactive_input(execution_signal, value)?)
    }

    /// Deliver one normalized sampled native state source through signal-owned routes.
    ///
    /// Source/value validation is shared with browser/native input envelopes. The
    /// source is resolved only against routes emitted by semantic reactive lowering;
    /// platform hosts never receive or construct execution `SignalId`s. An unbound
    /// source is a valid no-op.
    pub fn set_native_state_input(
        &mut self,
        source: NativeStateSource,
        value: NativeInputValue,
    ) -> Result<&FrameState, ExecutionSessionInputError> {
        let update = NativeStateUpdate::new(source, value)?;
        let targets = self
            .reactive_projection
            .native_state_targets(&update.source)
            .to_vec();
        let value = reactive_value_from_native(update.value);
        for signal in targets {
            self.runtime.set_reactive_input(signal, value.clone())?;
        }
        Ok(self.runtime.frame())
    }

    /// Deliver one explicitly ordered discrete native event occurrence.
    ///
    /// Occurrences are never coalesced. The session rejects duplicate/out-of-order
    /// sequence numbers before changing any event signal, then advances each lowered
    /// scalar event counter using the existing native-event convention.
    pub fn emit_native_event(
        &mut self,
        occurrence: NativeEventOccurrence,
    ) -> Result<&FrameState, ExecutionSessionInputError> {
        if let Some(previous) = self.last_native_event_sequence {
            if occurrence.sequence <= previous {
                return Err(ExecutionSessionInputError::NativeEventOutOfOrder {
                    previous,
                    next: occurrence.sequence,
                });
            }
        }

        let targets = self
            .reactive_projection
            .native_event_targets(&occurrence.source)
            .to_vec();
        let next_values = targets
            .iter()
            .map(|signal| {
                let value = self
                    .runtime
                    .reactive_value(*signal)
                    .expect("lowered native event target must remain a live reactive signal");
                let ReactiveValue::Scalar(current) = value else {
                    unreachable!(
                        "semantic native event declaration validates a scalar input signal"
                    )
                };
                if *current >= NATIVE_EVENT_SEQUENCE_WRAP {
                    0.0
                } else {
                    *current + 1.0
                }
            })
            .collect::<Vec<_>>();

        for (signal, next) in targets.into_iter().zip(next_values) {
            self.runtime.set_reactive_input(signal, next)?;
        }
        self.last_native_event_sequence = Some(occurrence.sequence);
        Ok(self.runtime.frame())
    }

    /// Resolve an authoritative semantic object identity to its current execution key.
    pub fn execution_object_id(&self, node: SemanticNodeId) -> Option<ObjectId> {
        self.execution_index.execution_object_id(node)
    }
}

fn semantic_painter_order(
    store: &SemanticStore,
    reachability: &SemanticExecutionReachability,
) -> SemanticPainterOrderIndex {
    let mut index = SemanticPainterOrderIndex::default();
    for node in reachability.reachable_objects() {
        let state = store
            .semantic_object_state_checked(node)
            .expect("reachable semantic object was validated during initial lowering");
        index.insert(node, state.presentation().order_key());
    }
    index
}

#[derive(Clone, Debug, Default)]
struct SemanticPainterOrderIndex {
    ordered: BTreeMap<(i32, u64), SemanticNodeId>,
    keys: HashMap<SemanticNodeId, (i32, u64)>,
}

impl SemanticPainterOrderIndex {
    fn tail(&self) -> Option<(i32, u64)> {
        self.ordered.last_key_value().map(|(key, _)| *key)
    }

    fn insert(&mut self, node: SemanticNodeId, key: (i32, u64)) {
        debug_assert!(self.keys.insert(node, key).is_none());
        debug_assert!(self.ordered.insert(key, node).is_none());
    }

    fn remove(&mut self, node: SemanticNodeId) {
        let key = self
            .keys
            .remove(&node)
            .expect("exited execution object has a painter-order entry");
        let removed = self.ordered.remove(&key);
        debug_assert_eq!(removed, Some(node));
    }
}

fn reactive_value_from_native(value: NativeInputValue) -> ReactiveValue {
    match value {
        NativeInputValue::Scalar(value) => ReactiveValue::Scalar(value),
        NativeInputValue::Bool(value) => ReactiveValue::Bool(value),
        NativeInputValue::Vec2(value) => ReactiveValue::Vec2(value),
    }
}

#[cfg(test)]
mod tests {
    use noon_core::{
        AnimationOptions, NativeEventSource, NativeStateSource, RateFunction,
        SemanticObjectProperty, SemanticObjectRole, SemanticObjectState, SemanticVec3,
        StoredGeometry, Vec2,
    };
    use noon_runtime::TimelineWakeState;

    use super::*;

    fn linear_second() -> AnimationOptions {
        AnimationOptions::new()
            .run_time(1.0)
            .rate_func(RateFunction::Linear)
    }

    fn camera_state(center: Vec2, height: f32) -> SemanticObjectState {
        let mut state = SemanticObjectState::new(StoredGeometry::Rectangle {
            size: Vec2::new(height * 16.0 / 9.0, height),
        });
        state.transform.translation = SemanticVec3::new(center.x as f64, center.y as f64, 0.0);
        state.set_role(SemanticObjectRole::Camera2D);
        state
    }

    #[test]
    fn semantic_family_sessions_isolate_camera_objects_and_reactive_state() {
        let mut store = SemanticStore::new();
        let left_root = store.insert_family();
        let right_root = store.insert_family();
        let left_camera = store.insert_semantic_object(camera_state(Vec2::new(-2.0, 0.0), 4.0));
        let right_camera = store.insert_semantic_object(camera_state(Vec2::new(3.0, 1.0), 6.0));
        let shared =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        let signal = store.insert_semantic_input_signal(0.4_f64).unwrap();
        store
            .bind_semantic_signal(signal, shared, SemanticObjectProperty::ObjectOpacity)
            .unwrap();
        for (root, camera) in [(left_root, left_camera), (right_root, right_camera)] {
            store.add_semantic_family_member(root, camera).unwrap();
            store.add_semantic_family_member(root, shared).unwrap();
        }
        let mut left = ExecutionSession::from_semantic_root(&store, left_root).unwrap();
        let right = ExecutionSession::from_semantic_root(&store, right_root).unwrap();
        assert_eq!(left.frame().objects.len(), 2);
        assert_eq!(right.frame().objects.len(), 2);
        assert_eq!(left.execution_object_id(right_camera), None);
        assert_eq!(right.execution_object_id(left_camera), None);
        assert_eq!(
            left.execution_object_id(shared),
            right.execution_object_id(shared)
        );
        assert_eq!(
            left.camera().unwrap(),
            Camera2DState {
                center: Vec2::new(-2.0, 0.0),
                height: 4.0
            }
        );
        assert_eq!(
            right.camera().unwrap(),
            Camera2DState {
                center: Vec2::new(3.0, 1.0),
                height: 6.0
            }
        );
        left.set_reactive_input(signal, 0.8_f32).unwrap();
        assert_eq!(left.frame().objects[1].style.opacity, 0.8);
        assert_eq!(right.frame().objects[1].style.opacity, 0.4);
        assert_eq!(store.scene_roots().count(), 0);
        // Existing all-roots consumers still see the store's actual attached roots.
        assert!(ExecutionSession::from_semantic_store(&store)
            .unwrap()
            .frame()
            .objects
            .is_empty());
    }

    #[test]
    fn semantic_store_runs_through_typed_session_into_renderer_facing_changes() {
        let mut store = SemanticStore::new();
        let signal = store.insert_semantic_input_signal(0.4_f64).unwrap();
        let object =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 2.0,
            }));
        store.attach_to_scene(object).unwrap();
        store
            .bind_semantic_signal(signal, object, SemanticObjectProperty::ObjectOpacity)
            .unwrap();

        let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
        let execution_object = session.execution_object_id(object).unwrap();
        let initial_publication = session.publication_context();

        assert_eq!(session.frame().objects.len(), 1);
        assert_eq!(session.frame().objects[0].id, execution_object);
        assert_eq!(session.frame().objects[0].style.opacity, 0.4);
        assert_eq!(session.camera().unwrap(), Camera2DState::default());

        session.take_frame_changes();
        session.set_reactive_input(signal, 0.7_f32).unwrap();
        let reactive_publication = session.publication_context();

        assert_eq!(session.frame().objects[0].style.opacity, 0.7);
        assert_eq!(session.take_frame_changes().object_indices(), &[0]);
        assert_eq!(
            reactive_publication.scene_revision(),
            initial_publication.scene_revision()
        );
        assert_eq!(
            reactive_publication.execution_revision(),
            initial_publication.execution_revision()
        );
        assert_eq!(
            reactive_publication.frame_epoch(),
            initial_publication.frame_epoch().checked_next().unwrap()
        );

        session.set_reactive_input(signal, 0.7_f32).unwrap();
        assert_eq!(session.publication_context(), reactive_publication);

        session.seek(1.25).unwrap();
        let timeline_publication = session.publication_context();
        assert_eq!(session.frame().time, 1.25);
        assert_eq!(session.frame().objects[0].style.opacity, 0.7);
        assert_eq!(
            timeline_publication.scene_revision(),
            reactive_publication.scene_revision()
        );
        assert_eq!(
            timeline_publication.execution_revision(),
            reactive_publication.execution_revision()
        );
        assert_eq!(
            timeline_publication.frame_epoch(),
            reactive_publication.frame_epoch().checked_next().unwrap()
        );
    }

    #[test]
    fn viewport_query_maps_slots_to_painter_order_and_refits_local_changes() {
        let mut store = SemanticStore::new();
        let moving = store
            .insert_semantic_input_signal(SemanticVec3::new(0.0, 0.0, 0.0))
            .unwrap();
        let mut left_state = SemanticObjectState::new(StoredGeometry::Circle { radius: 0.5 });
        left_state.transform.translation = SemanticVec3::new(-10.0, 0.0, 0.0);
        let left = store.insert_semantic_object(left_state);
        let middle =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 0.5,
            }));
        let top = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
            radius: 0.25,
        }));
        for object in [left, middle, top] {
            store.attach_to_scene(object).unwrap();
        }
        store
            .bind_semantic_signal(moving, middle, SemanticObjectProperty::Translation)
            .unwrap();
        let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
        let viewport = Rect::new(Vec2::new(-1.0, -1.0), Vec2::new(1.0, 1.0));

        let initial = session.query_viewport(viewport);
        assert_eq!(initial.object_indices(), &[1, 2]);
        assert_eq!(initial.spatial_stats().results, 2);

        session
            .set_reactive_input(moving, Vec2::new(20.0, 0.0))
            .unwrap();
        let moved = session.query_viewport(viewport);
        assert_eq!(moved.object_indices(), &[2]);
        assert_eq!(session.last_spatial_update_stats().full_rebuilds, 0);
        assert_eq!(session.last_spatial_update_stats().leaves_upserted, 1);
    }

    #[test]
    fn wake_state_separates_one_pending_frame_from_static_completion() {
        let mut store = SemanticStore::new();
        let object =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        store.attach_to_scene(object).unwrap();
        let mut session = ExecutionSession::from_semantic_store(&store).unwrap();

        let wake = session.wake_state();
        assert!(wake.frame_pending());
        assert_eq!(wake.timeline(), TimelineWakeState::Quiescent);
        assert!(!wake.is_quiescent());

        session.take_frame_changes();
        assert!(session.wake_state().is_quiescent());
    }

    #[test]
    fn activated_animation_requests_continuous_frames_until_endpoint() {
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
        session.take_frame_changes();

        session
            .activate_animation(&store, animation, linear_second())
            .unwrap();
        assert_eq!(
            session.wake_state().timeline(),
            TimelineWakeState::Continuous
        );

        session.seek(1.0).unwrap();
        assert_eq!(
            session.wake_state().timeline(),
            TimelineWakeState::Quiescent
        );
    }

    #[test]
    fn animation_activation_returns_resolved_continuation_segment() {
        let mut store = SemanticStore::new();
        let object =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        store.attach_to_scene(object).unwrap();
        let mut target_state = store.semantic_object_state_checked(object).unwrap().clone();
        target_state.transform.translation = SemanticVec3::new(6.0, 0.0, 0.0);
        let target = store.insert_semantic_object(target_state);
        let animation = store
            .insert_semantic_transform_animation(
                object,
                target,
                AnimationOptions::new().run_time(4.0),
            )
            .unwrap();
        let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
        session.seek(3.0).unwrap();
        session.take_frame_changes();

        let segment = session
            .activate_animation_segment(
                &store,
                animation,
                AnimationOptions::new()
                    .run_time(1.5)
                    .rate_func(RateFunction::Linear),
            )
            .unwrap();

        assert_eq!(segment.start_time(), 3.0);
        assert_eq!(segment.duration(), 1.5);
        assert_eq!(segment.end_time(), 4.5);
        assert_eq!(
            session.segment_state(segment).timeline(),
            TimelineWakeState::Continuous
        );

        session.advance_segment_to(segment, 100.0).unwrap();
        assert_eq!(session.frame().time, 4.5);
        assert!(session.segment_state(segment).is_complete());
        assert_eq!(session.frame().objects[0].transform.translation.x, 6.0);
    }

    #[test]
    fn animation_segment_overflow_fails_before_publication() {
        let mut store = SemanticStore::new();
        let object =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        store.attach_to_scene(object).unwrap();
        let mut target_state = store.semantic_object_state_checked(object).unwrap().clone();
        target_state.transform.translation = SemanticVec3::new(2.0, 0.0, 0.0);
        let target = store.insert_semantic_object(target_state);
        let animation = store
            .insert_semantic_transform_animation(object, target, AnimationOptions::new())
            .unwrap();
        let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
        session.seek(f64::MAX).unwrap();
        session.take_frame_changes();
        let before = session.frame().clone();

        assert_eq!(
            session.activate_animation_segment(&store, animation, linear_second()),
            Err(ExecutionSessionAnimationError::Segment(
                ExecutionSegmentError::EndTimeOverflow {
                    start_time: f64::MAX,
                    duration: 1.0,
                }
            ))
        );
        assert_eq!(session.frame(), &before);
        assert!(session.wake_state().is_quiescent());
    }

    #[test]
    fn native_sampled_source_reaches_runtime_without_exposing_signal_identity() {
        let mut store = SemanticStore::new();
        let signal = store.insert_semantic_input_signal(0.25_f64).unwrap();
        let source = NativeStateSource::Control {
            name: "opacity".to_owned(),
        };
        store
            .bind_semantic_native_state_input(signal, source.clone())
            .unwrap();
        let object =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        store.attach_to_scene(object).unwrap();
        store
            .bind_semantic_signal(signal, object, SemanticObjectProperty::ObjectOpacity)
            .unwrap();
        let mut session = ExecutionSession::from_semantic_store(&store).unwrap();

        session.take_frame_changes();
        session
            .set_native_state_input(source, NativeInputValue::Scalar(0.8))
            .unwrap();
        assert_eq!(session.frame().objects[0].style.opacity, 0.8);
        assert_eq!(session.take_frame_changes().object_indices(), &[0]);
    }

    #[test]
    fn native_sampled_source_validates_value_before_runtime_mutation() {
        let mut store = SemanticStore::new();
        let signal = store
            .insert_semantic_input_signal(SemanticVec3::new(0.0, 0.0, 0.0))
            .unwrap();
        store
            .bind_semantic_native_state_input(signal, NativeStateSource::ViewportSize)
            .unwrap();
        let object =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        store.attach_to_scene(object).unwrap();
        store
            .bind_semantic_signal(signal, object, SemanticObjectProperty::Translation)
            .unwrap();
        let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
        let before = session.frame().clone();

        assert!(matches!(
            session.set_native_state_input(
                NativeStateSource::ViewportSize,
                NativeInputValue::Scalar(1.0),
            ),
            Err(ExecutionSessionInputError::NativeInput(
                NativeInputRuntimeError::TypeMismatch { .. }
            ))
        ));
        assert_eq!(session.frame(), &before);
    }

    #[test]
    fn native_discrete_events_are_ordered_and_never_coalesced() {
        let mut store = SemanticStore::new();
        let signal = store.insert_semantic_input_signal(0.0_f64).unwrap();
        let source = NativeEventSource::KeyPress {
            code: "Space".to_owned(),
        };
        store
            .bind_semantic_native_event_input(signal, source.clone())
            .unwrap();
        let object =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        store.attach_to_scene(object).unwrap();
        store
            .bind_semantic_signal(signal, object, SemanticObjectProperty::RotationZ)
            .unwrap();
        let mut session = ExecutionSession::from_semantic_store(&store).unwrap();

        session
            .emit_native_event(NativeEventOccurrence::new(10, source.clone()))
            .unwrap();
        assert_eq!(session.frame().objects[0].transform.rotation, 1.0);
        session
            .emit_native_event(NativeEventOccurrence::new(11, source.clone()))
            .unwrap();
        assert_eq!(session.frame().objects[0].transform.rotation, 2.0);

        assert_eq!(
            session.emit_native_event(NativeEventOccurrence::new(11, source)),
            Err(ExecutionSessionInputError::NativeEventOutOfOrder {
                previous: 11,
                next: 11,
            })
        );
        assert_eq!(session.frame().objects[0].transform.rotation, 2.0);
    }

    #[test]
    fn native_discrete_event_counter_matches_existing_bounded_wrap_convention() {
        let mut store = SemanticStore::new();
        let signal = store
            .insert_semantic_input_signal(NATIVE_EVENT_SEQUENCE_WRAP)
            .unwrap();
        let source = NativeEventSource::PointerDown { button: 0 };
        store
            .bind_semantic_native_event_input(signal, source.clone())
            .unwrap();
        let object =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        store.attach_to_scene(object).unwrap();
        store
            .bind_semantic_signal(signal, object, SemanticObjectProperty::RotationZ)
            .unwrap();
        let mut session = ExecutionSession::from_semantic_store(&store).unwrap();

        session
            .emit_native_event(NativeEventOccurrence::new(0, source))
            .unwrap();
        assert_eq!(session.frame().objects[0].transform.rotation, 0.0);
    }

    #[test]
    fn canonical_camera_is_derived_from_effective_runtime_transform() {
        let mut store = SemanticStore::new();
        let camera = store.insert_semantic_object(camera_state(Vec2::new(1.0, 2.0), 8.0));
        store.attach_to_scene(camera).unwrap();

        let mut target_state = store.semantic_object_state_checked(camera).unwrap().clone();
        target_state.transform.translation = SemanticVec3::new(5.0, -2.0, 0.0);
        target_state.transform.scale = SemanticVec3::new(1.0, 0.5, 1.0);
        let target = store.insert_semantic_object(target_state);
        let animation = store
            .insert_semantic_transform_animation(camera, target, AnimationOptions::new())
            .unwrap();

        let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
        assert_eq!(
            session.camera().unwrap(),
            Camera2DState {
                center: Vec2::new(1.0, 2.0),
                height: 8.0,
            }
        );

        session
            .activate_animation(&store, animation, linear_second())
            .unwrap();
        session.seek(0.5).unwrap();
        assert_eq!(
            session.camera().unwrap(),
            Camera2DState {
                center: Vec2::new(3.0, 0.0),
                height: 6.0,
            }
        );
    }

    #[test]
    fn animation_activation_rejects_an_unpublished_semantic_revision() {
        let mut store = SemanticStore::new();
        let object =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        store.attach_to_scene(object).unwrap();
        let target =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        let animation = store
            .insert_semantic_transform_animation(object, target, AnimationOptions::new())
            .unwrap();
        let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
        session.take_frame_changes();
        let context = session.publication_context();
        let frame = session.frame().clone();
        let mut transaction = noon_core::SemanticMutationTransaction::new();
        transaction.set_property(object, SemanticObjectProperty::ObjectOpacity, 0.5_f64);
        transaction.apply(&mut store).unwrap();

        assert_eq!(
            session.activate_animation_segment(&store, animation, linear_second()),
            Err(ExecutionSessionAnimationError::StaleSceneRevision {
                expected: context.scene_revision(),
                actual: store.scene_revision(),
            })
        );
        assert_eq!(session.publication_context(), context);
        assert_eq!(session.frame(), &frame);
        assert!(session.take_frame_changes().is_empty());
    }

    #[test]
    fn execution_vm_signal_identity_does_not_escape_the_session_surface() {
        let mut store = SemanticStore::new();
        let object =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        store.attach_to_scene(object).unwrap();

        let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
        assert_eq!(
            session.set_reactive_input(object, 1.0_f32),
            Err(ExecutionSessionInputError::UnknownSemanticSignal(object))
        );
    }

    #[test]
    fn chained_activation_starts_from_current_effective_affine_state() {
        let mut store = SemanticStore::new();
        let object =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        store.attach_to_scene(object).unwrap();

        let mut first_state = store.semantic_object_state_checked(object).unwrap().clone();
        first_state.transform.translation = SemanticVec3::new(4.0, 0.0, 0.0);
        first_state.transform.rotation_z = 0.5;
        first_state.transform.scale = SemanticVec3::new(2.0, 2.0, 1.0);
        let first_state = store.insert_semantic_object(first_state);
        let first = store
            .insert_semantic_transform_animation(object, first_state, AnimationOptions::new())
            .unwrap();

        let mut second_state = store.semantic_object_state_checked(object).unwrap().clone();
        second_state.transform.translation = SemanticVec3::new(10.0, 0.0, 0.0);
        second_state.transform.rotation_z = 1.5;
        second_state.transform.scale = SemanticVec3::new(4.0, 1.0, 1.0);
        let second_state = store.insert_semantic_object(second_state);
        let second = store
            .insert_semantic_transform_animation(object, second_state, AnimationOptions::new())
            .unwrap();

        let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
        session
            .activate_animation(&store, first, linear_second())
            .unwrap();
        session.seek(1.0).unwrap();
        assert_eq!(
            session.frame().objects[0].transform,
            noon_core::Transform2D {
                translation: Vec2::new(4.0, 0.0),
                rotation: 0.5,
                scale: Vec2::new(2.0, 2.0),
            }
        );

        session
            .activate_animation(&store, second, linear_second())
            .unwrap();
        session.seek(1.5).unwrap();

        assert_eq!(
            session.frame().objects[0].transform,
            noon_core::Transform2D {
                translation: Vec2::new(7.0, 0.0),
                rotation: 1.0,
                scale: Vec2::new(3.0, 1.5),
            }
        );
    }
}
