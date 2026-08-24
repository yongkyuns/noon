use noon_compile::CompiledTrack;
use noon_core::Property;

use crate::SceneInstance;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TimelineSchedulerStats {
    pub events_crossed: usize,
    pub active_groups: usize,
    pub groups_requested: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EventKind {
    End,
    Start,
    Instant,
}

#[derive(Clone, Copy, Debug)]
struct TimelineEvent {
    time: f64,
    group: usize,
    kind: EventKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScheduledTrackGroup {
    pub object_index: usize,
    pub property: Property,
    pub start: usize,
    pub end: usize,
}

/// Event index for monotonic timeline playback.
///
/// The scheduler is deliberately independent of semantic scene identity. It
/// consumes today's sorted `CompiledTrack` contract, so the semantic-store and
/// execution-slot migrations can change their inputs later without replacing
/// this scheduling algorithm.
#[derive(Clone, Debug)]
pub struct TimelineEventScheduler {
    groups: Vec<ScheduledTrackGroup>,
    events: Vec<TimelineEvent>,
    event_cursor: usize,
    time: f64,
    active_counts: Vec<u32>,
    active_groups: Vec<usize>,
    active_positions: Vec<usize>,
    visit_epoch: Vec<u64>,
    epoch: u64,
    requested: Vec<usize>,
    last_stats: TimelineSchedulerStats,
}

impl TimelineEventScheduler {
    pub fn new(tracks: &[CompiledTrack]) -> Self {
        let groups = build_groups(tracks);
        let mut events = Vec::with_capacity(tracks.len().saturating_mul(2));
        for (group_index, group) in groups.iter().enumerate() {
            for track in &tracks[group.start..group.end] {
                if track.property == Property::Presence {
                    events.push(TimelineEvent {
                        time: track.timing.start_time,
                        group: group_index,
                        kind: EventKind::Instant,
                    });
                    continue;
                }
                events.push(TimelineEvent {
                    time: track.timing.start_time,
                    group: group_index,
                    kind: EventKind::Start,
                });
                events.push(TimelineEvent {
                    time: track.timing.start_time + track.timing.duration,
                    group: group_index,
                    kind: EventKind::End,
                });
            }
        }
        // End before start at equal timestamps keeps a handoff active when one
        // segment ends exactly where the next begins.
        events.sort_by(|left, right| {
            left.time
                .total_cmp(&right.time)
                .then_with(|| event_rank(left.kind).cmp(&event_rank(right.kind)))
                .then_with(|| left.group.cmp(&right.group))
        });
        let group_count = groups.len();
        Self {
            groups,
            events,
            event_cursor: 0,
            time: f64::NEG_INFINITY,
            active_counts: vec![0; group_count],
            active_groups: Vec::new(),
            active_positions: vec![usize::MAX; group_count],
            visit_epoch: vec![0; group_count],
            epoch: 0,
            requested: Vec::new(),
            last_stats: TimelineSchedulerStats::default(),
        }
    }

    pub fn groups(&self) -> &[ScheduledTrackGroup] {
        &self.groups
    }

    pub fn last_stats(&self) -> TimelineSchedulerStats {
        self.last_stats
    }

    pub fn active_groups(&self) -> &[usize] {
        &self.active_groups
    }

    /// Rebuild scheduler state for direct seek. Direct seek is intentionally
    /// allowed to be O(events); the ordinary forward frame path is not.
    pub fn seek(&mut self, time: f64) {
        self.event_cursor = 0;
        self.time = f64::NEG_INFINITY;
        self.active_counts.fill(0);
        self.active_groups.clear();
        self.active_positions.fill(usize::MAX);
        while self.event_cursor < self.events.len() && self.events[self.event_cursor].time <= time {
            let event = self.events[self.event_cursor];
            self.apply_event(event);
            self.event_cursor += 1;
        }
        self.time = time;
        self.requested.clear();
        self.last_stats = TimelineSchedulerStats {
            events_crossed: self.event_cursor,
            active_groups: self.active_groups.len(),
            groups_requested: 0,
        };
    }

    /// Return exactly the groups that may change at `time`: groups active after
    /// crossing the interval plus groups touched by boundary events. Historical
    /// and future inactive groups are not visited.
    pub fn advance(&mut self, time: f64) -> &[usize] {
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
        let mut events_crossed = 0;
        while self.event_cursor < self.events.len() && self.events[self.event_cursor].time <= time {
            let event = self.events[self.event_cursor];
            if event.time > self.time {
                self.request(event.group);
                self.apply_event(event);
                events_crossed += 1;
            }
            self.event_cursor += 1;
        }

        let active = self.active_groups.clone();
        for group in active {
            self.request(group);
        }
        self.time = time;
        self.last_stats = TimelineSchedulerStats {
            events_crossed,
            active_groups: self.active_groups.len(),
            groups_requested: self.requested.len(),
        };
        &self.requested
    }

    fn begin_request_epoch(&mut self) {
        self.requested.clear();
        self.epoch = self.epoch.wrapping_add(1);
        if self.epoch == 0 {
            self.visit_epoch.fill(0);
            self.epoch = 1;
        }
    }

    fn request(&mut self, group: usize) {
        if self.visit_epoch[group] == self.epoch {
            return;
        }
        self.visit_epoch[group] = self.epoch;
        self.requested.push(group);
    }

    fn apply_event(&mut self, event: TimelineEvent) {
        match event.kind {
            EventKind::Instant => {}
            EventKind::Start => {
                let count = &mut self.active_counts[event.group];
                *count += 1;
                if *count == 1 {
                    self.activate(event.group);
                }
            }
            EventKind::End => {
                let count = &mut self.active_counts[event.group];
                debug_assert!(*count > 0);
                *count = count.saturating_sub(1);
                if *count == 0 {
                    self.deactivate(event.group);
                }
            }
        }
    }

    fn activate(&mut self, group: usize) {
        if self.active_positions[group] != usize::MAX {
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

fn event_rank(kind: EventKind) -> u8 {
    match kind {
        EventKind::End => 0,
        EventKind::Instant => 1,
        EventKind::Start => 2,
    }
}

fn build_groups(tracks: &[CompiledTrack]) -> Vec<ScheduledTrackGroup> {
    let mut groups = Vec::new();
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
        groups.push(ScheduledTrackGroup {
            object_index,
            property,
            start,
            end,
        });
        start = end;
    }
    groups
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
        assert_eq!(scheduler.advance(3.0), &[0]);
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
        assert_eq!(scheduler.advance(1.0), &[0]);
        assert_eq!(scheduler.active_groups(), &[0]);
    }
}
