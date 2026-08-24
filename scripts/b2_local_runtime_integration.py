from pathlib import Path


def replace_once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected exactly one match, found {count}")
    return text.replace(old, new, 1)


# Compiler: expose a binary-searched channel slice. The compatibility track Vec
# remains sorted, but runtime groups no longer retain fragile start/end offsets.
compile_path = Path("crates/noon-compile/src/lib.rs")
text = compile_path.read_text()
text = replace_once(
    text,
    '''    pub fn object_index(&self, id: ObjectId) -> Option<u32> {
        self.object_indices.get(&id).copied()
    }

    pub fn apply_patch(&mut self, patch: &ScenePatch) -> Result<(), CompilePatchError> {
''',
    '''    pub fn object_index(&self, id: ObjectId) -> Option<u32> {
        self.object_indices.get(&id).copied()
    }

    /// Returns the sorted tracks for one stable execution slot/property channel.
    ///
    /// Runtime groups deliberately resolve this range on demand so insertion or
    /// removal in an unrelated channel never invalidates retained group metadata.
    pub fn track_group(&self, object_index: u32, property: Property) -> &[CompiledTrack] {
        let rank = property_rank(property);
        let start = self.tracks.partition_point(|track| {
            track.object_index < object_index
                || (track.object_index == object_index && property_rank(track.property) < rank)
        });
        let count = self.tracks[start..].partition_point(|track| {
            track.object_index == object_index && property_rank(track.property) == rank
        });
        &self.tracks[start..start + count]
    }

    pub fn apply_patch(&mut self, patch: &ScenePatch) -> Result<(), CompilePatchError> {
''',
    "compiled channel accessor",
)
compile_path.write_text(text)


scheduler_path = Path("crates/noon-runtime/src/reactive/timeline_scheduler.rs")
scheduler_path.write_text(r'''use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::ops::Bound::{Excluded, Included, Unbounded};

use noon_compile::CompiledTrack;
use noon_core::Property;

use crate::SceneInstance;

pub type TrackGroupKey = u64;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TimelineSchedulerStats {
    pub events_crossed: usize,
    pub active_groups: usize,
    pub groups_requested: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TimelineSchedulerPatchStats {
    pub groups_rebuilt: usize,
    pub events_removed: usize,
    pub events_added: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EventKind {
    End,
    Start,
    Instant,
}

#[derive(Clone, Copy, Debug)]
struct TimelineEvent {
    group: TrackGroupKey,
    kind: EventKind,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct EventTime(f64);

impl Eq for EventTime {}

impl PartialOrd for EventTime {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for EventTime {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.total_cmp(&other.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScheduledTrackGroup {
    pub object_index: usize,
    pub property: Property,
}

/// Stable channel identity derived from the stable compiled object slot and
/// property. It is independent of compact group/vector positions.
pub const fn track_group_key(object_index: u32, property: Property) -> TrackGroupKey {
    ((object_index as u64) << 3) | property_slot(property) as u64
}

/// Event index for monotonic timeline playback.
///
/// Events are retained in a time-ordered tree and refer to stable channel keys.
/// Replacing one channel removes/inserts only that channel's boundary events;
/// unrelated groups and event identities are untouched.
#[derive(Clone, Debug)]
pub struct TimelineEventScheduler {
    groups: BTreeMap<TrackGroupKey, ScheduledTrackGroup>,
    events: BTreeMap<EventTime, Vec<TimelineEvent>>,
    group_event_times: BTreeMap<TrackGroupKey, Vec<EventTime>>,
    time: f64,
    active_counts: BTreeMap<TrackGroupKey, u32>,
    active_groups: Vec<TrackGroupKey>,
    active_positions: BTreeMap<TrackGroupKey, usize>,
    visit_epoch: BTreeMap<TrackGroupKey, u64>,
    epoch: u64,
    requested: Vec<TrackGroupKey>,
    last_stats: TimelineSchedulerStats,
    last_patch_stats: TimelineSchedulerPatchStats,
}

impl TimelineEventScheduler {
    pub fn new(tracks: &[CompiledTrack]) -> Self {
        let mut scheduler = Self {
            groups: BTreeMap::new(),
            events: BTreeMap::new(),
            group_event_times: BTreeMap::new(),
            time: f64::NEG_INFINITY,
            active_counts: BTreeMap::new(),
            active_groups: Vec::new(),
            active_positions: BTreeMap::new(),
            visit_epoch: BTreeMap::new(),
            epoch: 0,
            requested: Vec::new(),
            last_stats: TimelineSchedulerStats::default(),
            last_patch_stats: TimelineSchedulerPatchStats::default(),
        };
        for track in tracks {
            let key = track_group_key(track.object_index, track.property);
            scheduler.groups.entry(key).or_insert(ScheduledTrackGroup {
                object_index: track.object_index as usize,
                property: track.property,
            });
            scheduler.insert_track_events(key, track);
        }
        scheduler.normalize_group_event_times();
        scheduler
    }

    pub fn groups(&self) -> impl Iterator<Item = &ScheduledTrackGroup> {
        self.groups.values()
    }

    pub fn last_stats(&self) -> TimelineSchedulerStats {
        self.last_stats
    }

    pub fn last_patch_stats(&self) -> TimelineSchedulerPatchStats {
        self.last_patch_stats
    }

    pub fn active_groups(&self) -> &[TrackGroupKey] {
        &self.active_groups
    }

    /// Replace exactly one slot/property channel at the current playhead.
    pub fn replace_group(
        &mut self,
        object_index: u32,
        property: Property,
        tracks: &[CompiledTrack],
        current_time: f64,
    ) -> TimelineSchedulerPatchStats {
        let key = track_group_key(object_index, property);
        let events_removed = self.remove_group_events(key);
        self.groups.remove(&key);
        self.active_counts.remove(&key);
        self.deactivate(key);
        self.visit_epoch.remove(&key);

        let mut events_added = 0;
        if !tracks.is_empty() {
            self.groups.insert(
                key,
                ScheduledTrackGroup {
                    object_index: object_index as usize,
                    property,
                },
            );
            for track in tracks {
                events_added += self.insert_track_events(key, track);
            }
            if let Some(times) = self.group_event_times.get_mut(&key) {
                times.sort_unstable();
                times.dedup();
            }
            let active = tracks
                .iter()
                .filter(|track| {
                    track.property != Property::Presence
                        && track.timing.start_time <= current_time
                        && current_time < track.timing.start_time + track.timing.duration
                })
                .count() as u32;
            if active > 0 {
                self.active_counts.insert(key, active);
                self.activate(key);
            }
        }
        self.time = current_time;
        self.requested.clear();
        let stats = TimelineSchedulerPatchStats {
            groups_rebuilt: 1,
            events_removed,
            events_added,
        };
        self.last_patch_stats = stats;
        stats
    }

    /// Remove one slot/property channel without rebuilding unrelated events.
    pub fn remove_group(
        &mut self,
        object_index: u32,
        property: Property,
        current_time: f64,
    ) -> TimelineSchedulerPatchStats {
        self.replace_group(object_index, property, &[], current_time)
    }

    /// Rebuild scheduler state for direct seek. Direct seek is intentionally
    /// allowed to be O(events); the ordinary forward frame path is not.
    pub fn seek(&mut self, time: f64) {
        self.time = f64::NEG_INFINITY;
        self.active_counts.clear();
        self.active_groups.clear();
        self.active_positions.clear();
        let mut crossed = Vec::new();
        for (_, events) in self.events.range((Unbounded, Included(EventTime(time)))) {
            crossed.extend(events.iter().copied());
        }
        for event in crossed.iter().copied() {
            self.apply_event(event);
        }
        self.time = time;
        self.requested.clear();
        self.last_stats = TimelineSchedulerStats {
            events_crossed: crossed.len(),
            active_groups: self.active_groups.len(),
            groups_requested: 0,
        };
    }

    /// Return exactly the stable channels that may change at `time`: channels
    /// active after crossing the interval plus channels touched by boundaries.
    pub fn advance(&mut self, time: f64) -> &[TrackGroupKey] {
        if time < self.time {
            self.seek(time);
            self.begin_request_epoch();
            let active = self.active_groups.clone();
            for group in active {
                self.request(group);
            }
            self.last_stats.groups_requested = self.requested.len();
            return &self.requested;
        }

        self.begin_request_epoch();
        let mut crossed = Vec::new();
        for (_, events) in self
            .events
            .range((Excluded(EventTime(self.time)), Included(EventTime(time))))
        {
            crossed.extend(events.iter().copied());
        }
        for event in crossed.iter().copied() {
            self.request(event.group);
            self.apply_event(event);
        }

        let active = self.active_groups.clone();
        for group in active {
            self.request(group);
        }
        self.time = time;
        self.last_stats = TimelineSchedulerStats {
            events_crossed: crossed.len(),
            active_groups: self.active_groups.len(),
            groups_requested: self.requested.len(),
        };
        &self.requested
    }

    fn insert_track_events(&mut self, group: TrackGroupKey, track: &CompiledTrack) -> usize {
        if track.property == Property::Presence {
            self.insert_event(
                EventTime(track.timing.start_time),
                TimelineEvent {
                    group,
                    kind: EventKind::Instant,
                },
            );
            self.group_event_times
                .entry(group)
                .or_default()
                .push(EventTime(track.timing.start_time));
            return 1;
        }
        let start = EventTime(track.timing.start_time);
        let end = EventTime(track.timing.start_time + track.timing.duration);
        self.insert_event(
            start,
            TimelineEvent {
                group,
                kind: EventKind::Start,
            },
        );
        self.insert_event(
            end,
            TimelineEvent {
                group,
                kind: EventKind::End,
            },
        );
        self.group_event_times
            .entry(group)
            .or_default()
            .extend([start, end]);
        2
    }

    fn insert_event(&mut self, time: EventTime, event: TimelineEvent) {
        let bucket = self.events.entry(time).or_default();
        bucket.push(event);
        bucket.sort_by(|left, right| {
            event_rank(left.kind)
                .cmp(&event_rank(right.kind))
                .then_with(|| left.group.cmp(&right.group))
        });
    }

    fn normalize_group_event_times(&mut self) {
        for times in self.group_event_times.values_mut() {
            times.sort_unstable();
            times.dedup();
        }
    }

    fn remove_group_events(&mut self, group: TrackGroupKey) -> usize {
        let Some(times) = self.group_event_times.remove(&group) else {
            return 0;
        };
        let mut removed = 0;
        let mut empty = Vec::new();
        for time in times {
            if let Some(bucket) = self.events.get_mut(&time) {
                let before = bucket.len();
                bucket.retain(|event| event.group != group);
                removed += before - bucket.len();
                if bucket.is_empty() {
                    empty.push(time);
                }
            }
        }
        for time in empty {
            self.events.remove(&time);
        }
        removed
    }

    fn begin_request_epoch(&mut self) {
        self.requested.clear();
        self.epoch = self.epoch.wrapping_add(1);
        if self.epoch == 0 {
            self.visit_epoch.clear();
            self.epoch = 1;
        }
    }

    fn request(&mut self, group: TrackGroupKey) {
        if self.visit_epoch.get(&group).copied() == Some(self.epoch) {
            return;
        }
        self.visit_epoch.insert(group, self.epoch);
        self.requested.push(group);
    }

    fn apply_event(&mut self, event: TimelineEvent) {
        match event.kind {
            EventKind::Instant => {}
            EventKind::Start => {
                let count = self.active_counts.entry(event.group).or_default();
                *count += 1;
                if *count == 1 {
                    self.activate(event.group);
                }
            }
            EventKind::End => {
                let Some(count) = self.active_counts.get_mut(&event.group) else {
                    return;
                };
                *count = count.saturating_sub(1);
                if *count == 0 {
                    self.active_counts.remove(&event.group);
                    self.deactivate(event.group);
                }
            }
        }
    }

    fn activate(&mut self, group: TrackGroupKey) {
        if self.active_positions.contains_key(&group) {
            return;
        }
        self.active_positions.insert(group, self.active_groups.len());
        self.active_groups.push(group);
    }

    fn deactivate(&mut self, group: TrackGroupKey) {
        let Some(position) = self.active_positions.remove(&group) else {
            return;
        };
        let last = self.active_groups.len() - 1;
        self.active_groups.swap(position, last);
        let removed = self.active_groups.pop().expect("active group exists");
        debug_assert_eq!(removed, group);
        if position < self.active_groups.len() {
            let moved = self.active_groups[position];
            self.active_positions.insert(moved, position);
        }
    }
}

impl SceneInstance {
    pub fn last_timeline_scheduler_stats(&self) -> TimelineSchedulerStats {
        self.timeline_scheduler.last_stats()
    }

    pub fn last_timeline_scheduler_patch_stats(&self) -> TimelineSchedulerPatchStats {
        self.timeline_scheduler.last_patch_stats()
    }
}

const fn event_rank(kind: EventKind) -> u8 {
    match kind {
        EventKind::End => 0,
        EventKind::Instant => 1,
        EventKind::Start => 2,
    }
}

const fn property_slot(property: Property) -> u8 {
    match property {
        Property::Presence => 0,
        Property::Transform => 1,
        Property::Position => 2,
        Property::Rotation => 3,
        Property::Opacity => 4,
        Property::Appearance => 5,
        Property::Reveal => 6,
        Property::Morph => 7,
    }
}

#[cfg(test)]
mod tests {
    use noon_compile::CompiledTrack;
    use noon_core::{
        CompositionTimeMap, Property, RateFunction, TrackId, TrackTiming, TrackValues, Vec2,
    };

    use super::*;

    fn position_track(id: u64, object: u32, start: f64, duration: f64) -> CompiledTrack {
        CompiledTrack {
            id: TrackId::new(id),
            object_index: object,
            property: Property::Position,
            values: TrackValues::Vec2 {
                from: Vec2::ZERO,
                to: Vec2::new(1.0, 0.0),
            },
            timing: TrackTiming {
                start_time: start,
                duration,
                easing: RateFunction::Linear,
            },
            time_map: CompositionTimeMap::default(),
            transform_geometry_plan: None,
        }
    }

    #[test]
    fn sparse_long_timeline_requests_only_active_or_crossed_groups() {
        let mut tracks = Vec::new();
        for index in 0..100_000u32 {
            tracks.push(position_track(
                index as u64,
                index,
                1_000.0 + index as f64 * 2.0,
                1.0,
            ));
        }
        let mut scheduler = TimelineEventScheduler::new(&tracks);
        scheduler.seek(0.0);
        assert!(scheduler.advance(1.0 / 60.0).is_empty());
        let stats = scheduler.last_stats();
        assert_eq!(stats.events_crossed, 0);
        assert_eq!(stats.active_groups, 0);
        assert_eq!(stats.groups_requested, 0);
    }

    #[test]
    fn active_cost_depends_on_active_groups_not_total_groups() {
        let mut tracks = Vec::new();
        for index in 0..100_000u32 {
            let start = if index < 10 {
                0.0
            } else {
                10_000.0 + index as f64
            };
            tracks.push(position_track(index as u64, index, start, 10.0));
        }
        let mut scheduler = TimelineEventScheduler::new(&tracks);
        scheduler.seek(0.0);
        let requested = scheduler.advance(0.5);
        assert_eq!(requested.len(), 10);
        let stats = scheduler.last_stats();
        assert_eq!(stats.active_groups, 10);
        assert_eq!(stats.groups_requested, 10);
        assert_eq!(stats.events_crossed, 0);
    }

    #[test]
    fn jumping_over_completed_segments_requests_endpoint_once() {
        let tracks = vec![
            position_track(1, 0, 1.0, 1.0),
            position_track(2, 1, 100.0, 1.0),
        ];
        let mut scheduler = TimelineEventScheduler::new(&tracks);
        scheduler.seek(0.0);
        assert_eq!(
            scheduler.advance(3.0),
            &[track_group_key(0, Property::Position)]
        );
        let stats = scheduler.last_stats();
        assert_eq!(stats.events_crossed, 2);
        assert_eq!(stats.active_groups, 0);
    }

    #[test]
    fn adjacent_segments_keep_group_active_at_handoff() {
        let tracks = vec![
            position_track(1, 0, 0.0, 1.0),
            position_track(2, 0, 1.0, 1.0),
        ];
        let mut scheduler = TimelineEventScheduler::new(&tracks);
        scheduler.seek(0.5);
        let key = track_group_key(0, Property::Position);
        assert_eq!(scheduler.advance(1.0), &[key]);
        assert_eq!(scheduler.active_groups(), &[key]);
    }

    #[test]
    fn replacing_one_group_touches_only_its_events() {
        let mut tracks = Vec::new();
        for index in 0..100_000u32 {
            tracks.push(position_track(index as u64, index, 1000.0 + index as f64, 1.0));
        }
        let mut scheduler = TimelineEventScheduler::new(&tracks);
        scheduler.seek(0.0);
        let replacement = vec![
            position_track(200_000, 50_000, 2.0, 1.0),
            position_track(200_001, 50_000, 4.0, 1.0),
        ];
        let stats = scheduler.replace_group(50_000, Property::Position, &replacement, 0.0);
        assert_eq!(stats.groups_rebuilt, 1);
        assert_eq!(stats.events_removed, 2);
        assert_eq!(stats.events_added, 4);
        assert_eq!(scheduler.groups().count(), 100_000);
    }
}
''')


runtime_path = Path("crates/noon-runtime/src/lib.rs")
text = runtime_path.read_text()
text = replace_once(
    text,
    "pub use reactive::*;\n\nuse noon_compile",
    "pub use reactive::*;\n\nuse std::collections::BTreeMap;\n\nuse noon_compile",
    "runtime BTreeMap import",
)
text = replace_once(
    text,
    '''#[derive(Clone, Debug)]
struct TrackGroup {
    object_index: usize,
    property: Property,
    start: usize,
    end: usize,
    cursor: usize,
    mapped: bool,
}

#[derive(Clone, Debug)]
pub struct SceneInstance {
''',
    '''#[derive(Clone, Debug)]
struct TrackGroup {
    object_index: usize,
    property: Property,
    cursor: usize,
    mapped: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RuntimePatchStats {
    pub affected_objects: usize,
    pub groups_rebuilt: usize,
    pub scheduler_groups_rebuilt: usize,
    pub full_group_rebuilds: usize,
    pub full_scheduler_rebuilds: usize,
}

#[derive(Clone, Debug)]
pub struct SceneInstance {
''',
    "runtime group definition",
)
text = replace_once(
    text,
    '''    groups: Vec<TrackGroup>,
    timeline_scheduler: TimelineEventScheduler,
    last_stats: EvaluationStats,
''',
    '''    groups: BTreeMap<TrackGroupKey, TrackGroup>,
    timeline_scheduler: TimelineEventScheduler,
    last_stats: EvaluationStats,
    last_patch_stats: RuntimePatchStats,
''',
    "runtime group storage",
)
text = replace_once(
    text,
    '''            timeline_scheduler,
            last_stats: EvaluationStats::default(),
            changes: FrameChanges::all(),
''',
    '''            timeline_scheduler,
            last_stats: EvaluationStats::default(),
            last_patch_stats: RuntimePatchStats::default(),
            changes: FrameChanges::all(),
''',
    "runtime patch stats init",
)
text = replace_once(
    text,
    '''    pub const fn last_stats(&self) -> EvaluationStats {
        self.last_stats
    }

    pub fn take_frame_changes(&mut self) -> FrameChanges {
''',
    '''    pub const fn last_stats(&self) -> EvaluationStats {
        self.last_stats
    }

    pub const fn last_patch_stats(&self) -> RuntimePatchStats {
        self.last_patch_stats
    }

    pub fn take_frame_changes(&mut self) -> FrameChanges {
''',
    "runtime patch stats accessor",
)

old_apply_patch = '''    pub fn apply_patch(&mut self, patch: &ScenePatch) -> Result<&FrameState, CompilePatchError> {
        if matches!(
            patch,
            ScenePatch::SetTransform { .. } | ScenePatch::SetStyle { .. }
        ) {
            self.apply_value_patch(patch)?;
            return Ok(&self.frame);
        }
        let current_time = self.frame.time;
        self.compiled.apply_patch(patch)?;
        self.groups = build_groups(self.compiled.tracks());
        self.timeline_scheduler = TimelineEventScheduler::new(self.compiled.tracks());
        self.seek_unchecked(current_time);
        Ok(&self.frame)
    }
'''
new_apply_patch = '''    pub fn apply_patch(&mut self, patch: &ScenePatch) -> Result<&FrameState, CompilePatchError> {
        if matches!(
            patch,
            ScenePatch::SetTransform { .. } | ScenePatch::SetStyle { .. }
        ) {
            self.apply_value_patch(patch)?;
            self.last_patch_stats = RuntimePatchStats {
                affected_objects: 1,
                ..RuntimePatchStats::default()
            };
            return Ok(&self.frame);
        }

        let current_time = self.frame.time;
        let mut affected_channels = Vec::new();
        let mut affected_objects = Vec::new();
        let removed_object = match patch {
            ScenePatch::RemoveObject(object) => self.compiled.object_index(*object).map(|index| index as usize),
            _ => None,
        };
        match patch {
            ScenePatch::AddTrack(track) => {
                if let Some(index) = self.compiled.object_index(track.object) {
                    push_channel(&mut affected_channels, index as usize, track.property);
                    push_object(&mut affected_objects, index as usize);
                }
            }
            ScenePatch::ReplaceTrack(track) => {
                if let Some(old) = self.compiled.tracks().iter().find(|old| old.id == track.id) {
                    push_channel(&mut affected_channels, old.object_index as usize, old.property);
                    push_object(&mut affected_objects, old.object_index as usize);
                }
                if let Some(index) = self.compiled.object_index(track.object) {
                    push_channel(&mut affected_channels, index as usize, track.property);
                    push_object(&mut affected_objects, index as usize);
                }
            }
            ScenePatch::RemoveTrack(id) => {
                if let Some(old) = self.compiled.tracks().iter().find(|track| track.id == *id) {
                    push_channel(&mut affected_channels, old.object_index as usize, old.property);
                    push_object(&mut affected_objects, old.object_index as usize);
                }
            }
            _ => {}
        }

        self.compiled.apply_patch(patch)?;
        let mut groups_rebuilt = 0;
        let mut scheduler_groups_rebuilt = 0;

        match patch {
            ScenePatch::CreateObject(object) => {
                let index = self
                    .compiled
                    .object_index(object.id)
                    .expect("compiled create succeeded") as usize;
                self.sync_frame_slot_from_compiled(index);
                self.changes.insert(index);
                push_object(&mut affected_objects, index);
            }
            ScenePatch::RemoveObject(_) => {
                if let Some(index) = removed_object {
                    for property in ALL_PROPERTIES {
                        let key = track_group_key(index as u32, property);
                        if self.groups.remove(&key).is_some() {
                            groups_rebuilt += 1;
                        }
                        let stats = self
                            .timeline_scheduler
                            .remove_group(index as u32, property, current_time);
                        scheduler_groups_rebuilt += stats.groups_rebuilt;
                    }
                    self.frame.objects[index].live = false;
                    self.frame.presences[index] = false;
                    self.frame.render_geometries[index] = None;
                    self.changes.insert(index);
                    push_object(&mut affected_objects, index);
                }
            }
            ScenePatch::AddTrack(_) | ScenePatch::ReplaceTrack(_) | ScenePatch::RemoveTrack(_) => {
                for (object_index, property) in affected_channels.iter().copied() {
                    self.refresh_group_structure(object_index, property, current_time);
                    groups_rebuilt += 1;
                    scheduler_groups_rebuilt += 1;
                }
                for object_index in affected_objects.iter().copied() {
                    self.reapply_object_timeline(object_index);
                }
            }
            ScenePatch::SetTransform { .. } | ScenePatch::SetStyle { .. } => unreachable!(),
        }

        self.last_patch_stats = RuntimePatchStats {
            affected_objects: affected_objects.len(),
            groups_rebuilt,
            scheduler_groups_rebuilt,
            full_group_rebuilds: 0,
            full_scheduler_rebuilds: 0,
        };
        Ok(&self.frame)
    }
'''
text = replace_once(text, old_apply_patch, new_apply_patch, "runtime local patch path")

old_reapply = '''    fn reapply_properties(&mut self, object_index: usize, properties: &[Property]) {
        let time = self.frame.time;
        let tracks = self.compiled.tracks();
        let mut stats = EvaluationStats::default();
        for group in &mut self.groups {
            if group.object_index == object_index && properties.contains(&group.property) {
                let slice = &tracks[group.start..group.end];
                group.cursor = upper_bound_start(slice, time, &mut stats.binary_search_steps);
                apply_group(&mut self.frame, slice, group, time);
                stats.groups_evaluated += 1;
            }
        }
        self.last_stats = stats;
    }
'''
new_reapply = '''    fn reapply_properties(&mut self, object_index: usize, properties: &[Property]) {
        let time = self.frame.time;
        let mut stats = EvaluationStats::default();
        for property in properties {
            let key = track_group_key(object_index as u32, *property);
            let Some(group) = self.groups.get_mut(&key) else {
                continue;
            };
            let slice = self.compiled.track_group(object_index as u32, *property);
            group.cursor = upper_bound_start(slice, time, &mut stats.binary_search_steps);
            apply_group(&mut self.frame, slice, group, time);
            stats.groups_evaluated += 1;
        }
        self.last_stats = stats;
    }

    fn refresh_group_structure(&mut self, object_index: usize, property: Property, time: f64) {
        let key = track_group_key(object_index as u32, property);
        let slice = self.compiled.track_group(object_index as u32, property);
        if slice.is_empty() {
            self.groups.remove(&key);
        } else {
            let mapped = slice.iter().any(|track| !track.time_map.is_identity());
            let mut binary_search_steps = 0;
            let cursor = upper_bound_start(slice, time, &mut binary_search_steps);
            self.groups.insert(
                key,
                TrackGroup {
                    object_index,
                    property,
                    cursor,
                    mapped,
                },
            );
        }
        self.timeline_scheduler
            .replace_group(object_index as u32, property, slice, time);
    }

    fn sync_frame_slot_from_compiled(&mut self, object_index: usize) {
        let object = &self.compiled.objects()[object_index];
        let state = FrameObjectState {
            live: object.live,
            id: object.id,
            geometry: object.geometry.clone(),
            transform: object.base_transform,
            style: object.base_style,
            appearance: 1.0,
        };
        if object_index == self.frame.objects.len() {
            self.frame.objects.push(state);
            self.frame.presences.push(true);
            self.frame.reveals.push(1.0);
            self.frame.morphs.push(0.0);
            self.frame.render_geometries.push(None);
        } else {
            self.frame.objects[object_index] = state;
            self.frame.presences[object_index] = true;
            self.frame.reveals[object_index] = 1.0;
            self.frame.morphs[object_index] = 0.0;
            self.frame.render_geometries[object_index] = None;
        }
    }

    fn reapply_object_timeline(&mut self, object_index: usize) {
        self.sync_frame_slot_from_compiled(object_index);
        let time = self.frame.time;
        let mut stats = EvaluationStats::default();
        for property in ALL_PROPERTIES {
            let key = track_group_key(object_index as u32, property);
            let Some(group) = self.groups.get_mut(&key) else {
                continue;
            };
            let slice = self.compiled.track_group(object_index as u32, property);
            if let Some(first) = slice.first() {
                match (property, &first.values) {
                    (Property::Presence, TrackValues::Bool { from, .. }) => {
                        self.frame.presences[object_index] = *from;
                    }
                    (Property::Appearance, TrackValues::Scalar { from, .. }) => {
                        self.frame.objects[object_index].appearance = from.clamp(0.0, 1.0);
                    }
                    (Property::Reveal, TrackValues::Scalar { from, .. }) => {
                        self.frame.reveals[object_index] = from.clamp(0.0, 1.0);
                    }
                    (Property::Morph, TrackValues::Scalar { from, .. }) => {
                        self.frame.morphs[object_index] = from.clamp(0.0, 1.0);
                    }
                    _ => {}
                }
            }
            group.cursor = upper_bound_start(slice, time, &mut stats.binary_search_steps);
            apply_group(&mut self.frame, slice, group, time);
            stats.groups_evaluated += 1;
        }
        self.reapply_reactive_for_object(object_index);
        self.changes.insert(object_index);
        self.last_stats = stats;
    }
'''
text = replace_once(text, old_reapply, new_reapply, "runtime channel refresh helpers")

old_seek = '''    fn seek_unchecked(&mut self, time: f64) {
        self.frame = base_frame(&self.compiled, time);
        self.changes.invalidate_all();
        let tracks = self.compiled.tracks();
        let mut stats = EvaluationStats::default();

        for group in &mut self.groups {
            let slice = &tracks[group.start..group.end];
            group.cursor = upper_bound_start(slice, time, &mut stats.binary_search_steps);
            apply_group(&mut self.frame, slice, group, time);
            stats.groups_evaluated += 1;
        }
        self.timeline_scheduler.seek(time);

        self.reapply_reactive();
        self.last_stats = stats;
    }
'''
new_seek = '''    fn seek_unchecked(&mut self, time: f64) {
        self.frame = base_frame(&self.compiled, time);
        self.changes.invalidate_all();
        let mut stats = EvaluationStats::default();

        for group in self.groups.values_mut() {
            let slice = self
                .compiled
                .track_group(group.object_index as u32, group.property);
            group.cursor = upper_bound_start(slice, time, &mut stats.binary_search_steps);
            apply_group(&mut self.frame, slice, group, time);
            stats.groups_evaluated += 1;
        }
        self.timeline_scheduler.seek(time);

        self.reapply_reactive();
        self.last_stats = stats;
    }
'''
text = replace_once(text, old_seek, new_seek, "runtime direct seek groups")

old_advance = '''    fn advance_unchecked(&mut self, time: f64) {
        self.frame.time = time;
        let requested = self.timeline_scheduler.advance(time).to_vec();
        let tracks = self.compiled.tracks();
        let mut stats = EvaluationStats::default();
        let changes = &mut self.changes;

        for group_index in requested {
            let group = &mut self.groups[group_index];
            let slice = &tracks[group.start..group.end];
            while group.cursor < slice.len() && slice[group.cursor].timing.start_time <= time {
                group.cursor += 1;
                stats.tracks_advanced += 1;
            }
            if apply_group(&mut self.frame, slice, group, time) {
                changes.insert(group.object_index);
            }
            stats.groups_evaluated += 1;
        }

        self.last_stats = stats;
    }
'''
new_advance = '''    fn advance_unchecked(&mut self, time: f64) {
        self.frame.time = time;
        let requested = self.timeline_scheduler.advance(time).to_vec();
        let mut stats = EvaluationStats::default();
        let changes = &mut self.changes;

        for group_key in requested {
            let Some(group) = self.groups.get_mut(&group_key) else {
                continue;
            };
            let slice = self
                .compiled
                .track_group(group.object_index as u32, group.property);
            while group.cursor < slice.len() && slice[group.cursor].timing.start_time <= time {
                group.cursor += 1;
                stats.tracks_advanced += 1;
            }
            if apply_group(&mut self.frame, slice, group, time) {
                changes.insert(group.object_index);
            }
            stats.groups_evaluated += 1;
        }

        self.last_stats = stats;
    }
'''
text = replace_once(text, old_advance, new_advance, "runtime forward group lookup")

start = text.index("fn build_groups(tracks: &[CompiledTrack]) -> Vec<TrackGroup> {")
end = text.index("\nfn upper_bound_start", start)
text = text[:start] + '''fn build_groups(tracks: &[CompiledTrack]) -> BTreeMap<TrackGroupKey, TrackGroup> {
    let mut groups = BTreeMap::new();
    let mut start = 0;

    while start < tracks.len() {
        let object_index = tracks[start].object_index as usize;
        let property = tracks[start].property;
        let mut end = start + 1;
        while end < tracks.len()
            && tracks[end].object_index as usize == object_index
            && tracks[end].property == property
        {
            end += 1;
        }
        let mapped = tracks[start..end]
            .iter()
            .any(|track| !track.time_map.is_identity());
        groups.insert(
            track_group_key(object_index as u32, property),
            TrackGroup {
                object_index,
                property,
                cursor: 0,
                mapped,
            },
        );
        start = end;
    }

    groups
}

const ALL_PROPERTIES: [Property; 8] = [
    Property::Presence,
    Property::Transform,
    Property::Position,
    Property::Rotation,
    Property::Opacity,
    Property::Appearance,
    Property::Reveal,
    Property::Morph,
];

fn push_channel(channels: &mut Vec<(usize, Property)>, object_index: usize, property: Property) {
    if !channels.contains(&(object_index, property)) {
        channels.push((object_index, property));
    }
}

fn push_object(objects: &mut Vec<usize>, object_index: usize) {
    if !objects.contains(&object_index) {
        objects.push(object_index);
    }
}
''' + text[end:]
runtime_path.write_text(text)

# Add a focused stress regression to runtime tests. It deliberately edits one
# channel in a 100k-channel scene and inspects relowering counters.
test_path = Path("crates/noon-runtime/tests/local_timeline_patch.rs")
test_path.write_text(r'''use noon_compile::CompiledScene;
use noon_core::{
    Easing, GeometryRef, Property, SceneDefinition, ScenePatch, TrackDefinition, TrackId,
    TrackTiming, TrackValues, Vec2,
};
use noon_runtime::SceneInstance;

#[test]
fn one_channel_edit_does_not_rebuild_hundred_thousand_runtime_groups() {
    let mut scene = SceneDefinition::new();
    let mut objects = Vec::with_capacity(100_000);
    for index in 0..100_000u64 {
        let object = scene.add(GeometryRef::circle(1.0));
        objects.push(object);
        scene
            .add_track_definition(TrackDefinition {
                id: TrackId::new(index),
                object,
                property: Property::Position,
                values: TrackValues::Vec2 {
                    from: Vec2::ZERO,
                    to: Vec2::new(1.0, 0.0),
                },
                timing: TrackTiming::new(1000.0 + index as f64, 1.0, Easing::Linear),
                time_map: noon_core::CompositionTimeMap::identity(),
            })
            .expect("valid sparse track");
    }
    let compiled = CompiledScene::compile(&scene).expect("scene compiles");
    let mut runtime = SceneInstance::new(compiled);
    runtime.seek(0.0).expect("valid seek");

    runtime
        .apply_patch(&ScenePatch::ReplaceTrack(TrackDefinition {
            id: TrackId::new(50_000),
            object: objects[50_000],
            property: Property::Position,
            values: TrackValues::Vec2 {
                from: Vec2::ZERO,
                to: Vec2::new(5.0, 0.0),
            },
            timing: TrackTiming::new(2.0, 1.0, Easing::Linear),
            time_map: noon_core::CompositionTimeMap::identity(),
        }))
        .expect("local replacement succeeds");

    let stats = runtime.last_patch_stats();
    assert_eq!(stats.affected_objects, 1);
    assert_eq!(stats.groups_rebuilt, 1);
    assert_eq!(stats.scheduler_groups_rebuilt, 1);
    assert_eq!(stats.full_group_rebuilds, 0);
    assert_eq!(stats.full_scheduler_rebuilds, 0);
    let scheduler = runtime.last_timeline_scheduler_patch_stats();
    assert_eq!(scheduler.groups_rebuilt, 1);
    assert_eq!(scheduler.events_removed, 2);
    assert_eq!(scheduler.events_added, 2);
}
''')

# Documentation: make the locality boundary explicit.
doc = Path("docs/execution-slots.md")
text = doc.read_text()
text += '''\n## Local timeline relowering\n\nRuntime timeline groups now retain stable `(execution slot, property)` keys rather than start/end offsets into the globally sorted compatibility track array. The event scheduler indexes boundary events by time and stable channel key, so add/replace/remove operations replace only the affected channel's events. Runtime patch instrumentation reports affected objects and rebuilt groups; the 100k-channel regression requires a one-channel replacement to rebuild exactly one group and one scheduler channel. Direct seek remains intentionally global.\n'''
doc.write_text(text)
