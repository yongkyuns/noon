use std::collections::{BTreeSet, HashMap};

use noon_compile::{CompiledScalarSignalTimelineEntry, CompiledScalarSignalTrack};
use noon_core::{evaluate_scalar_track, ReactiveValue, SemanticNodeId, SignalId};
use noon_runtime::TimelineWakeState;

#[derive(Clone, Debug)]
struct SignalTimelineGroup {
    semantic: SemanticNodeId,
    execution: SignalId,
    initial: f32,
    entries: Vec<CompiledScalarSignalTimelineEntry>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum EventKind {
    End,
    Hold,
    Start,
}

#[derive(Clone, Copy, Debug)]
struct SignalTimelineEvent {
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

#[derive(Clone, Debug, PartialEq)]
pub enum SignalTimelineAppendError {
    BeforeCurrentTime { current: f64, entry: f64 },
    SignalExecutionMismatch { signal: SemanticNodeId },
    SignalInitialMismatch { signal: SemanticNodeId },
    OverlappingEntries { signal: SemanticNodeId },
    DiscontinuousEntries { signal: SemanticNodeId },
}

impl std::fmt::Display for SignalTimelineAppendError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BeforeCurrentTime { current, entry } => write!(
                formatter,
                "scalar timeline entry starts at {entry}, before current authored time {current}"
            ),
            Self::SignalExecutionMismatch { signal } => write!(
                formatter,
                "semantic signal {}:{} changed its derived execution identity",
                signal.slot(),
                signal.generation()
            ),
            Self::SignalInitialMismatch { signal } => write!(
                formatter,
                "semantic signal {}:{} changed its compiled initial value",
                signal.slot(),
                signal.generation()
            ),
            Self::OverlappingEntries { signal } => write!(
                formatter,
                "semantic signal {}:{} has overlapping scalar timeline entries",
                signal.slot(),
                signal.generation()
            ),
            Self::DiscontinuousEntries { signal } => write!(
                formatter,
                "semantic signal {}:{} has discontinuous scalar timeline entries",
                signal.slot(),
                signal.generation()
            ),
        }
    }
}

impl std::error::Error for SignalTimelineAppendError {}

#[derive(Clone, Copy, Debug)]
pub(super) struct PreparedSignalTimelineAppend {
    entries: Vec<CompiledScalarSignalTimelineEntry>,
    current: f64,
}

#[derive(Clone, Debug, Default)]
pub(super) struct SignalTimelineSchedule {
    groups: Vec<SignalTimelineGroup>,
    group_by_signal: HashMap<SemanticNodeId, usize>,
    events: Vec<SignalTimelineEvent>,
    event_cursor: usize,
    active: BTreeSet<usize>,
    owned_signals: BTreeSet<SemanticNodeId>,
    initialized: bool,
}

impl SignalTimelineSchedule {
    pub(super) fn new(entries: Vec<CompiledScalarSignalTimelineEntry>) -> Self {
        let mut groups = Vec::<SignalTimelineGroup>::new();
        let mut group_by_signal = HashMap::new();
        for entry in entries {
            let semantic = entry.semantic_signal();
            let execution = entry.execution_signal();
            let initial = entry.initial_value();
            let group = *group_by_signal.entry(semantic).or_insert_with(|| {
                let group = groups.len();
                groups.push(SignalTimelineGroup {
                    semantic,
                    execution,
                    initial,
                    entries: Vec::new(),
                });
                group
            });
            debug_assert_eq!(groups[group].execution, execution);
            groups[group].entries.push(entry);
        }

        let mut events = Vec::new();
        let mut owned_signals = BTreeSet::new();
        for (group_index, group) in groups.iter().enumerate() {
            if matches!(
                group.entries.last(),
                Some(CompiledScalarSignalTimelineEntry::Track(_))
            ) {
                owned_signals.insert(group.semantic);
            }
            for entry in &group.entries {
                append_events(&mut events, group_index, entry);
            }
        }
        events.sort_by(|lhs, rhs| {
            lhs.time
                .total_cmp(&rhs.time)
                .then_with(|| lhs.kind.cmp(&rhs.kind))
                .then_with(|| lhs.group.cmp(&rhs.group))
        });

        let mut schedule = Self {
            groups,
            group_by_signal,
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

    /// Whether this signal has any authored timeline meaning. Raw execution
    /// input must never override such a signal, including after a Hold releases
    /// ordinary authoring ownership or while the runtime is seeking history.
    pub(super) fn has_history(&self, signal: SemanticNodeId) -> bool {
        self.group_by_signal.contains_key(&signal)
    }

    pub(super) fn is_empty(&self) -> bool {
        self.groups.is_empty()
    }

    pub(super) fn is_coherent_at(&self, current: f64, requested: f64) -> bool {
        self.initialized && current == requested
    }

    pub(super) fn prepare_append_batch(
        &self,
        entries: impl IntoIterator<Item = CompiledScalarSignalTimelineEntry>,
        current: f64,
    ) -> Result<PreparedSignalTimelineAppend, SignalTimelineAppendError> {
        #[derive(Clone, Copy)]
        struct Tail {
            execution: SignalId,
            initial: f32,
            end: f64,
            value: f32,
        }

        let entries = entries.into_iter().collect::<Vec<_>>();
        let mut tails = HashMap::<SemanticNodeId, Tail>::new();
        for entry in &entries {
            if entry.start_time() < current {
                return Err(SignalTimelineAppendError::BeforeCurrentTime {
                    current,
                    entry: entry.start_time(),
                });
            }
            let signal = entry.semantic_signal();
            let tail = *tails.entry(signal).or_insert_with(|| {
                self.group_by_signal
                    .get(&signal)
                    .and_then(|&group| {
                        self.groups[group].entries.last().map(|entry| Tail {
                            execution: self.groups[group].execution,
                            initial: self.groups[group].initial,
                            end: entry_end(entry),
                            value: entry_value(entry),
                        })
                    })
                    .unwrap_or(Tail {
                        execution: entry.execution_signal(),
                        initial: entry.initial_value(),
                        end: f64::NEG_INFINITY,
                        value: entry.initial_value(),
                    })
            });
            if tail.execution != entry.execution_signal() {
                return Err(SignalTimelineAppendError::SignalExecutionMismatch { signal });
            }
            if tail.initial != entry.initial_value() {
                return Err(SignalTimelineAppendError::SignalInitialMismatch { signal });
            }
            if entry.start_time() < tail.end {
                return Err(SignalTimelineAppendError::OverlappingEntries { signal });
            }
            if tail.end.is_finite() && entry_start_value(entry) != tail.value {
                return Err(SignalTimelineAppendError::DiscontinuousEntries { signal });
            }
            tails.insert(
                signal,
                Tail {
                    execution: tail.execution,
                    initial: tail.initial,
                    end: entry_end(entry),
                    value: entry_value(entry),
                },
            );
        }
        Ok(PreparedSignalTimelineAppend { entries, current })
    }

    pub(super) fn commit_append(&mut self, prepared: PreparedSignalTimelineAppend) {
        for entry in prepared.entries {
            let semantic = entry.semantic_signal();
            let execution = entry.execution_signal();
            let initial = entry.initial_value();
            let group = self
                .group_by_signal
                .get(&semantic)
                .copied()
                .unwrap_or_else(|| {
                    let group = self.groups.len();
                    self.groups.push(SignalTimelineGroup {
                        semantic,
                        execution,
                        initial,
                        entries: Vec::new(),
                    });
                    self.group_by_signal.insert(semantic, group);
                    group
                });
            append_events(&mut self.events, group, &entry);
            self.groups[group].entries.push(entry);
        }
        self.events[self.event_cursor..].sort_by(|lhs, rhs| {
            lhs.time
                .total_cmp(&rhs.time)
                .then_with(|| lhs.kind.cmp(&rhs.kind))
                .then_with(|| lhs.group.cmp(&rhs.group))
        });
        while let Some(event) = self.events.get(self.event_cursor).copied() {
            if event.time > prepared.current {
                break;
            }
            match event.kind {
                EventKind::Start => {
                    self.active.insert(event.group);
                    self.owned_signals.insert(self.groups[event.group].semantic);
                }
                EventKind::End => {
                    self.active.remove(&event.group);
                }
                EventKind::Hold => {
                    self.active.remove(&event.group);
                    self.owned_signals
                        .remove(&self.groups[event.group].semantic);
                }
            }
            self.event_cursor += 1;
        }
        self.initialized = true;
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
                EventKind::End | EventKind::Hold => {
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
                        ReactiveValue::Scalar(value_at(&group.entries, group.initial, time)),
                    )
                })
                .collect(),
        }
    }

    pub(super) fn preview_seek(&self, time: f64) -> SignalTimelinePreview {
        let event_cursor = self.events.partition_point(|event| event.time <= time);
        let mut active = BTreeSet::new();
        for (index, group) in self.groups.iter().enumerate() {
            let next = group
                .entries
                .partition_point(|entry| entry.start_time() <= time);
            if next == 0 {
                continue;
            }
            if let CompiledScalarSignalTimelineEntry::Track(track) = &group.entries[next - 1] {
                if time < track_end(track) {
                    active.insert(index);
                }
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
                        ReactiveValue::Scalar(value_at(&group.entries, group.initial, time)),
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

    #[cfg(test)]
    pub(super) fn entry_count(&self) -> usize {
        self.groups.iter().map(|group| group.entries.len()).sum()
    }

    #[cfg(test)]
    pub(super) fn event_count(&self) -> usize {
        self.events.len()
    }
}

fn append_events(
    events: &mut Vec<SignalTimelineEvent>,
    group: usize,
    entry: &CompiledScalarSignalTimelineEntry,
) {
    match entry {
        CompiledScalarSignalTimelineEntry::Track(track) => {
            events.push(SignalTimelineEvent {
                time: noon_core::continuous_time_map_interval(track.timing(), track.time_map())
                    .expect("compiled scalar track retains a validated monotone time map")
                    .0,
                group,
                kind: EventKind::Start,
            });
            events.push(SignalTimelineEvent {
                time: track_end(track),
                group,
                kind: EventKind::End,
            });
        }
        CompiledScalarSignalTimelineEntry::Hold(hold) => {
            events.push(SignalTimelineEvent {
                time: hold.start_time(),
                group,
                kind: EventKind::Hold,
            });
        }
    }
}

fn track_end(track: &CompiledScalarSignalTrack) -> f64 {
    noon_core::continuous_time_map_interval(track.timing(), track.time_map())
        .expect("compiled scalar track retains a validated monotone time map")
        .1
}

fn entry_end(entry: &CompiledScalarSignalTimelineEntry) -> f64 {
    match entry {
        CompiledScalarSignalTimelineEntry::Track(track) => track_end(track),
        CompiledScalarSignalTimelineEntry::Hold(hold) => hold.start_time(),
    }
}

fn entry_start_value(entry: &CompiledScalarSignalTimelineEntry) -> f32 {
    match entry {
        CompiledScalarSignalTimelineEntry::Track(track) => track.from(),
        CompiledScalarSignalTimelineEntry::Hold(hold) => hold.value(),
    }
}

fn entry_value(entry: &CompiledScalarSignalTimelineEntry) -> f32 {
    match entry {
        CompiledScalarSignalTimelineEntry::Track(track) => track.to(),
        CompiledScalarSignalTimelineEntry::Hold(hold) => hold.value(),
    }
}

fn value_at(entries: &[CompiledScalarSignalTimelineEntry], initial: f32, time: f64) -> f32 {
    let next = entries.partition_point(|entry| entry.start_time() <= time);
    if next == 0 {
        return initial;
    }
    match &entries[next - 1] {
        CompiledScalarSignalTimelineEntry::Track(track) => evaluate_scalar_track(
            track.from() as f64,
            track.to() as f64,
            track.timing(),
            track.time_map(),
            time,
        ) as f32,
        CompiledScalarSignalTimelineEntry::Hold(hold) => hold.value(),
    }
}
