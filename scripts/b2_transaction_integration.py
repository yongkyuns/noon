from pathlib import Path


def replace_once(text: str, old: str, new: str, label: str) -> str:
    if old not in text:
        raise RuntimeError(f"missing replacement anchor: {label}")
    return text.replace(old, new, 1)


# --- noon-compile: local track mutation + lightweight transaction preflight ---
path = Path("crates/noon-compile/src/lib.rs")
text = path.read_text()
text = replace_once(
    text,
    "    validate_track_definition, CompositionTimeMap, GeometryRef, ObjectId, Property,\n    SceneDefinition, ScenePatch, Style, TimelineError, TrackDefinition, TrackId, TrackTiming,\n",
    "    validate_track_definition, CompositionTimeMap, GeometryRef, MutationTransaction, ObjectId,\n    Property, SceneDefinition, ScenePatch, Style, TimelineError, TrackDefinition, TrackId,\n    TrackTiming,\n",
    "compile imports",
)
text = replace_once(
    text,
    '''impl DynamicProperties {
    fn mark(&mut self, property: Property) {
        match property {
            Property::Presence => self.presence = true,
            Property::Transform => self.transform = true,
            Property::Position => self.position = true,
            Property::Rotation => self.rotation = true,
            Property::Opacity => self.opacity = true,
            Property::Appearance => self.appearance = true,
            Property::Reveal => self.reveal = true,
            Property::Morph => self.morph = true,
        }
    }
''',
    '''impl DynamicProperties {
    fn mark(&mut self, property: Property) {
        self.set(property, true);
    }

    fn set(&mut self, property: Property, value: bool) {
        match property {
            Property::Presence => self.presence = value,
            Property::Transform => self.transform = value,
            Property::Position => self.position = value,
            Property::Rotation => self.rotation = value,
            Property::Opacity => self.opacity = value,
            Property::Appearance => self.appearance = value,
            Property::Reveal => self.reveal = value,
            Property::Morph => self.morph = value,
        }
    }
''',
    "dynamic property setter",
)
text = replace_once(
    text,
    '''pub struct CompiledScene {
    objects: Vec<CompiledObject>,
    tracks: Vec<CompiledTrack>,
    object_indices: BTreeMap<ObjectId, u32>,
    free_object_indices: Vec<u32>,
}
''',
    '''pub struct CompiledScene {
    objects: Vec<CompiledObject>,
    tracks: Vec<CompiledTrack>,
    object_indices: BTreeMap<ObjectId, u32>,
    free_object_indices: Vec<u32>,
}

/// Lightweight validation accounting for an atomic compiled-scene transaction.
/// Heavy object/geometry payloads are never cloned for transaction staging.
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
            presence: presence_values(track.property, &track.values),
        }
    }

    fn from_definition(track: &TrackDefinition, object_index: u32) -> Self {
        Self {
            id: track.id,
            object_index,
            property: track.property,
            start_time: track.timing.start_time,
            presence: presence_values(track.property, &track.values),
        }
    }
}
''',
    "compiled transaction structs",
)
anchor = '''    pub fn apply_patch(&mut self, patch: &ScenePatch) -> Result<(), CompilePatchError> {
'''
preflight = r'''    /// Validate a transaction against lightweight identity/channel metadata only.
    /// Incoming tracks are compiled individually so transform-geometry failures
    /// are caught before any live runtime state is mutated.
    pub fn preflight_transaction(
        &self,
        transaction: &MutationTransaction,
    ) -> Result<CompiledTransactionPreflightStats, CompilePatchError> {
        let mut object_indices = self.object_indices.clone();
        let mut free_object_indices = self.free_object_indices.clone();
        let mut next_object_len = self.objects.len();
        let mut tracks = self
            .tracks
            .iter()
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
                    let index = if let Some(index) = free_object_indices.pop() {
                        index
                    } else {
                        let index = u32::try_from(next_object_len)
                            .map_err(|_| CompilePatchError::TooManyObjects(next_object_len))?;
                        next_object_len += 1;
                        index
                    };
                    object_indices.insert(object.id, index);
                }
                ScenePatch::RemoveObject(id) => {
                    let index = object_indices
                        .remove(id)
                        .ok_or(CompilePatchError::UnknownObject(*id))?;
                    tracks.retain(|track| track.object_index != index);
                    free_object_indices.push(index);
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
                        && (replacement.object_index != old.object_index
                            || old.property != Property::Presence)
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

'''
text = replace_once(text, anchor, preflight + anchor, "compiled preflight method")
text = replace_once(
    text,
    '''            ScenePatch::AddTrack(track) => {
                if self.tracks.iter().any(|existing| existing.id == track.id) {
                    return Err(CompilePatchError::DuplicateTrack(track.id));
                }
                let compiled = self.compile_patch_track(track)?;
                let mut tracks = self.tracks.clone();
                tracks.push(compiled);
                sort_tracks(&mut tracks);
                validate_presence_chains(&tracks).map_err(|(previous, next)| {
                    CompilePatchError::DiscontinuousPresence { previous, next }
                })?;
                self.tracks = tracks;
                self.recompute_dynamic();
            }
            ScenePatch::ReplaceTrack(track) => {
                let position = self
                    .tracks
                    .iter()
                    .position(|existing| existing.id == track.id)
                    .ok_or(CompilePatchError::UnknownTrack(track.id))?;
                let compiled = self.compile_patch_track(track)?;
                let mut tracks = self.tracks.clone();
                tracks[position] = compiled;
                sort_tracks(&mut tracks);
                validate_presence_chains(&tracks).map_err(|(previous, next)| {
                    CompilePatchError::DiscontinuousPresence { previous, next }
                })?;
                self.tracks = tracks;
                self.recompute_dynamic();
            }
            ScenePatch::RemoveTrack(id) => {
                let position = self
                    .tracks
                    .iter()
                    .position(|track| track.id == *id)
                    .ok_or(CompilePatchError::UnknownTrack(*id))?;
                let mut tracks = self.tracks.clone();
                tracks.remove(position);
                validate_presence_chains(&tracks).map_err(|(previous, next)| {
                    CompilePatchError::DiscontinuousPresence { previous, next }
                })?;
                self.tracks = tracks;
                self.recompute_dynamic();
            }
''',
    '''            ScenePatch::AddTrack(track) => {
                if self.tracks.iter().any(|existing| existing.id == track.id) {
                    return Err(CompilePatchError::DuplicateTrack(track.id));
                }
                let compiled = self.compile_patch_track(track)?;
                if compiled.property == Property::Presence {
                    validate_presence_patch(&self.tracks, None, Some(&compiled), compiled.object_index)?;
                }
                let object_index = compiled.object_index;
                let property = compiled.property;
                insert_track_sorted(&mut self.tracks, compiled);
                self.objects[object_index as usize].dynamic.mark(property);
            }
            ScenePatch::ReplaceTrack(track) => {
                let position = self
                    .tracks
                    .iter()
                    .position(|existing| existing.id == track.id)
                    .ok_or(CompilePatchError::UnknownTrack(track.id))?;
                let old_object_index = self.tracks[position].object_index;
                let old_property = self.tracks[position].property;
                let compiled = self.compile_patch_track(track)?;
                if old_property == Property::Presence {
                    validate_presence_patch(
                        &self.tracks,
                        Some(track.id),
                        (compiled.property == Property::Presence).then_some(&compiled),
                        old_object_index,
                    )?;
                }
                if compiled.property == Property::Presence && compiled.object_index != old_object_index {
                    validate_presence_patch(
                        &self.tracks,
                        Some(track.id),
                        Some(&compiled),
                        compiled.object_index,
                    )?;
                }
                let new_object_index = compiled.object_index;
                let new_property = compiled.property;
                self.tracks.remove(position);
                insert_track_sorted(&mut self.tracks, compiled);
                self.refresh_dynamic_property(old_object_index, old_property);
                self.objects[new_object_index as usize].dynamic.mark(new_property);
            }
            ScenePatch::RemoveTrack(id) => {
                let position = self
                    .tracks
                    .iter()
                    .position(|track| track.id == *id)
                    .ok_or(CompilePatchError::UnknownTrack(*id))?;
                let object_index = self.tracks[position].object_index;
                let property = self.tracks[position].property;
                if property == Property::Presence {
                    validate_presence_patch(&self.tracks, Some(*id), None, object_index)?;
                }
                self.tracks.remove(position);
                self.refresh_dynamic_property(object_index, property);
            }
''',
    "local compiled track mutation",
)
text = replace_once(
    text,
    '''    fn recompute_dynamic(&mut self) {
        for object in &mut self.objects {
            object.dynamic = DynamicProperties::default();
        }
        for track in &self.tracks {
            self.objects[track.object_index as usize]
                .dynamic
                .mark(track.property);
        }
    }
''',
    '''    fn refresh_dynamic_property(&mut self, object_index: u32, property: Property) {
        let dynamic = !self.track_group(object_index, property).is_empty();
        self.objects[object_index as usize].dynamic.set(property, dynamic);
    }
''',
    "local dynamic refresh",
)
text = replace_once(
    text,
    '''fn sort_tracks(tracks: &mut [CompiledTrack]) {
    tracks.sort_by(|left, right| {
        left.object_index
            .cmp(&right.object_index)
            .then_with(|| property_rank(left.property).cmp(&property_rank(right.property)))
            .then_with(|| left.timing.start_time.total_cmp(&right.timing.start_time))
            .then_with(|| left.id.cmp(&right.id))
    });
}
''',
    '''fn compare_tracks(left: &CompiledTrack, right: &CompiledTrack) -> std::cmp::Ordering {
    left.object_index
        .cmp(&right.object_index)
        .then_with(|| property_rank(left.property).cmp(&property_rank(right.property)))
        .then_with(|| left.timing.start_time.total_cmp(&right.timing.start_time))
        .then_with(|| left.id.cmp(&right.id))
}

fn sort_tracks(tracks: &mut [CompiledTrack]) {
    tracks.sort_by(compare_tracks);
}

fn insert_track_sorted(tracks: &mut Vec<CompiledTrack>, track: CompiledTrack) {
    let position = tracks
        .binary_search_by(|existing| compare_tracks(existing, &track))
        .unwrap_or_else(|position| position);
    tracks.insert(position, track);
}

fn presence_values(property: Property, values: &TrackValues) -> Option<(bool, bool)> {
    if property != Property::Presence {
        return None;
    }
    let TrackValues::Bool { from, to } = values else {
        unreachable!("validated Presence track must contain bool values");
    };
    Some((*from, *to))
}

fn validate_presence_patch(
    tracks: &[CompiledTrack],
    remove_id: Option<TrackId>,
    addition: Option<&CompiledTrack>,
    object_index: u32,
) -> Result<(), CompilePatchError> {
    let mut chain = tracks
        .iter()
        .filter(|track| {
            track.object_index == object_index
                && track.property == Property::Presence
                && Some(track.id) != remove_id
        })
        .collect::<Vec<_>>();
    if let Some(addition) = addition.filter(|track| {
        track.object_index == object_index && track.property == Property::Presence
    }) {
        chain.push(addition);
    }
    chain.sort_by(|left, right| {
        left.timing
            .start_time
            .total_cmp(&right.timing.start_time)
            .then_with(|| left.id.cmp(&right.id))
    });
    for pair in chain.windows(2) {
        let (_, previous_to) = presence_values(pair[0].property, &pair[0].values)
            .expect("presence chain contains presence tracks");
        let (next_from, _) = presence_values(pair[1].property, &pair[1].values)
            .expect("presence chain contains presence tracks");
        if previous_to != next_from {
            return Err(CompilePatchError::DiscontinuousPresence {
                previous: pair[0].id,
                next: pair[1].id,
            });
        }
    }
    Ok(())
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
            .expect("presence shadow contains bool endpoints");
        let (next_from, _) = pair[1]
            .presence
            .expect("presence shadow contains bool endpoints");
        if previous_to != next_from {
            return Err(CompilePatchError::DiscontinuousPresence {
                previous: pair[0].id,
                next: pair[1].id,
            });
        }
    }
    Ok(())
}
''',
    "compiled track helpers",
)
path.write_text(text)

# Compile-level acceptance tests.
Path("crates/noon-compile/tests/transaction_preflight.rs").write_text(r'''use noon_compile::CompiledScene;
use noon_core::{
    Easing, GeometryRef, MutationTransaction, ObjectId, Property, SceneDefinition, ScenePatch,
    TrackDefinition, TrackId, TrackTiming, TrackValues, Vec2,
};

#[test]
fn hundred_thousand_object_transaction_preflight_clones_no_compiled_scene() {
    let mut scene = SceneDefinition::new();
    for _ in 0..100_000 {
        scene.add(GeometryRef::circle(1.0));
    }
    let compiled = CompiledScene::compile(&scene).expect("scene compiles");
    let transaction = MutationTransaction::from_mutations([ScenePatch::RemoveObject(
        ObjectId::new(10),
    )]);
    let stats = compiled
        .preflight_transaction(&transaction)
        .expect("removal preflights");
    assert_eq!(stats.objects_indexed, 100_000);
    assert_eq!(stats.mutations_preflighted, 1);
    assert_eq!(stats.staged_compiled_scene_clones, 0);
    assert_eq!(compiled.object_index(ObjectId::new(10)), Some(10));
}

#[test]
fn transaction_preflight_rejects_late_invalid_track_without_mutation() {
    let mut scene = SceneDefinition::new();
    let object = scene.add(GeometryRef::circle(1.0));
    let compiled = CompiledScene::compile(&scene).expect("scene compiles");
    let before = compiled.clone();
    let transaction = MutationTransaction::from_mutations([
        ScenePatch::AddTrack(TrackDefinition {
            id: TrackId::new(10),
            object,
            property: Property::Position,
            values: TrackValues::Vec2 {
                from: Vec2::ZERO,
                to: Vec2::ONE,
            },
            timing: TrackTiming::new(0.0, 1.0, Easing::Linear),
            time_map: noon_core::CompositionTimeMap::identity(),
        }),
        ScenePatch::AddTrack(TrackDefinition {
            id: TrackId::new(11),
            object: ObjectId::new(999),
            property: Property::Position,
            values: TrackValues::Vec2 {
                from: Vec2::ZERO,
                to: Vec2::ONE,
            },
            timing: TrackTiming::new(0.0, 1.0, Easing::Linear),
            time_map: noon_core::CompositionTimeMap::identity(),
        }),
    ]);
    assert!(compiled.preflight_transaction(&transaction).is_err());
    assert_eq!(compiled, before);
}
''')

# --- noon-runtime: transaction API + slot-aware aggregated delta ---
path = Path("crates/noon-runtime/src/lib.rs")
text = path.read_text()
text = replace_once(
    text,
    "use noon_compile::{CompilePatchError, CompiledScene, CompiledTrack, TransformGeometryPlan};",
    "use noon_compile::{\n    CompilePatchError, CompiledScene, CompiledTrack, CompiledTransactionPreflightStats,\n    TransformGeometryPlan,\n};",
    "runtime compile imports",
)
text = replace_once(
    text,
    "    Color, GeometryRef, ObjectId, ObjectSnapshot, Property, ScenePatch, Style, TrackValues,\n",
    "    Color, GeometryRef, MutationTransaction, ObjectId, ObjectSnapshot, Property, ScenePatch,\n    Style, TrackValues,\n",
    "runtime core imports",
)
anchor = '''    pub fn apply_patch(&mut self, patch: &ScenePatch) -> Result<&FrameState, CompilePatchError> {
'''
transaction_methods = r'''    pub fn preflight_transaction(
        &self,
        transaction: &MutationTransaction,
    ) -> Result<CompiledTransactionPreflightStats, CompilePatchError> {
        self.compiled.preflight_transaction(transaction)
    }

    pub fn apply_transaction(
        &mut self,
        transaction: &MutationTransaction,
    ) -> Result<&FrameState, CompilePatchError> {
        self.preflight_transaction(transaction)?;
        let mut aggregate = RuntimePatchStats::default();
        for patch in transaction.mutations() {
            self.apply_patch(patch)
                .expect("compiled transaction was fully preflighted");
            aggregate.merge_from(self.last_patch_stats);
        }
        self.last_patch_stats = aggregate;
        Ok(&self.frame)
    }

'''
text = replace_once(text, anchor, transaction_methods + anchor, "runtime transaction methods")
text = replace_once(
    text,
    '''pub struct RuntimePatchStats {
    pub affected_objects: usize,
    pub groups_rebuilt: usize,
    pub scheduler_groups_rebuilt: usize,
    pub full_group_rebuilds: usize,
    pub full_scheduler_rebuilds: usize,
}
''',
    '''pub struct RuntimePatchStats {
    pub affected_objects: usize,
    pub groups_rebuilt: usize,
    pub scheduler_groups_rebuilt: usize,
    pub full_group_rebuilds: usize,
    pub full_scheduler_rebuilds: usize,
}

impl RuntimePatchStats {
    fn merge_from(&mut self, other: Self) {
        self.affected_objects += other.affected_objects;
        self.groups_rebuilt += other.groups_rebuilt;
        self.scheduler_groups_rebuilt += other.scheduler_groups_rebuilt;
        self.full_group_rebuilds += other.full_group_rebuilds;
        self.full_scheduler_rebuilds += other.full_scheduler_rebuilds;
    }
}
''',
    "runtime patch stats merge",
)
path.write_text(text)

path = Path("crates/noon-runtime/src/execution_slots.rs")
text = path.read_text()
text = replace_once(
    text,
    "use noon_compile::{CompilePatchError, CompiledScene};\nuse noon_core::{ObjectId, Property, ScenePatch, TrackId};\n\nuse crate::{FrameState, SceneInstance};",
    "use noon_compile::{\n    CompilePatchError, CompiledScene, CompiledTransactionPreflightStats,\n};\nuse noon_core::{MutationTransaction, ObjectId, Property, ScenePatch, TrackId};\n\nuse crate::{EvaluationError, FrameChanges, FrameState, SceneInstance};",
    "slot imports",
)
text = replace_once(
    text,
    '''        for object in compiled.objects() {
            table
                .insert_object(object.id)
                .expect("compiled scene object identities are unique");
        }
''',
    '''        for object in compiled.objects().iter().filter(|object| object.live) {
            table
                .insert_object(object.id)
                .expect("compiled scene object identities are unique");
        }
''',
    "skip compiled tombstones",
)
anchor = '''    pub const fn last_mutation_stats(&self) -> ExecutionSlotMutationStats {
        self.last_mutation
    }
}
'''
slot_preflight = '''    pub const fn last_mutation_stats(&self) -> ExecutionSlotMutationStats {
        self.last_mutation
    }

    fn preflight_transaction(
        &self,
        transaction: &MutationTransaction,
    ) -> Result<(), ExecutionSlotError> {
        // Slot metadata is intentionally cheap to stage: it contains only IDs,
        // generations, and free-list links—not frame, geometry, or runtime state.
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
'''
text = replace_once(text, anchor, slot_preflight, "slot transaction preflight")
text = replace_once(
    text,
    '''    fn push_channel(&mut self, slot: ExecutionSlotId, property: Property) {
        let delta = ExecutionChannelDelta { slot, property };
        if !self.channels.contains(&delta) {
            self.channels.push(delta);
        }
        self.push_slot(slot);
    }
}
''',
    '''    fn push_channel(&mut self, slot: ExecutionSlotId, property: Property) {
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
            Self::Compile(error) => write!(formatter, "compiled transaction failed: {error}"),
            Self::Slot(error) => write!(formatter, "execution slot transaction failed: {error}"),
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
''',
    "execution transaction types",
)
text = replace_once(
    text,
    '''/// Transitional runtime adapter exposing stable execution identity while
/// the renderer/frame compatibility view remains dense.
pub struct SlottedSceneInstance {
''',
    '''/// Transitional runtime adapter exposing stable execution identity while
/// the renderer/frame compatibility view remains dense.
#[derive(Clone, Debug)]
pub struct SlottedSceneInstance {
''',
    "slotted derives",
)
anchor = '''    pub fn apply_patch(&mut self, patch: &ScenePatch) -> Result<&FrameState, CompilePatchError> {
'''
slotted_methods = r'''    pub fn seek(&mut self, time: f64) -> Result<&FrameState, EvaluationError> {
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

    pub fn preflight_transaction(
        &self,
        transaction: &MutationTransaction,
    ) -> Result<ExecutionTransactionPreflightStats, ExecutionTransactionError> {
        let compiled = self.inner.preflight_transaction(transaction)?;
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
                .expect("execution transaction was fully preflighted");
            aggregate.merge_from(&self.last_delta);
        }
        self.last_delta = aggregate;
        Ok(self.inner.frame())
    }

'''
text = replace_once(text, anchor, slotted_methods + anchor, "slotted transaction methods")
# Localize RemoveObject channel capture instead of scanning every compiled track.
text = replace_once(
    text,
    '''                if matches!(patch, ScenePatch::RemoveObject(_)) {
                    for track in self.inner.compiled.tracks() {
                        let compiled_object =
                            &self.inner.compiled.objects()[track.object_index as usize];
                        if compiled_object.id == *object {
                            if let Some(slot) = context.primary_slot {
                                push_context_channel(
                                    &mut context.old_track_channels,
                                    slot,
                                    track.property,
                                );
                            }
                        }
                    }
                }
''',
    '''                if matches!(patch, ScenePatch::RemoveObject(_)) {
                    if let (Some(slot), Some(object_index)) =
                        (context.primary_slot, self.inner.compiled.object_index(*object))
                    {
                        for property in EXECUTION_PROPERTIES {
                            if !self.inner.compiled.track_group(object_index, property).is_empty() {
                                push_context_channel(
                                    &mut context.old_track_channels,
                                    slot,
                                    property,
                                );
                            }
                        }
                    }
                }
''',
    "local remove context",
)
text = replace_once(
    text,
    '''fn push_context_channel(
    channels: &mut Vec<ExecutionChannelDelta>,
''',
    '''const EXECUTION_PROPERTIES: [Property; 8] = [
    Property::Presence,
    Property::Transform,
    Property::Position,
    Property::Rotation,
    Property::Opacity,
    Property::Appearance,
    Property::Reveal,
    Property::Morph,
];

fn push_context_channel(
    channels: &mut Vec<ExecutionChannelDelta>,
''',
    "execution properties",
)
# Add transaction aggregation test before the test module's final brace using a stable existing test tail.
needle = '''    fn timeline_patch_reports_only_affected_slot_and_channel() {
'''
if needle not in text:
    raise RuntimeError("missing slot test anchor")
# Append test just before module final brace by using the final occurrence.
insert = r'''

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
        live.apply_transaction(&transaction)
            .expect("transaction commits locally");
        let delta = live.last_execution_delta();
        assert_eq!(delta.slots(), &[first_slot]);
        assert!(!delta.slots().contains(&second_slot));
        assert!(delta.effects().property);
        assert!(delta.effects().timeline);
    }
'''
idx = text.rfind("\n}")
if idx < 0:
    raise RuntimeError("missing slot module end")
text = text[:idx] + insert + text[idx:]
path.write_text(text)

# --- noon-web: make the slotted runtime the production player and remove full rebuild staging ---
path = Path("crates/noon-web/src/legacy.rs")
text = path.read_text()
text = replace_once(
    text,
    "use noon_core::{ObjectId, PatchError, SceneDefinition, ScenePatch};",
    "use noon_core::{\n    preflight_transaction, MutationTransaction, ObjectId, PatchError, SceneDefinition, ScenePatch,\n};",
    "web core imports",
)
text = replace_once(
    text,
    "use noon_runtime::{EvaluationError, FrameChanges, FrameState, SceneInstance};",
    "use noon_runtime::{\n    EvaluationError, ExecutionDelta, ExecutionTransactionError, FrameChanges, FrameState,\n    SlottedSceneInstance,\n};",
    "web runtime imports",
)
text = replace_once(
    text,
    '''    CompilePatch(CompilePatchError),
    Evaluation(EvaluationError),
''',
    '''    CompilePatch(CompilePatchError),
    ExecutionTransaction(ExecutionTransactionError),
    Evaluation(EvaluationError),
''',
    "player error variant",
)
text = replace_once(
    text,
    '''            Self::CompilePatch(error) => write!(formatter, "runtime patch failed: {error}"),
            Self::Evaluation(error) => write!(formatter, "scene evaluation failed: {error}"),
''',
    '''            Self::CompilePatch(error) => write!(formatter, "runtime patch failed: {error}"),
            Self::ExecutionTransaction(error) => {
                write!(formatter, "execution transaction failed: {error}")
            }
            Self::Evaluation(error) => write!(formatter, "scene evaluation failed: {error}"),
''',
    "player error display",
)
text = replace_once(
    text,
    '''impl From<EvaluationError> for PlayerError {
''',
    '''impl From<ExecutionTransactionError> for PlayerError {
    fn from(value: ExecutionTransactionError) -> Self {
        Self::ExecutionTransaction(value)
    }
}

impl From<EvaluationError> for PlayerError {
''',
    "player execution error conversion",
)
text = replace_once(
    text,
    '''#[derive(Clone, Debug)]
pub struct ScenePlayer {
    definition: SceneDefinition,
    instance: SceneInstance,
    next_sequence: u64,
}
''',
    '''#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
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
''',
    "slotted scene player",
)
text = replace_once(
    text,
    '''            definition,
            instance: SceneInstance::new(compiled),
            next_sequence: 0,
''',
    '''            definition,
            instance: SlottedSceneInstance::new(compiled),
            next_sequence: 0,
            last_transaction_stats: PlayerTransactionStats::default(),
''',
    "player construction",
)
text = text.replace("let mut instance = SceneInstance::new(compiled);", "let mut instance = SlottedSceneInstance::new(compiled);")
text = replace_once(
    text,
    '''        let patch_count = patches.len();
        let value_only = patches.iter().all(is_value_patch);
        self.apply_patches_transactionally(&patches)?;
        self.next_sequence = 0;
        Ok(if value_only {
            ReconcileOutcome::Incremental { patch_count }
        } else {
            ReconcileOutcome::Rebuilt { patch_count }
        })
''',
    '''        let patch_count = patches.len();
        self.apply_patches_transactionally(&patches)?;
        self.next_sequence = 0;
        Ok(ReconcileOutcome::Incremental { patch_count })
''',
    "incremental reconcile outcome",
)
old_tx = '''    fn apply_patches_transactionally(&mut self, patches: &[ScenePatch]) -> Result<(), PlayerError> {
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
'''
new_tx = '''    fn apply_patches_transactionally(&mut self, patches: &[ScenePatch]) -> Result<(), PlayerError> {
        let transaction = MutationTransaction::from_mutations(patches.iter().cloned());
        // Semantic and execution validation both happen before either live world
        // mutates. Neither stage clones the SceneDefinition or SceneInstance.
        preflight_transaction(&self.definition, &transaction)?;
        self.instance.preflight_transaction(&transaction)?;
        self.instance.apply_transaction(&transaction)?;
        self.definition
            .apply_transaction(&transaction)
            .expect("semantic transaction was fully preflighted");
        self.last_transaction_stats = PlayerTransactionStats {
            mutations: patches.len(),
            semantic_scene_clones: 0,
            runtime_rebuilds: 0,
        };
        Ok(())
    }
'''
text = replace_once(text, old_tx, new_tx, "browser local transaction")
text = replace_once(
    text,
    '''    pub const fn next_sequence(&self) -> u64 {
        self.next_sequence
    }

    pub fn object_count(&self) -> usize {
''',
    '''    pub const fn next_sequence(&self) -> u64 {
        self.next_sequence
    }

    pub const fn last_transaction_stats(&self) -> PlayerTransactionStats {
        self.last_transaction_stats
    }

    pub fn last_execution_delta(&self) -> &ExecutionDelta {
        self.instance.last_execution_delta()
    }

    pub fn object_count(&self) -> usize {
''',
    "player transaction observability",
)
# Remove obsolete value-only helpers.
start = text.find("fn is_value_patch(patch: &ScenePatch) -> bool {")
end = text.find("fn scene_diff(", start)
if start < 0 or end < 0:
    raise RuntimeError("missing obsolete web patch helpers")
text = text[:start] + text[end:]
# Update structural/timeline reconcile assertions to the new local path.
text = text.replace(
    '        let ReconcileOutcome::Rebuilt { patch_count } = outcome else {\n            panic!("dense structural edit must use one atomic rebuild: {outcome:?}");\n        };',
    '        let ReconcileOutcome::Incremental { patch_count } = outcome else {\n            panic!("dense structural edit must stay incremental: {outcome:?}");\n        };',
)
text = text.replace(
    '            ReconcileOutcome::Rebuilt { patch_count: 1 }\n',
    '            ReconcileOutcome::Incremental { patch_count: 1 }\n',
)
# Add web acceptance regression before no-op reconciliation test.
web_anchor = '''    #[test]
    fn no_op_scene_rerun_remains_incremental_without_mutation() {
'''
web_test = r'''    #[test]
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
        assert_eq!(runtime.affected_objects, 1);
        assert_eq!(runtime.full_group_rebuilds, 0);
        assert_eq!(runtime.full_scheduler_rebuilds, 0);
    }

'''
text = replace_once(text, web_anchor, web_test + web_anchor, "web 100k transaction test")
path.write_text(text)

# Document the completed B2 contract.
path = Path("docs/execution-slots.md")
text = path.read_text()
text += r'''

## Local transactional mutation

Live patch batches now use a three-stage preflight before commit: semantic identity/field
validation, lightweight compiled channel validation, and execution-slot generation validation.
None of these stages clones `SceneDefinition`, `CompiledScene`, `SceneInstance`, frame state, or
geometry payloads. After preflight, patches commit through stable compiled slots and only the
affected `(slot, property)` timeline channels are relowered. `SlottedSceneInstance` aggregates
all per-patch effects into one `ExecutionDelta` for renderer/reactive consumers.

The compatibility frame vectors remain slot-addressed and may contain tombstones. Removing an
object therefore never renumbers unrelated compiled/frame/GPU targets; later creates reuse a free
slot. Direct seek is still intentionally allowed to revisit the timeline, while forward playback
and live mutation stay proportional to active/crossed or explicitly affected channels.
'''
path.write_text(text)
