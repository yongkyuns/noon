use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet},
    ops::Bound::{Excluded, Included, Unbounded},
};

use noon_compile::{CompiledChannelKey, CompiledScene, CompiledTrack};
use noon_core::TrackId;

use crate::SceneInstance;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TimelineSchedulerStats {
    pub events_crossed: usize,
    pub active_groups: usize,
    pub groups_requested: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TimelineRelowerStats {
    pub groups_relowered: usize,
    pub events_removed: usize,
    pub events_inserted: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TimelineAdvancePreview {
    requested: Vec<CompiledChannelKey>,
    stats: TimelineSchedulerStats,
}

impl TimelineAdvancePreview {
    pub(crate) fn requested(&self) -> &[CompiledChannelKey] {
        &self.requested
    }

    pub(crate) const fn stats(&self) -> TimelineSchedulerStats {
        self.stats
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EventKind {
    End,
    Start,
    Instant,
}

#[derive(Clone, Copy, Debug)]
struct EventTime(f64);

impl PartialEq for EventTime {
    fn eq(&self, other: &Self) -> bool {
        self.0.total_cmp(&other.0) == Ordering::Equal
    }
}

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

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct TimelineEventKey {
    time: EventTime,
    rank: u8,
    group: usize,
    track: TrackId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ScheduledTrackGroup {
    channel: CompiledChannelKey,
}

/// Event-driven scheduler with stable channel slots.
///
/// Forward playback remains proportional to active groups plus crossed events.
/// Timeline mutation can replace one object/property channel without rebuilding
/// unrelated groups or their events. Direct seek is intentionally O(events).
#[derive(Clone, Debug)]
pub struct TimelineEventScheduler {
    groups: Vec<Option<ScheduledTrackGroup>>,
    group_indices: BTreeMap<CompiledChannelKey, usize>,
    free_groups: Vec<usize>,
    events: BTreeMap<TimelineEventKey, EventKind>,
    group_events: Vec<Vec<TimelineEventKey>>,
    time: f64,
    active_counts: Vec<u32>,
    active_groups: Vec<usize>,
    active_positions: Vec<usize>,
    visit_epoch: Vec<u64>,
    epoch: u64,
    requested: Vec<CompiledChannelKey>,
    crossed: Vec<(TimelineEventKey, EventKind)>,
    last_stats: TimelineSchedulerStats,
}

impl TimelineEventScheduler {
    pub fn new(tracks: &[CompiledTrack]) -> Self {
        let mut scheduler = Self {
            groups: Vec::new(),
            group_indices: BTreeMap::new(),
            free_groups: Vec::new(),
            events: BTreeMap::new(),
            group_events: Vec::new(),
            time: f64::NEG_INFINITY,
            active_counts: Vec::new(),
            active_groups: Vec::new(),
            active_positions: Vec::new(),
            visit_epoch: Vec::new(),
            epoch: 0,
            requested: Vec::new(),
            crossed: Vec::new(),
            last_stats: TimelineSchedulerStats::default(),
        };

        let mut start = 0;
        while start < tracks.len() {
            let channel =
                CompiledChannelKey::new(tracks[start].object_index, tracks[start].property);
            let mut end = start + 1;
            while end < tracks.len()
                && tracks[end].object_index == channel.object_index
                && tracks[end].property == channel.property
            {
                end += 1;
            }
            scheduler.relower_channel(channel, &tracks[start..end]);
            start = end;
        }
        scheduler.last_stats = TimelineSchedulerStats::default();
        scheduler
    }

    pub fn from_compiled(compiled: &CompiledScene) -> Self {
        let mut scheduler = Self::new(&[]);
        for channel in compiled.channels() {
            scheduler.relower_channel(channel, compiled.channel_tracks(channel));
        }
        scheduler.last_stats = TimelineSchedulerStats::default();
        scheduler
    }

    pub const fn last_stats(&self) -> TimelineSchedulerStats {
        self.last_stats
    }

    pub fn active_groups(&self) -> &[usize] {
        &self.active_groups
    }

    pub fn requested(&self) -> &[CompiledChannelKey] {
        &self.requested
    }

    pub fn live_group_count(&self) -> usize {
        self.group_indices.len()
    }

    pub(crate) fn next_event_time(&self) -> Option<f64> {
        let lower = time_upper_bound(self.time);
        self.events
            .range((Excluded(lower), Unbounded))
            .next()
            .map(|(key, _kind)| key.time.0)
    }

    /// Replace the event/index lowering for exactly one object/property channel.
    /// Empty tracks remove the channel and free its stable scheduler slot.
    pub fn relower_channel(
        &mut self,
        channel: CompiledChannelKey,
        tracks: &[CompiledTrack],
    ) -> TimelineRelowerStats {
        debug_assert!(tracks
            .iter()
            .all(|track| track.object_index == channel.object_index
                && track.property == channel.property));

        if tracks.is_empty() {
            let Some(group) = self.group_indices.remove(&channel) else {
                return TimelineRelowerStats::default();
            };
            let removed = self.remove_group_events(group);
            self.active_counts[group] = 0;
            self.deactivate(group);
            self.groups[group] = None;
            self.free_groups.push(group);
            self.visit_epoch[group] = 0;
            return TimelineRelowerStats {
                groups_relowered: 1,
                events_removed: removed,
                events_inserted: 0,
            };
        }

        let (group, events_removed) = if let Some(group) = self.group_indices.get(&channel).copied()
        {
            (group, self.remove_group_events(group))
        } else {
            (self.allocate_group(channel), 0)
        };
        self.groups[group] = Some(ScheduledTrackGroup { channel });
        self.group_indices.insert(channel, group);

        let mut inserted = 0;
        for track in tracks {
            if track.timing.is_instant() {
                inserted +=
                    self.insert_event(group, track.id, track.timing.start_time, EventKind::Instant);
            } else {
                inserted +=
                    self.insert_event(group, track.id, track.timing.start_time, EventKind::Start);
                inserted += self.insert_event(
                    group,
                    track.id,
                    track.timing.start_time + track.timing.duration,
                    EventKind::End,
                );
            }
        }
        self.recompute_group_activity(group, tracks);
        TimelineRelowerStats {
            groups_relowered: 1,
            events_removed,
            events_inserted: inserted,
        }
    }

    /// Rebuild scheduler state for direct seek. Direct seek is intentionally
    /// allowed to be O(events); the ordinary forward frame path is not.
    pub fn seek(&mut self, time: f64) {
        self.time = time;
        self.active_counts.fill(0);
        self.active_groups.clear();
        self.active_positions.fill(usize::MAX);
        let upper = time_upper_bound(time);
        let mut events_crossed = 0;
        for (key, kind) in self.events.range(..=upper) {
            events_crossed += 1;
            match kind {
                EventKind::Instant => {}
                EventKind::Start => self.active_counts[key.group] += 1,
                EventKind::End => {
                    self.active_counts[key.group] = self.active_counts[key.group].saturating_sub(1)
                }
            }
        }
        for group in 0..self.active_counts.len() {
            if self.active_counts[group] > 0 && self.groups[group].is_some() {
                self.activate(group);
            }
        }
        self.requested.clear();
        self.last_stats = TimelineSchedulerStats {
            events_crossed,
            active_groups: self.active_groups.len(),
            groups_requested: 0,
        };
    }

    /// Build the reusable request buffer for groups that can change at `time`.
    /// The return value is the request count; callers can read `requested()` one
    /// channel at a time without cloning the active set.
    pub fn advance(&mut self, time: f64) -> usize {
        if time < self.time {
            self.seek(time);
            self.begin_request_epoch();
            self.request_active_groups();
            self.last_stats.groups_requested = self.requested.len();
            return self.requested.len();
        }

        self.begin_request_epoch();
        self.crossed.clear();
        let lower = time_upper_bound(self.time);
        let upper = time_upper_bound(time);
        self.crossed.extend(
            self.events
                .range((Excluded(lower), Included(upper)))
                .map(|(key, kind)| (*key, *kind)),
        );
        for index in 0..self.crossed.len() {
            let (key, kind) = self.crossed[index];
            self.request(key.group);
            self.apply_event(key.group, kind);
        }

        let events_crossed = self.crossed.len();
        self.request_active_groups();
        self.time = time;
        self.last_stats = TimelineSchedulerStats {
            events_crossed,
            active_groups: self.active_groups.len(),
            groups_requested: self.requested.len(),
        };
        self.requested.len()
    }

    /// Derive the exact forward request order without changing scheduler state.
    ///
    /// Scratch storage is proportional to the active groups plus groups whose
    /// events are crossed. This is used by a required host-callback barrier to
    /// evaluate a sparse tentative frame while the last coherent scheduler/time
    /// remains live.
    pub(crate) fn preview_advance(&self, time: f64) -> TimelineAdvancePreview {
        debug_assert!(time >= self.time);

        let mut active_groups = self.active_groups.clone();
        let mut active_positions = active_groups
            .iter()
            .copied()
            .enumerate()
            .map(|(position, group)| (group, position))
            .collect::<BTreeMap<_, _>>();
        let mut changed_counts = BTreeMap::<usize, u32>::new();
        let mut requested_groups = Vec::new();
        let mut requested = BTreeSet::new();
        let mut events_crossed = 0;

        let mut request = |group: usize| {
            if self.groups[group].is_some() && requested.insert(group) {
                requested_groups.push(group);
            }
        };

        let lower = time_upper_bound(self.time);
        let upper = time_upper_bound(time);
        for (key, kind) in self.events.range((Excluded(lower), Included(upper))) {
            events_crossed += 1;
            request(key.group);
            let count = changed_counts
                .entry(key.group)
                .or_insert(self.active_counts[key.group]);
            match kind {
                EventKind::Instant => {}
                EventKind::Start => {
                    *count += 1;
                    if *count == 1 && !active_positions.contains_key(&key.group) {
                        active_positions.insert(key.group, active_groups.len());
                        active_groups.push(key.group);
                    }
                }
                EventKind::End => {
                    *count = count.saturating_sub(1);
                    if *count == 0 {
                        let Some(position) = active_positions.remove(&key.group) else {
                            continue;
                        };
                        let last = active_groups.len() - 1;
                        active_groups.swap(position, last);
                        active_groups.pop();
                        if position < active_groups.len() {
                            active_positions.insert(active_groups[position], position);
                        }
                    }
                }
            }
        }
        for group in &active_groups {
            request(*group);
        }

        TimelineAdvancePreview {
            requested: requested_groups
                .into_iter()
                .filter_map(|group| self.groups[group].map(|scheduled| scheduled.channel))
                .collect(),
            stats: TimelineSchedulerStats {
                events_crossed,
                active_groups: active_groups.len(),
                groups_requested: requested.len(),
            },
        }
    }

    fn allocate_group(&mut self, channel: CompiledChannelKey) -> usize {
        if let Some(group) = self.free_groups.pop() {
            self.groups[group] = Some(ScheduledTrackGroup { channel });
            self.active_counts[group] = 0;
            self.active_positions[group] = usize::MAX;
            self.visit_epoch[group] = 0;
            self.group_events[group].clear();
            return group;
        }
        let group = self.groups.len();
        self.groups.push(Some(ScheduledTrackGroup { channel }));
        self.group_events.push(Vec::new());
        self.active_counts.push(0);
        self.active_positions.push(usize::MAX);
        self.visit_epoch.push(0);
        group
    }

    fn insert_event(&mut self, group: usize, track: TrackId, time: f64, kind: EventKind) -> usize {
        let key = TimelineEventKey {
            time: EventTime(time),
            rank: event_rank(kind),
            group,
            track,
        };
        let previous = self.events.insert(key, kind);
        debug_assert!(previous.is_none());
        self.group_events[group].push(key);
        usize::from(previous.is_none())
    }

    fn remove_group_events(&mut self, group: usize) -> usize {
        let keys = std::mem::take(&mut self.group_events[group]);
        let mut removed = 0;
        for key in keys {
            removed += usize::from(self.events.remove(&key).is_some());
        }
        removed
    }

    fn recompute_group_activity(&mut self, group: usize, tracks: &[CompiledTrack]) {
        self.active_counts[group] = 0;
        self.deactivate(group);
        if self.time == f64::NEG_INFINITY {
            return;
        }
        let count = tracks
            .iter()
            .filter(|track| {
                !track.timing.is_instant()
                    && track.timing.start_time <= self.time
                    && self.time < track.timing.start_time + track.timing.duration
            })
            .count();
        self.active_counts[group] = u32::try_from(count).expect("active track count exceeds u32");
        if count > 0 {
            self.activate(group);
        }
    }

    fn begin_request_epoch(&mut self) {
        self.requested.clear();
        self.epoch = self.epoch.wrapping_add(1);
        if self.epoch == 0 {
            self.visit_epoch.fill(0);
            self.epoch = 1;
        }
    }

    fn request_active_groups(&mut self) {
        for index in 0..self.active_groups.len() {
            let group = self.active_groups[index];
            self.request(group);
        }
    }

    fn request(&mut self, group: usize) {
        if self.visit_epoch[group] == self.epoch {
            return;
        }
        let Some(scheduled) = self.groups[group] else {
            return;
        };
        self.visit_epoch[group] = self.epoch;
        self.requested.push(scheduled.channel);
    }

    fn apply_event(&mut self, group: usize, kind: EventKind) {
        match kind {
            EventKind::Instant => {}
            EventKind::Start => {
                self.active_counts[group] += 1;
                if self.active_counts[group] == 1 {
                    self.activate(group);
                }
            }
            EventKind::End => {
                self.active_counts[group] = self.active_counts[group].saturating_sub(1);
                if self.active_counts[group] == 0 {
                    self.deactivate(group);
                }
            }
        }
    }

    fn activate(&mut self, group: usize) {
        if self.active_positions[group] != usize::MAX || self.groups[group].is_none() {
            return;
        }
        self.active_positions[group] = self.active_groups.len();
        self.active_groups.push(group);
    }

    fn deactivate(&mut self, group: usize) {
        let position = self.active_positions[group];
        if position == usize::MAX {
            return;
        }
        let last = self.active_groups.len() - 1;
        self.active_groups.swap(position, last);
        let removed = self.active_groups.pop().expect("active group exists");
        debug_assert_eq!(removed, group);
        self.active_positions[group] = usize::MAX;
        if position < self.active_groups.len() {
            let moved = self.active_groups[position];
            self.active_positions[moved] = position;
        }
    }
}

impl SceneInstance {
    pub fn last_timeline_scheduler_stats(&self) -> TimelineSchedulerStats {
        self.timeline_scheduler.last_stats()
    }
}

fn time_upper_bound(time: f64) -> TimelineEventKey {
    TimelineEventKey {
        time: EventTime(time),
        rank: u8::MAX,
        group: usize::MAX,
        track: TrackId::new(u64::MAX),
    }
}

const fn event_rank(kind: EventKind) -> u8 {
    match kind {
        EventKind::End => 0,
        EventKind::Instant => 1,
        EventKind::Start => 2,
    }
}

#[cfg(test)]
mod tests {
    use noon_compile::{CompiledChannelKey, CompiledTrack};
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
            reconciled: false,
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
        assert_eq!(scheduler.advance(1.0 / 60.0), 0);
        assert!(scheduler.requested().is_empty());
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
        assert_eq!(scheduler.advance(0.5), 10);
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
        assert_eq!(scheduler.advance(3.0), 1);
        assert_eq!(
            scheduler.requested(),
            &[CompiledChannelKey::new(0, Property::Position)]
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
        assert_eq!(scheduler.advance(1.0), 1);
        assert_eq!(scheduler.active_groups().len(), 1);
    }

    #[test]
    fn next_event_time_tracks_strictly_future_boundary() {
        let tracks = vec![
            position_track(1, 0, 1.0, 2.0),
            position_track(2, 1, 4.0, 1.0),
        ];
        let mut scheduler = TimelineEventScheduler::new(&tracks);
        scheduler.seek(0.0);
        assert_eq!(scheduler.next_event_time(), Some(1.0));
        scheduler.advance(1.0);
        assert_eq!(scheduler.next_event_time(), Some(3.0));
        scheduler.advance(3.0);
        assert_eq!(scheduler.next_event_time(), Some(4.0));
        scheduler.advance(4.0);
        assert_eq!(scheduler.next_event_time(), Some(5.0));
        scheduler.advance(5.0);
        assert_eq!(scheduler.next_event_time(), None);
    }

    #[test]
    fn relowering_one_large_timeline_channel_does_not_rebuild_other_groups() {
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
        let channel = CompiledChannelKey::new(50_000, Property::Position);
        let replacement = [
            position_track(200_000, 50_000, 2.0, 1.0),
            position_track(200_001, 50_000, 4.0, 1.0),
        ];
        let stats = scheduler.relower_channel(channel, &replacement);
        assert_eq!(stats.groups_relowered, 1);
        assert_eq!(stats.events_removed, 2);
        assert_eq!(stats.events_inserted, 4);
        assert_eq!(scheduler.live_group_count(), 100_000);
        assert_eq!(scheduler.advance(2.5), 1);
        assert_eq!(scheduler.requested(), &[channel]);
    }

    #[test]
    fn forward_preview_matches_mutating_request_order_and_stats() {
        let tracks = vec![
            position_track(1, 0, 0.0, 2.0),
            position_track(2, 1, 1.0, 3.0),
            position_track(3, 2, 3.0, 1.0),
        ];
        let mut scheduler = TimelineEventScheduler::new(&tracks);
        scheduler.seek(0.5);

        let preview = scheduler.preview_advance(3.5);
        let requested = preview.requested().to_vec();
        let stats = preview.stats();
        scheduler.advance(3.5);

        assert_eq!(scheduler.requested(), requested);
        assert_eq!(scheduler.last_stats(), stats);
    }
}
