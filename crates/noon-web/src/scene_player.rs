//! Temporary persistent scene execution core used by current web execution adapters.
//!
//! This is intentionally separated from the deleted browser/canvas compatibility
//! wrapper island. It remains a migration seam until the typed execution-session
//! contract replaces `SceneDefinition`/JSON ingress under Phase A.

use noon_compile::{CompileError, CompilePatchError, CompiledScene};
use std::collections::{BTreeMap, BTreeSet};

use noon_core::{
    preflight_transaction, MutationTransaction, ObjectId, PatchError, Rect, SceneDefinition,
    ScenePatch, Vec2,
};
use noon_ir::{decode_patch_batch, decode_scene, encode_scene, IrError};
use noon_runtime::{
    EvaluationError, ExecutionCompactionError, ExecutionCompactionStats, ExecutionDelta,
    ExecutionSlotId, ExecutionTransactionError, FrameChanges, FrameSlotId, FrameState,
    RetiredSlotCompactionPolicy, SlottedSceneInstance,
};

#[derive(Debug)]
pub enum PlayerError {
    Ir(IrError),
    Compile(CompileError),
    Patch(PatchError),
    CompilePatch(CompilePatchError),
    ExecutionTransaction(ExecutionTransactionError),
    Compaction(ExecutionCompactionError),
    Evaluation(EvaluationError),
    Sequence { expected: u64, actual: u64 },
    SequenceExhausted,
}

impl std::fmt::Display for PlayerError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ir(error) => write!(formatter, "{error}"),
            Self::Compile(error) => write!(formatter, "scene compilation failed: {error}"),
            Self::Patch(error) => write!(formatter, "scene patch failed: {error}"),
            Self::CompilePatch(error) => write!(formatter, "runtime patch failed: {error}"),
            Self::ExecutionTransaction(error) => {
                write!(formatter, "execution transaction failed: {error}")
            }
            Self::Compaction(error) => write!(formatter, "execution compaction failed: {error}"),
            Self::Evaluation(error) => write!(formatter, "scene evaluation failed: {error}"),
            Self::Sequence { expected, actual } => {
                write!(
                    formatter,
                    "expected patch sequence {expected}, got {actual}"
                )
            }
            Self::SequenceExhausted => formatter.write_str("patch sequence space exhausted"),
        }
    }
}

impl std::error::Error for PlayerError {}

impl From<IrError> for PlayerError {
    fn from(value: IrError) -> Self {
        Self::Ir(value)
    }
}

impl From<CompileError> for PlayerError {
    fn from(value: CompileError) -> Self {
        Self::Compile(value)
    }
}

impl From<PatchError> for PlayerError {
    fn from(value: PatchError) -> Self {
        Self::Patch(value)
    }
}

impl From<CompilePatchError> for PlayerError {
    fn from(value: CompilePatchError) -> Self {
        Self::CompilePatch(value)
    }
}

impl From<ExecutionTransactionError> for PlayerError {
    fn from(value: ExecutionTransactionError) -> Self {
        Self::ExecutionTransaction(value)
    }
}

impl From<ExecutionCompactionError> for PlayerError {
    fn from(value: ExecutionCompactionError) -> Self {
        Self::Compaction(value)
    }
}

impl From<EvaluationError> for PlayerError {
    fn from(value: EvaluationError) -> Self {
        Self::Evaluation(value)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlayerTransactionStats {
    pub mutations: usize,
    pub semantic_scene_clones: usize,
    pub runtime_rebuilds: usize,
}

#[derive(Clone, Debug)]
pub struct ScenePlayer {
    definition: SceneDefinition,
    instance: SlottedSceneInstance,
    next_sequence: u64,
    last_transaction_stats: PlayerTransactionStats,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReconcileOutcome {
    Incremental { patch_count: usize },
    Rebuilt { patch_count: usize },
    Replaced,
}

impl ScenePlayer {
    pub fn from_scene_json(json: &str) -> Result<Self, PlayerError> {
        let definition = decode_scene(json)?;
        let compiled = CompiledScene::compile(&definition)?;
        Ok(Self {
            definition,
            instance: SlottedSceneInstance::new(compiled),
            next_sequence: 0,
            last_transaction_stats: PlayerTransactionStats::default(),
        })
    }

    pub fn seek(&mut self, time: f64) -> Result<&FrameState, PlayerError> {
        Ok(self.instance.seek(time)?)
    }

    pub fn advance_to(&mut self, time: f64) -> Result<&FrameState, PlayerError> {
        Ok(self.instance.advance_to(time)?)
    }

    pub fn take_frame_changes(&mut self) -> FrameChanges {
        self.instance.take_frame_changes()
    }

    /// Apply host-callback mutations without consuming the interactive patch sequence.
    pub(crate) fn apply_host_patch_batch_json(
        &mut self,
        json: &str,
    ) -> Result<&FrameState, PlayerError> {
        let batch = decode_patch_batch(json)?;
        self.apply_patches_transactionally(&batch.patches)?;
        Ok(self.instance.frame())
    }

    pub fn apply_patch_batch_json(&mut self, json: &str) -> Result<&FrameState, PlayerError> {
        let batch = decode_patch_batch(json)?;
        if batch.sequence != self.next_sequence {
            return Err(PlayerError::Sequence {
                expected: self.next_sequence,
                actual: batch.sequence,
            });
        }

        let next_sequence = self
            .next_sequence
            .checked_add(1)
            .ok_or(PlayerError::SequenceExhausted)?;
        self.apply_patches_transactionally(&batch.patches)?;
        self.next_sequence = next_sequence;
        Ok(self.instance.frame())
    }

    pub fn replace_scene_json(&mut self, json: &str) -> Result<&FrameState, PlayerError> {
        let definition = decode_scene(json)?;
        let compiled = CompiledScene::compile(&definition)?;
        let playhead = self.instance.frame().time;
        let mut instance = SlottedSceneInstance::new(compiled);
        instance.seek(playhead)?;

        self.definition = definition;
        self.instance = instance;
        self.next_sequence = 0;
        Ok(self.instance.frame())
    }

    pub fn reconcile_scene_json(&mut self, json: &str) -> Result<ReconcileOutcome, PlayerError> {
        let desired = decode_scene(json)?;
        let Some(patches) = scene_diff(&self.definition, &desired) else {
            self.replace_scene_json(json)?;
            return Ok(ReconcileOutcome::Replaced);
        };
        let patch_count = patches.len();
        self.apply_patches_transactionally(&patches)?;
        self.next_sequence = 0;
        Ok(ReconcileOutcome::Incremental { patch_count })
    }

    fn apply_patches_transactionally(&mut self, patches: &[ScenePatch]) -> Result<(), PlayerError> {
        let transaction = MutationTransaction::from_mutations(patches.iter().cloned());
        preflight_transaction(&self.definition, &transaction)?;
        self.instance.apply_transaction(&transaction)?;
        for patch in patches {
            self.definition
                .apply_patch(patch.clone())
                .expect("semantic transaction was fully preflighted");
        }
        self.last_transaction_stats = PlayerTransactionStats {
            mutations: patches.len(),
            semantic_scene_clones: 0,
            runtime_rebuilds: 0,
        };
        Ok(())
    }

    pub fn scene_json(&self) -> Result<String, PlayerError> {
        Ok(encode_scene(&self.definition)?)
    }

    pub fn frame(&self) -> &FrameState {
        self.instance.frame()
    }

    /// Explicitly reclaim retired compatibility frame slots.
    ///
    /// Ordinary edits remain append-only and never renumber frame rows. This method
    /// recompiles the already-authoritative semantic definition at a maintenance
    /// checkpoint, preserves all durable execution-slot identities, and marks the
    /// frame for a full renderer/transport resynchronization.
    pub fn compact_retired_slots(&mut self) -> Result<ExecutionCompactionStats, PlayerError> {
        let compact = CompiledScene::compile(&self.definition)?;
        Ok(self.instance.compact_with_compiled(compact)?)
    }

    pub const fn layout_generation(&self) -> u64 {
        self.instance.layout_generation()
    }

    pub fn frame_slot_capacity(&self) -> usize {
        self.instance.frame_slot_capacity()
    }

    pub fn retired_frame_slot_count(&self) -> usize {
        self.instance.retired_frame_slot_count()
    }

    pub fn compaction_recommended(&self, policy: RetiredSlotCompactionPolicy) -> bool {
        self.instance.compaction_recommended(policy)
    }

    pub fn frame_slot_for_execution_slot(&self, slot: ExecutionSlotId) -> Option<FrameSlotId> {
        self.instance.frame_slot_for_execution_slot(slot)
    }

    pub fn execution_slot_for_frame_slot(&self, slot: FrameSlotId) -> Option<ExecutionSlotId> {
        self.instance.execution_slot_for_frame_slot(slot)
    }

    pub const fn next_sequence(&self) -> u64 {
        self.next_sequence
    }

    pub const fn last_transaction_stats(&self) -> PlayerTransactionStats {
        self.last_transaction_stats
    }

    pub fn last_execution_delta(&self) -> &ExecutionDelta {
        self.instance.last_execution_delta()
    }

    pub(crate) fn take_execution_delta(&mut self) -> ExecutionDelta {
        self.instance.take_execution_delta()
    }

    pub(crate) fn execution_slot_for_frame_index(
        &self,
        frame_index: usize,
    ) -> Option<ExecutionSlotId> {
        self.instance.slot_for_frame_index(frame_index)
    }

    pub(crate) fn frame_index_for_execution_slot(&self, slot: ExecutionSlotId) -> Option<usize> {
        self.instance.frame_index_for_slot(slot)
    }

    pub fn object_count(&self) -> usize {
        self.instance.live_object_count()
    }

    pub fn hit_test(&self, point: Vec2) -> noon_runtime::SpatialQueryResult {
        self.instance.hit_test(point)
    }

    pub fn query_viewport(&self, bounds: Rect) -> noon_runtime::SpatialQueryResult {
        self.instance.query_viewport(bounds)
    }

    pub(crate) fn live_frame_indices(&self) -> Vec<usize> {
        self.instance.live_frame_indices()
    }
}

fn scene_diff(current: &SceneDefinition, desired: &SceneDefinition) -> Option<Vec<ScenePatch>> {
    let current_objects = current
        .objects()
        .iter()
        .map(|object| (object.id, object))
        .collect::<BTreeMap<_, _>>();
    let desired_objects = desired
        .objects()
        .iter()
        .map(|object| (object.id, object))
        .collect::<BTreeMap<_, _>>();
    if !append_compatible(
        current.objects().iter().map(|object| object.id),
        desired.objects().iter().map(|object| object.id),
    ) {
        return None;
    }
    for object in desired.objects() {
        if let Some(existing) = current_objects.get(&object.id) {
            if existing.geometry != object.geometry {
                return None;
            }
        }
    }

    let current_tracks = current
        .tracks()
        .iter()
        .map(|track| (track.id, track))
        .collect::<BTreeMap<_, _>>();
    let desired_tracks = desired
        .tracks()
        .iter()
        .map(|track| (track.id, track))
        .collect::<BTreeMap<_, _>>();
    if !append_compatible(
        current.tracks().iter().map(|track| track.id),
        desired.tracks().iter().map(|track| track.id),
    ) {
        return None;
    }

    let removed_objects = current_objects
        .keys()
        .filter(|id| !desired_objects.contains_key(id))
        .copied()
        .collect::<BTreeSet<ObjectId>>();
    let mut patches = Vec::new();
    for (id, track) in &current_tracks {
        if !desired_tracks.contains_key(id) && !removed_objects.contains(&track.object) {
            patches.push(ScenePatch::RemoveTrack(*id));
        }
    }
    for id in &removed_objects {
        patches.push(ScenePatch::RemoveObject(*id));
    }
    for object in desired.objects() {
        let id = object.id;
        match current_objects.get(&id) {
            Some(existing) => {
                if existing.transform != object.transform {
                    patches.push(ScenePatch::SetTransform {
                        object: id,
                        transform: object.transform,
                    });
                }
                if existing.style != object.style {
                    patches.push(ScenePatch::SetStyle {
                        object: id,
                        style: object.style,
                    });
                }
            }
            None => patches.push(ScenePatch::CreateObject(object.clone())),
        }
    }
    for track in desired.tracks() {
        match current_tracks.get(&track.id) {
            Some(existing) if **existing != *track => {
                patches.push(ScenePatch::ReplaceTrack(track.clone()));
            }
            None => patches.push(ScenePatch::AddTrack(track.clone())),
            _ => {}
        }
    }
    Some(patches)
}

fn append_compatible<Id: Copy + Ord>(
    current: impl Iterator<Item = Id>,
    desired: impl Iterator<Item = Id>,
) -> bool {
    let current = current.collect::<Vec<_>>();
    let desired = desired.collect::<Vec<_>>();
    let current_set = current.iter().copied().collect::<BTreeSet<_>>();
    let desired_set = desired.iter().copied().collect::<BTreeSet<_>>();
    let retained = current
        .into_iter()
        .filter(|id| desired_set.contains(id))
        .collect::<Vec<_>>();
    let desired_existing = desired
        .iter()
        .copied()
        .filter(|id| current_set.contains(id))
        .collect::<Vec<_>>();
    retained == desired_existing && desired.iter().take(retained.len()).copied().eq(retained)
}
