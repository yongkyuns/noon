use std::{collections::BTreeSet, ops::Range};

use noon_compile::CompiledScalarSignalTrack;
use noon_core::{evaluate_scalar_track, ReactiveValue, SemanticNodeId, SignalId};
use noon_runtime::TimelineWakeState;

#[derive(Clone, Debug)]
struct SignalTrackGroup {
    semantic: SemanticNodeId,
    execution: SignalId,
    tracks: Range<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum EventKind {
    End,
    Start,
}

#[derive(Clone, Copy, Debug)]
struct SignalTrackEvent {
    time: f64,
    group: usize,
    kind: EventKind,
}

#[derive(Clone, Debug)]
pub(super) struct SignalTimelinePreview {
    event_cursor: usize,
    active: BTreeSet<usize>,
    inputs: Vec<(SignalId, ReactiveValue)>,
}

impl SignalTimelinePreview {
    pub(super) fn inputs(&self) -> &[(SignalId, ReactiveValue)] {
        &self.inputs
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct SignalTimelineSchedule {
    groups: Vec<SignalTrackGroup>,
    tracks: Vec<CompiledScalarSignalTrack>,
    events: Vec<SignalTrackEvent>,
    event_cursor: usize,
    active: BTreeSet<usize>,
    owned_signals: BTreeSet<SemanticNodeId>,
    initialized: bool,
}

impl SignalTimelineSchedule {
    pub(super) fn new(tracks: Vec<CompiledScalarSignalTrack>) -> Self {
        let mut groups = Vec::<SignalTrackGroup>::new();
        let mut start = 0;
        while start < tracks.len() {
            let execution = tracks[start].execution_signal();
            let semantic = tracks[start].semantic_signal();
            let mut end = start + 1;
            while end < tracks.len() && tracks[end].execution_signal() == execution {
                end += 1;
            }
            groups.push(SignalTrackGroup {
                semantic,
                execution,
                tracks: start..end,
            });
            start = end;
        }
        let mut events = Vec::new();
        for (group, range) in groups.iter().map(|group| group.tracks.clone()).enumerate() {
            for track in &tracks[range] {
                let timing = track.timing();
                events.push(SignalTrackEvent {
                    time: timing.start_time,
                    group,
                    kind: EventKind::Start,
                });
                events.push(SignalTrackEvent {
                    time: timing.start_time + timing.duration,
                    group,
                    kind: EventKind::End,
                });
            }
        }
        events.sort_by(|lhs, rhs| {
            lhs.time
                .total_cmp(&rhs.time)
                .then_with(|| lhs.kind.cmp(&rhs.kind))
                .then_with(|| lhs.group.cmp(&rhs.group))
        });
        let owned_signals = groups.iter().map(|group| group.semantic).collect();
        let mut schedule = Self {
            groups,
            tracks,
            events,
            owned_signals,
            ..Self::default()
        };
        if !schedule.groups.is_empty() {
            let preview = schedule.preview_seek(0.0);
            schedule.commit(preview);
        }
        schedule
    }

    pub(super) fn owns(&self, signal: SemanticNodeId) -> bool {
        self.owned_signals.contains(&signal)
    }

    pub(super) fn is_empty(&self) -> bool {
        self.groups.is_empty()
    }

    pub(super) fn is_coherent_at(&self, current: f64, requested: f64) -> bool {
        self.initialized && current == requested
    }

    pub(super) fn preview(&self, current: f64, time: f64) -> SignalTimelinePreview {
        if time < current {
            return self.preview_seek(time);
        }
        let mut cursor = self.event_cursor;
        let mut active = self.active.clone();
        let mut touched = active.clone();
        while let Some(event) = self.events.get(cursor).copied() {
            if event.time > time {
                break;
            }
            touched.insert(event.group);
            match event.kind {
                EventKind::Start => {
                    active.insert(event.group);
                }
                EventKind::End => {
                    active.remove(&event.group);
                }
            }
            cursor += 1;
        }
        SignalTimelinePreview {
            event_cursor: cursor,
            active,
            inputs: touched
                .into_iter()
                .map(|group| {
                    let group = &self.groups[group];
                    (
                        group.execution,
                        ReactiveValue::Scalar(value_at(&self.tracks[group.tracks.clone()], time)),
                    )
                })
                .collect(),
        }
    }

    pub(super) fn preview_seek(&self, time: f64) -> SignalTimelinePreview {
        let event_cursor = self.events.partition_point(|event| event.time <= time);
        let mut active = BTreeSet::new();
        for (index, group) in self.groups.iter().enumerate() {
            let group_tracks = &self.tracks[group.tracks.clone()];
            let next = group_tracks.partition_point(|track| track.timing().start_time <= time);
            if next == 0 {
                continue;
            }
            let track = group_tracks[next - 1];
            if time < track.timing().start_time + track.timing().duration {
                active.insert(index);
            }
        }
        SignalTimelinePreview {
            event_cursor,
            active,
            inputs: self
                .groups
                .iter()
                .map(|group| {
                    (
                        group.execution,
                        ReactiveValue::Scalar(value_at(&self.tracks[group.tracks.clone()], time)),
                    )
                })
                .collect(),
        }
    }

    pub(super) fn commit(&mut self, preview: SignalTimelinePreview) {
        self.event_cursor = preview.event_cursor;
        self.active = preview.active;
        self.initialized = true;
    }

    pub(super) fn wake_state(&self) -> TimelineWakeState {
        if !self.initialized && !self.groups.is_empty() {
            return TimelineWakeState::Continuous;
        }
        if !self.active.is_empty() {
            TimelineWakeState::Continuous
        } else {
            self.events
                .get(self.event_cursor)
                .map_or(TimelineWakeState::Quiescent, |event| {
                    TimelineWakeState::Deadline(event.time)
                })
        }
    }
}

fn value_at(tracks: &[CompiledScalarSignalTrack], time: f64) -> f32 {
    let next = tracks.partition_point(|track| track.timing().start_time <= time);
    if next == 0 {
        return tracks[0].from();
    }
    let track = tracks[next - 1];
    evaluate_scalar_track(track.from() as f64, track.to() as f64, track.timing(), time) as f32
}
