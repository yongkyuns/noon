from pathlib import Path


def replace_once(text: str, old: str, new: str, label: str) -> str:
    if old not in text:
        raise RuntimeError(f"missing replacement anchor: {label}")
    return text.replace(old, new, 1)


path = Path("crates/noon-compile/src/lib.rs")
text = path.read_text()
text = replace_once(
    text,
    '''pub struct CompiledTrack {
    pub id: TrackId,
    pub object_index: u32,
    pub property: Property,
    pub values: TrackValues,
    pub timing: TrackTiming,
    pub time_map: CompositionTimeMap,
    pub transform_geometry_plan: Option<TransformGeometryPlan>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CompiledScene {
''',
    '''pub struct CompiledTrack {
    pub id: TrackId,
    pub object_index: u32,
    pub property: Property,
    pub values: TrackValues,
    pub timing: TrackTiming,
    pub time_map: CompositionTimeMap,
    pub transform_geometry_plan: Option<TransformGeometryPlan>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct CompiledTrackLocator {
    object_index: u32,
    property: Property,
    start_time: f64,
    id: TrackId,
}

impl CompiledTrackLocator {
    fn from_track(track: &CompiledTrack) -> Self {
        Self {
            object_index: track.object_index,
            property: track.property,
            start_time: track.timing.start_time,
            id: track.id,
        }
    }
}

/// Instrumentation for one compiled-scene patch. Stable object slots remove
/// identity churn while dense track movement remains explicitly observable.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CompiledPatchStats {
    pub track_vector_clones: usize,
    pub presence_tracks_inspected: usize,
    pub dynamic_objects_recomputed: usize,
    pub dynamic_tracks_inspected: usize,
    pub dense_track_slots_shifted: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CompiledScene {
''',
    "track locator and patch stats",
)
text = replace_once(
    text,
    '''    object_indices: BTreeMap<ObjectId, u32>,
    free_object_indices: Vec<u32>,
}
''',
    '''    object_indices: BTreeMap<ObjectId, u32>,
    free_object_indices: Vec<u32>,
    track_locators: BTreeMap<TrackId, CompiledTrackLocator>,
}
''',
    "compiled locator field",
)
text = replace_once(
    text,
    '''        validate_presence_chains(&tracks)
            .map_err(|(previous, next)| CompileError::DiscontinuousPresence { previous, next })?;

        Ok(Self {
            objects,
            tracks,
            object_indices,
            free_object_indices: Vec::new(),
        })
''',
    '''        validate_presence_chains(&tracks)
            .map_err(|(previous, next)| CompileError::DiscontinuousPresence { previous, next })?;
        let track_locators = tracks
            .iter()
            .map(|track| (track.id, CompiledTrackLocator::from_track(track)))
            .collect();

        Ok(Self {
            objects,
            tracks,
            object_indices,
            free_object_indices: Vec::new(),
            track_locators,
        })
''',
    "compile locator initialization",
)
start = text.index("    pub fn apply_patch(&mut self, patch: &ScenePatch) -> Result<(), CompilePatchError> {")
end = text.index("    fn compile_patch_track(", start)
combined = r'''    pub fn apply_patch(&mut self, patch: &ScenePatch) -> Result<(), CompilePatchError> {
        self.apply_patch_with_stats(patch).map(|_| ())
    }

    pub fn apply_patch_with_stats(
        &mut self,
        patch: &ScenePatch,
    ) -> Result<CompiledPatchStats, CompilePatchError> {
        let mut stats = CompiledPatchStats::default();
        match patch {
            ScenePatch::CreateObject(object) => {
                if self.object_indices.contains_key(&object.id) {
                    return Err(CompilePatchError::DuplicateObject(object.id));
                }
                let compiled = CompiledObject {
                    id: object.id,
                    geometry: object.geometry.clone(),
                    base_transform: object.transform,
                    base_style: object.style,
                    dynamic: DynamicProperties::default(),
                    live: true,
                };
                let index = if let Some(index) = self.free_object_indices.pop() {
                    self.objects[index as usize] = compiled;
                    index
                } else {
                    let index = u32::try_from(self.objects.len())
                        .map_err(|_| CompilePatchError::TooManyObjects(self.objects.len()))?;
                    self.objects.push(compiled);
                    index
                };
                self.object_indices.insert(object.id, index);
            }
            ScenePatch::RemoveObject(id) => {
                let index = self
                    .object_indices
                    .remove(id)
                    .ok_or(CompilePatchError::UnknownObject(*id))?;
                let object = &mut self.objects[index as usize];
                debug_assert!(object.live);
                object.live = false;
                object.dynamic = DynamicProperties::default();

                let range = self.object_track_range(index);
                let before_len = self.tracks.len();
                let removed_ids = self.tracks[range.clone()]
                    .iter()
                    .map(|track| track.id)
                    .collect::<Vec<_>>();
                stats.dense_track_slots_shifted = before_len.saturating_sub(range.end);
                self.tracks.drain(range);
                for id in removed_ids {
                    self.track_locators.remove(&id);
                }
                self.free_object_indices.push(index);
            }
            ScenePatch::SetTransform { object, transform } => {
                let index = self
                    .object_index(*object)
                    .ok_or(CompilePatchError::UnknownObject(*object))?;
                self.objects[index as usize].base_transform = *transform;
            }
            ScenePatch::SetStyle { object, style } => {
                let index = self
                    .object_index(*object)
                    .ok_or(CompilePatchError::UnknownObject(*object))?;
                self.objects[index as usize].base_style = *style;
            }
            ScenePatch::AddTrack(track) => {
                if self.track_locators.contains_key(&track.id) {
                    return Err(CompilePatchError::DuplicateTrack(track.id));
                }
                let compiled = self.compile_patch_track(track)?;
                stats.presence_tracks_inspected +=
                    self.validate_presence_edit(None, Some(&compiled))?;
                let locator = CompiledTrackLocator::from_track(&compiled);
                let position = self.track_insertion_position(&compiled);
                stats.dense_track_slots_shifted = self.tracks.len().saturating_sub(position);
                self.tracks.insert(position, compiled);
                self.track_locators.insert(track.id, locator);
                self.objects[locator.object_index as usize]
                    .dynamic
                    .mark(locator.property);
            }
            ScenePatch::ReplaceTrack(track) => {
                let old_locator = self
                    .track_locators
                    .get(&track.id)
                    .copied()
                    .ok_or(CompilePatchError::UnknownTrack(track.id))?;
                let old_position = self.track_position(old_locator);
                let compiled = self.compile_patch_track(track)?;
                stats.presence_tracks_inspected +=
                    self.validate_presence_edit(Some(track.id), Some(&compiled))?;

                let before_remove_len = self.tracks.len();
                self.tracks.remove(old_position);
                stats.dense_track_slots_shifted +=
                    before_remove_len.saturating_sub(old_position + 1);
                let new_locator = CompiledTrackLocator::from_track(&compiled);
                let new_position = self.track_insertion_position(&compiled);
                stats.dense_track_slots_shifted += self.tracks.len().saturating_sub(new_position);
                self.tracks.insert(new_position, compiled);
                self.track_locators.insert(track.id, new_locator);
                self.recompute_dynamic_for_objects(
                    &[old_locator.object_index, new_locator.object_index],
                    &mut stats,
                );
            }
            ScenePatch::RemoveTrack(id) => {
                let old_locator = self
                    .track_locators
                    .get(id)
                    .copied()
                    .ok_or(CompilePatchError::UnknownTrack(*id))?;
                stats.presence_tracks_inspected += self.validate_presence_edit(Some(*id), None)?;
                let position = self.track_position(old_locator);
                let before_remove_len = self.tracks.len();
                self.tracks.remove(position);
                stats.dense_track_slots_shifted = before_remove_len.saturating_sub(position + 1);
                self.track_locators.remove(id);
                self.recompute_dynamic_for_objects(&[old_locator.object_index], &mut stats);
            }
        }
        Ok(stats)
    }

'''
text = text[:start] + combined + text[end:]
# Replace the narrow dynamic helper with the converged locator/channel helpers.
old = '''    fn refresh_dynamic_property(&mut self, object_index: u32, property: Property) {
        let dynamic = !self.track_group(object_index, property).is_empty();
        self.objects[object_index as usize]
            .dynamic
            .set(property, dynamic);
    }
'''
new = r'''    fn track_insertion_position(&self, track: &CompiledTrack) -> usize {
        self.tracks
            .binary_search_by(|existing| compare_tracks(existing, track))
            .unwrap_or_else(|position| position)
    }

    fn track_position(&self, locator: CompiledTrackLocator) -> usize {
        self.tracks
            .binary_search_by(|track| compare_track_locator(track, locator))
            .expect("track locator index must match sorted track storage")
    }

    fn object_track_range(&self, object_index: u32) -> std::ops::Range<usize> {
        let start = self
            .tracks
            .partition_point(|track| track.object_index < object_index);
        let end = self
            .tracks
            .partition_point(|track| track.object_index <= object_index);
        start..end
    }

    fn validate_presence_edit(
        &self,
        excluded: Option<TrackId>,
        candidate: Option<&CompiledTrack>,
    ) -> Result<usize, CompilePatchError> {
        let mut affected_objects = Vec::with_capacity(2);
        if let Some(id) = excluded {
            let locator = self
                .track_locators
                .get(&id)
                .copied()
                .expect("excluded track was resolved before presence validation");
            if locator.property == Property::Presence {
                affected_objects.push(locator.object_index);
            }
        }
        if let Some(track) = candidate {
            if track.property == Property::Presence
                && !affected_objects.contains(&track.object_index)
            {
                affected_objects.push(track.object_index);
            }
        }

        let mut inspected = 0;
        for object_index in affected_objects {
            let range = self.object_track_range(object_index);
            let mut events = Vec::new();
            for track in &self.tracks[range] {
                if track.property != Property::Presence {
                    break;
                }
                if excluded == Some(track.id) {
                    continue;
                }
                let TrackValues::Bool { from, to } = track.values else {
                    unreachable!("validated Presence track must contain bool values");
                };
                events.push((track.timing.start_time, track.id, from, to));
                inspected += 1;
            }
            if let Some(track) = candidate.filter(|track| {
                track.object_index == object_index && track.property == Property::Presence
            }) {
                let TrackValues::Bool { from, to } = track.values else {
                    unreachable!("validated Presence track must contain bool values");
                };
                events.push((track.timing.start_time, track.id, from, to));
                inspected += 1;
            }
            events.sort_by(|left, right| {
                left.0
                    .total_cmp(&right.0)
                    .then_with(|| left.1.cmp(&right.1))
            });
            for pair in events.windows(2) {
                if pair[0].3 != pair[1].2 {
                    return Err(CompilePatchError::DiscontinuousPresence {
                        previous: pair[0].1,
                        next: pair[1].1,
                    });
                }
            }
        }
        Ok(inspected)
    }

    fn recompute_dynamic_for_objects(
        &mut self,
        object_indices: &[u32],
        stats: &mut CompiledPatchStats,
    ) {
        let mut unique = Vec::with_capacity(object_indices.len());
        for object_index in object_indices.iter().copied() {
            if !unique.contains(&object_index) {
                unique.push(object_index);
            }
        }
        for object_index in unique {
            let range = self.object_track_range(object_index);
            let mut dynamic = DynamicProperties::default();
            for track in &self.tracks[range] {
                dynamic.mark(track.property);
                stats.dynamic_tracks_inspected += 1;
            }
            self.objects[object_index as usize].dynamic = dynamic;
            stats.dynamic_objects_recomputed += 1;
        }
    }
'''
text = replace_once(text, old, new, "compiler locator helpers")
# Keep the shadow endpoint helper but remove the now-obsolete direct presence/insert helpers.
start = text.index("fn insert_track_sorted(")
end = text.index("fn presence_values(", start)
text = text[:start] + text[end:]
start = text.index("fn validate_presence_patch(")
end = text.index("fn validate_shadow_presence_channel(", start)
text = text[:start] + text[end:]
text = replace_once(
    text,
    '''fn sort_tracks(tracks: &mut [CompiledTrack]) {
    tracks.sort_by(compare_tracks);
}
''',
    '''fn compare_track_locator(
    track: &CompiledTrack,
    locator: CompiledTrackLocator,
) -> std::cmp::Ordering {
    track
        .object_index
        .cmp(&locator.object_index)
        .then_with(|| property_rank(track.property).cmp(&property_rank(locator.property)))
        .then_with(|| track.timing.start_time.total_cmp(&locator.start_time))
        .then_with(|| track.id.cmp(&locator.id))
}

fn sort_tracks(tracks: &mut [CompiledTrack]) {
    tracks.sort_by(compare_tracks);
}
''',
    "track locator comparator",
)
path.write_text(text)
