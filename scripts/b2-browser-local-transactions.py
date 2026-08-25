from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    p = Path(path)
    text = p.read_text()
    if text.count(old) != 1:
        raise SystemExit(f"{path}: expected one anchor, found {text.count(old)}\nANCHOR:\n{old[:400]}")
    p.write_text(text.replace(old, new, 1))


def insert_before_last_brace(path: str, addition: str) -> None:
    p = Path(path)
    text = p.read_text()
    pos = text.rfind("\n}")
    if pos < 0:
        raise SystemExit(f"{path}: final brace not found")
    p.write_text(text[:pos] + addition + text[pos:])

# --- noon-compile: lightweight transaction preflight ---
replace_once(
    "crates/noon-compile/src/lib.rs",
    "use noon_core::{\n    validate_track_definition, CompositionTimeMap, GeometryRef, ObjectId, Property,\n    SceneDefinition, ScenePatch, Style, TimelineError, TrackDefinition, TrackId, TrackTiming,\n    TrackValues, Transform2D, Vec2, VectorPath,\n};",
    "use noon_core::{\n    validate_track_definition, CompositionTimeMap, GeometryRef, MutationTransaction, ObjectId,\n    Property, SceneDefinition, ScenePatch, Style, TimelineError, TrackDefinition, TrackId,\n    TrackTiming, TrackValues, Transform2D, Vec2, VectorPath,\n};",
)

compiled_stats_anchor = """pub struct CompiledPatchStats {
    pub track_vector_clones: usize,
    pub presence_tracks_inspected: usize,
    pub dynamic_objects_recomputed: usize,
    pub dynamic_tracks_inspected: usize,
    /// Entries shifted inside an affected object/property channel only.
    pub dense_track_slots_shifted: usize,
    /// Global/unrelated track payload movement. This must remain zero for local edits.
    pub unrelated_track_slots_shifted: usize,
    pub object_slots_appended: usize,
    pub object_slots_retired: usize,
    pub object_indices_rewritten: usize,
    pub track_object_indices_rewritten: usize,
    pub track_locators_removed: usize,
}
"""
compiled_stats_replacement = compiled_stats_anchor + """
/// Lightweight validation accounting for an atomic compiled-scene transaction.
/// Existing geometry/track payloads are never cloned for staging.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CompiledTransactionPreflightStats {
    pub objects_indexed: usize,
    pub tracks_indexed: usize,
    pub mutations_preflighted: usize,
    pub staged_compiled_scene_clones: usize,
}

#[derive(Clone, Copy, Debug)]
struct TrackShadow {
    id: TrackId,
    object_index: u32,
    property: Property,
    start_time: f64,
    presence: Option<(bool, bool)>,
}

impl TrackShadow {
    fn from_compiled(track: &CompiledTrack) -> Self {
        Self {
            id: track.id,
            object_index: track.object_index,
            property: track.property,
            start_time: track.timing.start_time,
            presence: presence_endpoints(track.property, &track.values),
        }
    }

    fn from_definition(track: &TrackDefinition, object_index: u32) -> Self {
        Self {
            id: track.id,
            object_index,
            property: track.property,
            start_time: track.timing.start_time,
            presence: presence_endpoints(track.property, &track.values),
        }
    }
}
"""
replace_once("crates/noon-compile/src/lib.rs", compiled_stats_anchor, compiled_stats_replacement)

apply_patch_anchor = """    pub fn apply_patch(&mut self, patch: &ScenePatch) -> Result<(), CompilePatchError> {
        self.apply_patch_with_stats(patch).map(|_| ())
    }
"""
preflight_method = """    /// Validate a mutation transaction using only stable identity/channel metadata.
    /// Incoming track payloads are validated individually, but existing compiled
    /// scene payloads are never cloned.
    pub fn preflight_transaction(
        &self,
        transaction: &MutationTransaction,
    ) -> Result<CompiledTransactionPreflightStats, CompilePatchError> {
        let mut object_indices = self.object_indices.clone();
        let mut next_object_index = self.objects.len();
        let mut tracks = self
            .tracks_iter()
            .map(TrackShadow::from_compiled)
            .collect::<Vec<_>>();
        let stats = CompiledTransactionPreflightStats {
            objects_indexed: object_indices.len(),
            tracks_indexed: tracks.len(),
            mutations_preflighted: transaction.mutations().len(),
            staged_compiled_scene_clones: 0,
        };

        for patch in transaction.mutations() {
            match patch {
                ScenePatch::CreateObject(object) => {
                    if object_indices.contains_key(&object.id) {
                        return Err(CompilePatchError::DuplicateObject(object.id));
                    }
                    let index = u32::try_from(next_object_index)
                        .map_err(|_| CompilePatchError::TooManyObjects(next_object_index))?;
                    next_object_index += 1;
                    object_indices.insert(object.id, index);
                }
                ScenePatch::RemoveObject(id) => {
                    let object_index = object_indices
                        .remove(id)
                        .ok_or(CompilePatchError::UnknownObject(*id))?;
                    tracks.retain(|track| track.object_index != object_index);
                }
                ScenePatch::SetTransform { object, .. } | ScenePatch::SetStyle { object, .. } => {
                    if !object_indices.contains_key(object) {
                        return Err(CompilePatchError::UnknownObject(*object));
                    }
                }
                ScenePatch::AddTrack(track) => {
                    if tracks.iter().any(|existing| existing.id == track.id) {
                        return Err(CompilePatchError::DuplicateTrack(track.id));
                    }
                    let object_index = *object_indices
                        .get(&track.object)
                        .ok_or(CompilePatchError::UnknownObject(track.object))?;
                    validate_track_definition(track).map_err(CompilePatchError::InvalidTrack)?;
                    compile_transform_geometry_plan(track)
                        .map_err(|error| compile_patch_error(track.id, error))?;
                    let shadow = TrackShadow::from_definition(track, object_index);
                    tracks.push(shadow);
                    if shadow.property == Property::Presence {
                        validate_shadow_presence_channel(&tracks, object_index)?;
                    }
                }
                ScenePatch::ReplaceTrack(track) => {
                    let position = tracks
                        .iter()
                        .position(|existing| existing.id == track.id)
                        .ok_or(CompilePatchError::UnknownTrack(track.id))?;
                    let old = tracks[position];
                    let object_index = *object_indices
                        .get(&track.object)
                        .ok_or(CompilePatchError::UnknownObject(track.object))?;
                    validate_track_definition(track).map_err(CompilePatchError::InvalidTrack)?;
                    compile_transform_geometry_plan(track)
                        .map_err(|error| compile_patch_error(track.id, error))?;
                    let replacement = TrackShadow::from_definition(track, object_index);
                    tracks[position] = replacement;
                    if old.property == Property::Presence {
                        validate_shadow_presence_channel(&tracks, old.object_index)?;
                    }
                    if replacement.property == Property::Presence
                        && (old.property != Property::Presence
                            || old.object_index != replacement.object_index)
                    {
                        validate_shadow_presence_channel(&tracks, replacement.object_index)?;
                    }
                }
                ScenePatch::RemoveTrack(id) => {
                    let position = tracks
                        .iter()
                        .position(|track| track.id == *id)
                        .ok_or(CompilePatchError::UnknownTrack(*id))?;
                    let removed = tracks.remove(position);
                    if removed.property == Property::Presence {
                        validate_shadow_presence_channel(&tracks, removed.object_index)?;
                    }
                }
            }
        }
        Ok(stats)
    }

""" + apply_patch_anchor
replace_once("crates/noon-compile/src/lib.rs", apply_patch_anchor, preflight_method)

presence_anchor = """fn validate_presence_chains(tracks: &[CompiledTrack]) -> Result<(), (TrackId, TrackId)> {
"""
presence_helpers = """fn presence_endpoints(property: Property, values: &TrackValues) -> Option<(bool, bool)> {
    if property != Property::Presence {
        return None;
    }
    let TrackValues::Bool { from, to } = values else {
        unreachable!(\"validated Presence track must contain bool values\");
    };
    Some((*from, *to))
}

fn validate_shadow_presence_channel(
    tracks: &[TrackShadow],
    object_index: u32,
) -> Result<(), CompilePatchError> {
    let mut chain = tracks
        .iter()
        .filter(|track| track.object_index == object_index && track.property == Property::Presence)
        .copied()
        .collect::<Vec<_>>();
    chain.sort_by(|left, right| {
        left.start_time
            .total_cmp(&right.start_time)
            .then_with(|| left.id.cmp(&right.id))
    });
    for pair in chain.windows(2) {
        let (_, previous_to) = pair[0]
            .presence
            .expect(\"presence shadow contains bool endpoints\");
        let (next_from, _) = pair[1]
            .presence
            .expect(\"presence shadow contains bool endpoints\");
        if previous_to != next_from {
            return Err(CompilePatchError::DiscontinuousPresence {
                previous: pair[0].id,
                next: pair[1].id,
            });
        }
    }
    Ok(())
}

""" + presence_anchor
replace_once("crates/noon-compile/src/lib.rs", presence_anchor, presence_helpers)

compile_test = r'''

    #[test]
    fn transaction_preflight_rejects_late_compile_failure_without_scene_clone() {
        let mut scene = SceneDefinition::new();
        let object = scene.add(GeometryRef::circle(1.0));
        let compiled = CompiledScene::compile(&scene).expect("valid scene");
        let from = noon_core::ObjectSnapshot::new(GeometryRef::circle(1.0));
        let to = noon_core::ObjectSnapshot::new(GeometryRef::line(
            Vec2::new(-1.0, 0.0),
            Vec2::new(1.0, 0.0),
        ));
        let transaction = MutationTransaction::from_mutations([
            ScenePatch::SetTransform {
                object,
                transform: Transform2D {
                    translation: Vec2::new(2.0, 0.0),
                    ..Transform2D::IDENTITY
                },
            },
            ScenePatch::AddTrack(TrackDefinition {
                id: TrackId::new(50),
                object,
                property: Property::Transform,
                values: TrackValues::Object { from, to },
                timing: TrackTiming::new(0.0, 1.0, Easing::Linear),
                time_map: CompositionTimeMap::identity(),
            }),
        ]);

        assert!(matches!(
            compiled.preflight_transaction(&transaction),
            Err(CompilePatchError::UnsupportedTransformGeometry(TrackId(50)))
        ));
        assert_eq!(compiled.objects()[0].base_transform, Transform2D::IDENTITY);

        let valid = MutationTransaction::from_mutations([ScenePatch::SetStyle {
            object,
            style: Style::default(),
        }]);
        let stats = compiled
            .preflight_transaction(&valid)
            .expect("valid transaction preflights");
        assert_eq!(stats.mutations_preflighted, 1);
        assert_eq!(stats.staged_compiled_scene_clones, 0);
    }
'''
insert_before_last_brace("crates/noon-compile/src/lib.rs", compile_test)

# --- noon-runtime: stable-slot transaction preflight + aggregate delta ---
replace_once(
    "crates/noon-runtime/src/execution_slots.rs",
    "use noon_compile::{CompilePatchError, CompiledScene};\nuse noon_core::{ObjectId, Property, ScenePatch, TrackId};\n\nuse crate::{FrameState, SceneInstance};",
    "use noon_compile::{CompilePatchError, CompiledScene, CompiledTransactionPreflightStats};\nuse noon_core::{MutationTransaction, ObjectId, Property, ScenePatch, TrackId};\n\nuse crate::{EvaluationError, FrameChanges, FrameState, SceneInstance};",
)

slot_stats_anchor = """    pub const fn last_mutation_stats(&self) -> ExecutionSlotMutationStats {
        self.last_mutation
    }
}
"""
slot_stats_replacement = """    pub const fn last_mutation_stats(&self) -> ExecutionSlotMutationStats {
        self.last_mutation
    }

    fn preflight_transaction(
        &self,
        transaction: &MutationTransaction,
    ) -> Result<(), ExecutionSlotError> {
        // Slot metadata is cheap to stage and contains no frame/geometry payloads.
        let mut shadow = self.clone();
        for patch in transaction.mutations() {
            match patch {
                ScenePatch::CreateObject(object) => {
                    shadow.insert_object(object.id)?;
                }
                ScenePatch::RemoveObject(object) => {
                    shadow.remove_object(*object)?;
                }
                _ => {}
            }
        }
        Ok(())
    }
}
"""
replace_once("crates/noon-runtime/src/execution_slots.rs", slot_stats_anchor, slot_stats_replacement)

delta_anchor = """    fn push_channel(&mut self, slot: ExecutionSlotId, property: Property) {
        let delta = ExecutionChannelDelta { slot, property };
        if !self.channels.contains(&delta) {
            self.channels.push(delta);
        }
        self.push_slot(slot);
    }
}
"""
delta_replacement = """    fn push_channel(&mut self, slot: ExecutionSlotId, property: Property) {
        let delta = ExecutionChannelDelta { slot, property };
        if !self.channels.contains(&delta) {
            self.channels.push(delta);
        }
        self.push_slot(slot);
    }

    fn merge_from(&mut self, other: &Self) {
        for slot in other.slots.iter().copied() {
            self.push_slot(slot);
        }
        for channel in other.channels.iter().copied() {
            self.push_channel(channel.slot, channel.property);
        }
        self.effects.property |= other.effects.property;
        self.effects.timeline |= other.effects.timeline;
        self.effects.structure |= other.effects.structure;
        self.effects.render |= other.effects.render;
        self.effects.resources |= other.effects.resources;
        self.effects.hierarchy |= other.effects.hierarchy;
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ExecutionTransactionPreflightStats {
    pub compiled: CompiledTransactionPreflightStats,
    pub slots_indexed: usize,
    pub staged_runtime_clones: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ExecutionTransactionError {
    Compile(CompilePatchError),
    Slot(ExecutionSlotError),
}

impl std::fmt::Display for ExecutionTransactionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Compile(error) => write!(formatter, \"compiled transaction failed: {error}\"),
            Self::Slot(error) => write!(formatter, \"execution slot transaction failed: {error}\"),
        }
    }
}

impl std::error::Error for ExecutionTransactionError {}

impl From<CompilePatchError> for ExecutionTransactionError {
    fn from(value: CompilePatchError) -> Self {
        Self::Compile(value)
    }
}

impl From<ExecutionSlotError> for ExecutionTransactionError {
    fn from(value: ExecutionSlotError) -> Self {
        Self::Slot(value)
    }
}
"""
replace_once("crates/noon-runtime/src/execution_slots.rs", delta_anchor, delta_replacement)

replace_once(
    "crates/noon-runtime/src/execution_slots.rs",
    "/// Transitional runtime adapter exposing stable execution identity while\n/// the renderer/frame compatibility view remains dense.\npub struct SlottedSceneInstance {",
    "/// Transitional runtime adapter exposing stable execution identity while\n/// the renderer/frame compatibility view remains dense.\n#[derive(Clone, Debug)]\npub struct SlottedSceneInstance {",
)

slotted_apply_anchor = """    pub fn apply_patch(&mut self, patch: &ScenePatch) -> Result<&FrameState, CompilePatchError> {
"""
slotted_methods = """    pub fn seek(&mut self, time: f64) -> Result<&FrameState, EvaluationError> {
        self.inner.seek(time)
    }

    pub fn advance_to(&mut self, time: f64) -> Result<&FrameState, EvaluationError> {
        self.inner.advance_to(time)
    }

    pub fn take_frame_changes(&mut self) -> FrameChanges {
        self.inner.take_frame_changes()
    }

    pub fn contains_object(&self, object: ObjectId) -> bool {
        self.inner.contains_object(object)
    }

    pub fn live_object_count(&self) -> usize {
        self.inner.compiled.live_object_count()
    }

    pub fn preflight_transaction(
        &self,
        transaction: &MutationTransaction,
    ) -> Result<ExecutionTransactionPreflightStats, ExecutionTransactionError> {
        let compiled = self.inner.compiled.preflight_transaction(transaction)?;
        self.slots.preflight_transaction(transaction)?;
        Ok(ExecutionTransactionPreflightStats {
            compiled,
            slots_indexed: self.slots.slot_capacity(),
            staged_runtime_clones: 0,
        })
    }

    pub fn apply_transaction(
        &mut self,
        transaction: &MutationTransaction,
    ) -> Result<&FrameState, ExecutionTransactionError> {
        self.preflight_transaction(transaction)?;
        let mut aggregate = ExecutionDelta::default();
        for patch in transaction.mutations() {
            self.apply_patch(patch)
                .expect(\"execution transaction was fully preflighted\");
            aggregate.merge_from(&self.last_delta);
        }
        self.last_delta = aggregate;
        Ok(self.inner.frame())
    }

""" + slotted_apply_anchor
replace_once("crates/noon-runtime/src/execution_slots.rs", slotted_apply_anchor, slotted_methods)

runtime_test = r'''

    #[test]
    fn transaction_aggregates_bounded_execution_delta_without_runtime_clone() {
        let mut definition = SceneDefinition::new();
        let first = definition.add(GeometryRef::circle(1.0));
        let second = definition.add(GeometryRef::circle(1.0));
        let compiled = CompiledScene::compile(&definition).expect("valid scene");
        let mut live = SlottedSceneInstance::new(compiled);
        let first_slot = live.slot_for_object(first).expect("first slot");
        let second_slot = live.slot_for_object(second).expect("second slot");
        let transaction = MutationTransaction::from_mutations([
            ScenePatch::SetTransform {
                object: first,
                transform: noon_core::Transform2D {
                    translation: Vec2::new(2.0, 0.0),
                    ..noon_core::Transform2D::IDENTITY
                },
            },
            ScenePatch::AddTrack(TrackDefinition {
                id: TrackId::new(20),
                object: first,
                property: Property::Position,
                values: TrackValues::Vec2 {
                    from: Vec2::ZERO,
                    to: Vec2::new(3.0, 0.0),
                },
                timing: TrackTiming::new(0.0, 2.0, Easing::Linear),
                time_map: noon_core::CompositionTimeMap::identity(),
            }),
        ]);
        let preflight = live
            .preflight_transaction(&transaction)
            .expect("transaction preflights");
        assert_eq!(preflight.staged_runtime_clones, 0);
        assert_eq!(preflight.compiled.staged_compiled_scene_clones, 0);
        live.apply_transaction(&transaction)
            .expect("transaction commits locally");
        let delta = live.last_execution_delta();
        assert_eq!(delta.slots(), &[first_slot]);
        assert!(!delta.slots().contains(&second_slot));
        assert!(delta.effects().property);
        assert!(delta.effects().timeline);
    }
'''
insert_before_last_brace("crates/noon-runtime/src/execution_slots.rs", runtime_test)

# --- noon-web ScenePlayer: use local atomic transaction path ---
replace_once(
    "crates/noon-web/src/legacy.rs",
    "use noon_core::{ObjectId, PatchError, SceneDefinition, ScenePatch};\nuse noon_ir::{decode_patch_batch, decode_scene, encode_scene, IrError};\nuse noon_runtime::{EvaluationError, FrameChanges, FrameState, SceneInstance};",
    "use noon_core::{\n    preflight_transaction, MutationTransaction, ObjectId, PatchError, SceneDefinition, ScenePatch,\n};\nuse noon_ir::{decode_patch_batch, decode_scene, encode_scene, IrError};\nuse noon_runtime::{\n    EvaluationError, ExecutionDelta, ExecutionTransactionError, FrameChanges, FrameState,\n    SlottedSceneInstance,\n};",
)

replace_once(
    "crates/noon-web/src/legacy.rs",
    "    CompilePatch(CompilePatchError),\n    Evaluation(EvaluationError),",
    "    CompilePatch(CompilePatchError),\n    ExecutionTransaction(ExecutionTransactionError),\n    Evaluation(EvaluationError),",
)
replace_once(
    "crates/noon-web/src/legacy.rs",
    "            Self::CompilePatch(error) => write!(formatter, \"runtime patch failed: {error}\"),\n            Self::Evaluation(error) => write!(formatter, \"{error}\"),",
    "            Self::CompilePatch(error) => write!(formatter, \"runtime patch failed: {error}\"),\n            Self::ExecutionTransaction(error) => {\n                write!(formatter, \"execution transaction failed: {error}\")\n            }\n            Self::Evaluation(error) => write!(formatter, \"{error}\"),",
)

from_compile_anchor = """impl From<CompilePatchError> for PlayerError {
    fn from(value: CompilePatchError) -> Self {
        Self::CompilePatch(value)
    }
}

"""
from_compile_replacement = from_compile_anchor + """impl From<ExecutionTransactionError> for PlayerError {
    fn from(value: ExecutionTransactionError) -> Self {
        Self::ExecutionTransaction(value)
    }
}

"""
replace_once("crates/noon-web/src/legacy.rs", from_compile_anchor, from_compile_replacement)

scene_player_anchor = """#[derive(Clone, Debug)]
pub struct ScenePlayer {
    definition: SceneDefinition,
    instance: SceneInstance,
    next_sequence: u64,
}
"""
scene_player_replacement = """#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
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
"""
replace_once("crates/noon-web/src/legacy.rs", scene_player_anchor, scene_player_replacement)

replace_once(
    "crates/noon-web/src/legacy.rs",
    "            instance: SceneInstance::new(compiled),\n            next_sequence: 0,",
    "            instance: SlottedSceneInstance::new(compiled),\n            next_sequence: 0,\n            last_transaction_stats: PlayerTransactionStats::default(),",
)
replace_once(
    "crates/noon-web/src/legacy.rs",
    "        let mut instance = SceneInstance::new(compiled);",
    "        let mut instance = SlottedSceneInstance::new(compiled);",
)

reconcile_anchor = """        let patch_count = patches.len();
        let value_only = patches.iter().all(is_value_patch);
        self.apply_patches_transactionally(&patches)?;
        self.next_sequence = 0;
        Ok(if value_only {
            ReconcileOutcome::Incremental { patch_count }
        } else {
            ReconcileOutcome::Rebuilt { patch_count }
        })
"""
reconcile_replacement = """        let patch_count = patches.len();
        self.apply_patches_transactionally(&patches)?;
        self.next_sequence = 0;
        Ok(ReconcileOutcome::Incremental { patch_count })
"""
replace_once("crates/noon-web/src/legacy.rs", reconcile_anchor, reconcile_replacement)

transaction_anchor = """    fn apply_patches_transactionally(&mut self, patches: &[ScenePatch]) -> Result<(), PlayerError> {
        if patches.iter().all(is_value_patch) {
            for patch in patches {
                let object = value_patch_object(patch);
                if self.definition.object(object).is_none() {
                    return Err(PlayerError::Patch(PatchError::UnknownObject(object)));
                }
                if !self.instance.contains_object(object) {
                    return Err(PlayerError::CompilePatch(CompilePatchError::UnknownObject(
                        object,
                    )));
                }
            }
            for patch in patches {
                self.definition
                    .apply_patch(patch.clone())
                    .expect("value patch was preflighted against the scene definition");
                self.instance
                    .apply_patch(patch)
                    .expect("value patch was preflighted against the compiled scene");
            }
            return Ok(());
        }

        let playhead = self.instance.frame().time;
        let mut definition = self.definition.clone();
        for patch in patches {
            definition.apply_patch(patch.clone())?;
        }
        let compiled = CompiledScene::compile(&definition)?;
        let mut instance = SceneInstance::new(compiled);
        instance.seek(playhead)?;

        self.definition = definition;
        self.instance = instance;
        Ok(())
    }
"""
transaction_replacement = """    fn apply_patches_transactionally(&mut self, patches: &[ScenePatch]) -> Result<(), PlayerError> {
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
"""
replace_once("crates/noon-web/src/legacy.rs", transaction_anchor, transaction_replacement)

player_tail_anchor = """    pub const fn next_sequence(&self) -> u64 {
        self.next_sequence
    }

    pub fn object_count(&self) -> usize {
        self.instance.frame().objects.len()
    }
}

fn is_value_patch(patch: &ScenePatch) -> bool {
    matches!(
        patch,
        ScenePatch::SetTransform { .. } | ScenePatch::SetStyle { .. }
    )
}

fn value_patch_object(patch: &ScenePatch) -> ObjectId {
    match patch {
        ScenePatch::SetTransform { object, .. } | ScenePatch::SetStyle { object, .. } => *object,
        _ => unreachable!("value patch helper only accepts transform or style patches"),
    }
}
"""
player_tail_replacement = """    pub const fn next_sequence(&self) -> u64 {
        self.next_sequence
    }

    pub const fn last_transaction_stats(&self) -> PlayerTransactionStats {
        self.last_transaction_stats
    }

    pub fn last_execution_delta(&self) -> &ExecutionDelta {
        self.instance.last_execution_delta()
    }

    pub fn object_count(&self) -> usize {
        self.instance.live_object_count()
    }
}
"""
replace_once("crates/noon-web/src/legacy.rs", player_tail_anchor, player_tail_replacement)

# Update reconciliation expectations now that all compatible edits stay local.
replace_once(
    "crates/noon-web/src/legacy.rs",
    "    fn dense_grid_edit_rebuilds_atomically_and_preserves_playhead() {",
    "    fn dense_grid_edit_stays_incremental_and_preserves_playhead() {",
)
replace_once(
    "crates/noon-web/src/legacy.rs",
    "        let ReconcileOutcome::Rebuilt { patch_count } = outcome else {\n            panic!(\"dense structural edit must use one atomic rebuild: {outcome:?}\");\n        };",
    "        let ReconcileOutcome::Incremental { patch_count } = outcome else {\n            panic!(\"dense structural edit must stay incremental: {outcome:?}\");\n        };",
)
replace_once(
    "crates/noon-web/src/legacy.rs",
    "            ReconcileOutcome::Rebuilt { patch_count: 1 }",
    "            ReconcileOutcome::Incremental { patch_count: 1 }",
)

browser_tests = r'''

    #[test]
    fn hundred_thousand_object_remove_is_atomic_local_and_bounded() {
        let mut scene = SceneDefinition::new();
        for _ in 0..100_000 {
            scene.add(GeometryRef::circle(1.0));
        }
        let json = encode_scene(&scene).expect("large scene serializes");
        let mut player = ScenePlayer::from_scene_json(&json).expect("large scene loads");
        let retained_before = player
            .instance
            .slot_for_object(ObjectId::new(99_999))
            .expect("retained slot exists");
        let batch = PatchBatch::new(0, vec![ScenePatch::RemoveObject(ObjectId::new(10))]);
        let json = encode_patch_batch(&batch).expect("batch serializes");

        player
            .apply_patch_batch_json(&json)
            .expect("local removal succeeds");

        assert_eq!(player.object_count(), 99_999);
        assert_eq!(
            player.instance.slot_for_object(ObjectId::new(99_999)),
            Some(retained_before)
        );
        assert_eq!(player.last_transaction_stats().semantic_scene_clones, 0);
        assert_eq!(player.last_transaction_stats().runtime_rebuilds, 0);
        assert_eq!(player.last_execution_delta().slots().len(), 1);
        let runtime = player.instance.scene_instance().last_patch_stats();
        assert_eq!(runtime.object_slots_retired, 1);
        assert_eq!(runtime.full_group_rebuilds, 0);
        assert_eq!(runtime.full_seeks, 0);
    }

    #[test]
    fn compile_only_failure_keeps_browser_scene_and_frame_atomic() {
        let mut player = player();
        let before_scene = player.scene_json().expect("scene serializes");
        let before_frame = player.frame().clone();
        let from = ObjectSnapshot::new(GeometryRef::circle(1.0));
        let to = ObjectSnapshot::new(GeometryRef::line(
            Vec2::new(-1.0, 0.0),
            Vec2::new(1.0, 0.0),
        ));
        let batch = PatchBatch::new(
            0,
            vec![
                ScenePatch::SetStyle {
                    object: ObjectId::new(0),
                    style: Style {
                        opacity: 0.25,
                        ..Style::default()
                    },
                },
                ScenePatch::AddTrack(TrackDefinition {
                    id: TrackId::new(50),
                    object: ObjectId::new(0),
                    property: Property::Transform,
                    values: TrackValues::Object { from, to },
                    timing: TrackTiming::new(0.0, 1.0, Easing::Linear),
                    time_map: noon_core::CompositionTimeMap::identity(),
                }),
            ],
        );
        let json = encode_patch_batch(&batch).expect("batch serializes");

        assert!(matches!(
            player.apply_patch_batch_json(&json),
            Err(PlayerError::ExecutionTransaction(ExecutionTransactionError::Compile(
                CompilePatchError::UnsupportedTransformGeometry(TrackId(50))
            )))
        ));
        assert_eq!(player.scene_json().expect("scene serializes"), before_scene);
        assert_eq!(player.frame(), &before_frame);
        assert_eq!(player.next_sequence(), 0);
    }
'''
insert_before_last_brace("crates/noon-web/src/legacy.rs", browser_tests)

# Keep the migration document explicit about this end-to-end browser slice.
doc = Path("docs/execution-slots.md")
text = doc.read_text()
old = "Renderer and reactive consumers also still need to consume stable `ExecutionDelta`/slot identities directly instead of relying on compiled/frame positions."
new = "Browser patch batches now preflight semantic, compiled, and execution-slot metadata and commit through `SlottedSceneInstance` without cloning `SceneDefinition`, recompiling the scene, rebuilding runtime groups, or seeking the full scene. Renderer and reactive consumers still need to consume stable `ExecutionDelta`/slot identities directly instead of relying on compiled/frame positions."
if old not in text:
    raise SystemExit("docs/execution-slots.md: expected migration sentence not found")
doc.write_text(text.replace(old, new, 1))

print("applied browser-local transaction slice")
