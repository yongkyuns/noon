use std::cmp::Ordering;
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
            self.request_active_groups();
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

        self.request_active_groups();
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

    /// Request active stable channels without cloning the active set.
    fn request_active_groups(&mut self) {
        for index in 0..self.active_groups.len() {
            let group = self.active_groups[index];
            self.request(group);
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
        self.active_positions
            .insert(group, self.active_groups.len());
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
            tracks.push(position_track(
                index as u64,
                index,
                1000.0 + index as f64,
                1.0,
            ));
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
