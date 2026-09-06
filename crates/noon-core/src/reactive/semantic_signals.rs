use std::collections::HashMap;

use super::{
    NativeEventSource, NativeStateSource, SemanticNodeId, SemanticNodeKind, SemanticStore,
    SemanticVec3,
};
use crate::{validate_continuous_track_timing, TimelineError, TrackTiming};

/// One authored scalar timeline interval owned by a semantic input signal.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SemanticScalarSignalTrack {
    signal: SemanticNodeId,
    from: f64,
    to: f64,
    timing: TrackTiming,
}

/// One persistent scalar value beginning at an authored timeline boundary.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SemanticScalarSignalHold {
    signal: SemanticNodeId,
    value: f64,
    start_time: f64,
}

impl SemanticScalarSignalHold {
    pub const fn new(signal: SemanticNodeId, value: f64, start_time: f64) -> Self {
        Self {
            signal,
            value,
            start_time,
        }
    }

    pub const fn signal(self) -> SemanticNodeId {
        self.signal
    }

    pub const fn value(self) -> f64 {
        self.value
    }

    pub const fn start_time(self) -> f64 {
        self.start_time
    }
}

/// One entry in a semantic input signal's ordered authored timeline.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SemanticScalarSignalTimelineEntry {
    Track(SemanticScalarSignalTrack),
    Hold(SemanticScalarSignalHold),
}

impl SemanticScalarSignalTimelineEntry {
    pub const fn signal(self) -> SemanticNodeId {
        match self {
            Self::Track(track) => track.signal(),
            Self::Hold(hold) => hold.signal(),
        }
    }

    pub const fn start_time(self) -> f64 {
        match self {
            Self::Track(track) => track.timing().start_time,
            Self::Hold(hold) => hold.start_time(),
        }
    }

    pub const fn terminal_value(self) -> f64 {
        match self {
            Self::Track(track) => track.to(),
            Self::Hold(hold) => hold.value(),
        }
    }
}

impl SemanticScalarSignalTrack {
    pub const fn new(signal: SemanticNodeId, from: f64, to: f64, timing: TrackTiming) -> Self {
        Self {
            signal,
            from,
            to,
            timing,
        }
    }

    pub const fn signal(self) -> SemanticNodeId {
        self.signal
    }

    pub const fn from(self) -> f64 {
        self.from
    }

    pub const fn to(self) -> f64 {
        self.to
    }

    pub const fn timing(self) -> TrackTiming {
        self.timing
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SemanticScalarSignalQueryError {
    Signal(SemanticSignalError),
    NotInputSignal(SemanticNodeId),
    NonScalarSignal(SemanticNodeId),
    InvalidTime,
}

impl std::fmt::Display for SemanticScalarSignalQueryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Signal(error) => error.fmt(formatter),
            Self::NotInputSignal(signal) => {
                write!(formatter, "semantic signal {signal:?} is derived")
            }
            Self::NonScalarSignal(signal) => {
                write!(formatter, "semantic signal {signal:?} is not scalar")
            }
            Self::InvalidTime => formatter.write_str("semantic signal query time must be finite"),
        }
    }
}

impl std::error::Error for SemanticScalarSignalQueryError {}

impl From<SemanticSignalError> for SemanticScalarSignalQueryError {
    fn from(value: SemanticSignalError) -> Self {
        Self::Signal(value)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum SemanticScalarSignalTrackError {
    Signal(SemanticSignalError),
    Timeline(TimelineError),
    NotInputSignal(SemanticNodeId),
    NonScalarSignal(SemanticNodeId),
    NativeOwnedSignal(SemanticNodeId),
    NonFiniteValue {
        signal: SemanticNodeId,
        value: f64,
    },
    ZeroDuration(SemanticNodeId),
    NonFiniteEndTime(SemanticNodeId),
    NonFiniteHoldTime(SemanticNodeId),
    OverlappingTracks {
        signal: SemanticNodeId,
        previous_end: f64,
        next_start: f64,
    },
    DiscontinuousTrack {
        signal: SemanticNodeId,
        expected: f64,
        actual: f64,
    },
}

impl std::fmt::Display for SemanticScalarSignalTrackError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Signal(error) => error.fmt(formatter),
            Self::Timeline(error) => error.fmt(formatter),
            Self::NotInputSignal(signal) => write!(formatter, "semantic signal {signal:?} is derived"),
            Self::NonScalarSignal(signal) => write!(formatter, "semantic signal {signal:?} is not scalar"),
            Self::NativeOwnedSignal(signal) => write!(formatter, "semantic signal {signal:?} is owned by native input"),
            Self::NonFiniteValue { signal, value } => write!(formatter, "semantic signal {signal:?} track value must be finite, got {value}"),
            Self::ZeroDuration(signal) => write!(formatter, "semantic signal {signal:?} track duration must be positive"),
            Self::NonFiniteEndTime(signal) => write!(formatter, "semantic signal {signal:?} track end time must be finite"),
            Self::NonFiniteHoldTime(signal) => write!(formatter, "semantic signal {signal:?} hold time must be finite"),
            Self::OverlappingTracks { signal, previous_end, next_start } => write!(formatter, "semantic signal {signal:?} track starts at {next_start} before prior end {previous_end}"),
            Self::DiscontinuousTrack { signal, expected, actual } => write!(formatter, "semantic signal {signal:?} track begins at {actual}, expected {expected}"),
        }
    }
}

impl std::error::Error for SemanticScalarSignalTrackError {}

impl From<SemanticSignalError> for SemanticScalarSignalTrackError {
    fn from(value: SemanticSignalError) -> Self {
        Self::Signal(value)
    }
}

impl From<TimelineError> for SemanticScalarSignalTrackError {
    fn from(value: TimelineError) -> Self {
        Self::Timeline(value)
    }
}

/// Stable authored value kind of a semantic signal.
///
/// This is semantic vocabulary, not the execution-layer `ValueKind`. Lowering may
/// specialize `Vec3` or scalar precision for a target runtime without changing the
/// authored signal contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SemanticSignalValueKind {
    Bool,
    Scalar,
    Vec3,
}

impl std::fmt::Display for SemanticSignalValueKind {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Bool => "bool",
            Self::Scalar => "scalar",
            Self::Vec3 => "vec3",
        })
    }
}

/// High-precision authored value carried by a semantic signal.
///
/// This is semantic state, not a runtime slot value. Lowering may specialize it
/// to a narrower representation when the target execution plan permits that.
#[derive(Clone, Debug, PartialEq)]
pub enum SemanticSignalValue {
    Bool(bool),
    Scalar(f64),
    Vec3(SemanticVec3),
}

impl SemanticSignalValue {
    pub const fn value_kind(&self) -> SemanticSignalValueKind {
        match self {
            Self::Bool(_) => SemanticSignalValueKind::Bool,
            Self::Scalar(_) => SemanticSignalValueKind::Scalar,
            Self::Vec3(_) => SemanticSignalValueKind::Vec3,
        }
    }

    pub fn is_finite(&self) -> bool {
        match self {
            Self::Bool(_) => true,
            Self::Scalar(value) => value.is_finite(),
            Self::Vec3(value) => value.is_finite(),
        }
    }
}

impl From<bool> for SemanticSignalValue {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

impl From<f64> for SemanticSignalValue {
    fn from(value: f64) -> Self {
        Self::Scalar(value)
    }
}

impl From<f32> for SemanticSignalValue {
    fn from(value: f32) -> Self {
        Self::Scalar(value as f64)
    }
}

impl From<SemanticVec3> for SemanticSignalValue {
    fn from(value: SemanticVec3) -> Self {
        Self::Vec3(value)
    }
}

/// Author-authored native reactive expression over semantic signal identity.
///
/// Signal references use the same scene-global generational [`SemanticNodeId`]
/// as every other semantic entity. `SignalId` remains a migration/execution-era
/// identity and is deliberately absent from the target authored model.
#[derive(Clone, Debug, PartialEq)]
pub enum SemanticSignalExpr {
    Constant(SemanticSignalValue),
    Signal(SemanticNodeId),
    Add(Box<Self>, Box<Self>),
    Sub(Box<Self>, Box<Self>),
    Mul(Box<Self>, Box<Self>),
    Neg(Box<Self>),
    Sin(Box<Self>),
    Cos(Box<Self>),
}

impl SemanticSignalExpr {
    pub const fn signal(signal: SemanticNodeId) -> Self {
        Self::Signal(signal)
    }

    pub fn scalar(value: f64) -> Self {
        Self::Constant(SemanticSignalValue::Scalar(value))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum SemanticSignalSource {
    Input(SemanticSignalValue),
    Derived(SemanticSignalExpr),
}

/// Authored native source that drives one semantic input signal.
///
/// Platform hosts collect these language-neutral sources, while lowering maps the
/// owning semantic signal onto private execution `SignalId`s. Native source identity
/// never becomes authored object identity and no platform-specific event type enters
/// the semantic scene.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SemanticNativeInputSource {
    State(NativeStateSource),
    Event(NativeEventSource),
}

#[derive(Clone, Debug, PartialEq)]
pub struct SemanticSignalState {
    source: SemanticSignalSource,
    value_kind: SemanticSignalValueKind,
    native_input: Option<SemanticNativeInputSource>,
    scalar_timeline: Vec<SemanticScalarSignalTimelineEntry>,
}

impl SemanticSignalState {
    pub(crate) const fn new(
        source: SemanticSignalSource,
        value_kind: SemanticSignalValueKind,
    ) -> Self {
        Self {
            source,
            value_kind,
            native_input: None,
            scalar_timeline: Vec::new(),
        }
    }

    pub(crate) fn input(value: SemanticSignalValue) -> Result<Self, SemanticSignalError> {
        let value_kind = validate_value(&value)?;
        Ok(Self::new(SemanticSignalSource::Input(value), value_kind))
    }

    pub const fn source(&self) -> &SemanticSignalSource {
        &self.source
    }

    pub const fn value_kind(&self) -> SemanticSignalValueKind {
        self.value_kind
    }

    pub const fn native_input(&self) -> Option<&SemanticNativeInputSource> {
        self.native_input.as_ref()
    }

    pub fn scalar_timeline(&self) -> &[SemanticScalarSignalTimelineEntry] {
        &self.scalar_timeline
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SemanticSignalError {
    UnknownSignal(SemanticNodeId),
    NotSignal(SemanticNodeId),
    NonFiniteValue,
    DependencyCycle(SemanticNodeId),
    InvalidUnaryExpression {
        operation: &'static str,
        operand: SemanticSignalValueKind,
    },
    InvalidBinaryExpression {
        operation: &'static str,
        lhs: SemanticSignalValueKind,
        rhs: SemanticSignalValueKind,
    },
    SourceTypeMismatch {
        signal: SemanticNodeId,
        expected: SemanticSignalValueKind,
        actual: SemanticSignalValueKind,
    },
    NativeInputRequiresInputSignal {
        signal: SemanticNodeId,
    },
    NativeInputTypeMismatch {
        signal: SemanticNodeId,
        expected: SemanticSignalValueKind,
        actual: SemanticSignalValueKind,
    },
    TimelineOwnedSignal {
        signal: SemanticNodeId,
    },
    NativeOwnedSignal {
        signal: SemanticNodeId,
    },
}

impl std::fmt::Display for SemanticSignalError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownSignal(id) => write!(
                formatter,
                "unknown semantic signal {}:{}",
                id.slot(),
                id.generation()
            ),
            Self::NotSignal(id) => write!(
                formatter,
                "semantic node {}:{} is not a signal",
                id.slot(),
                id.generation()
            ),
            Self::NonFiniteValue => {
                formatter.write_str("semantic signal contains a non-finite value")
            }
            Self::DependencyCycle(id) => write!(
                formatter,
                "semantic signal {}:{} source would create a dependency cycle",
                id.slot(),
                id.generation()
            ),
            Self::InvalidUnaryExpression { operation, operand } => write!(
                formatter,
                "semantic signal operation {operation} does not accept {operand}"
            ),
            Self::InvalidBinaryExpression {
                operation,
                lhs,
                rhs,
            } => write!(
                formatter,
                "semantic signal operation {operation} does not accept {lhs} and {rhs}"
            ),
            Self::SourceTypeMismatch {
                signal,
                expected,
                actual,
            } => write!(
                formatter,
                "semantic signal {}:{} has stable kind {expected}, but replacement source is {actual}",
                signal.slot(),
                signal.generation()
            ),
            Self::NativeInputRequiresInputSignal { signal } => write!(
                formatter,
                "semantic signal {}:{} must remain an input signal while a native input source is attached",
                signal.slot(),
                signal.generation()
            ),
            Self::NativeInputTypeMismatch {
                signal,
                expected,
                actual,
            } => write!(
                formatter,
                "native input source for semantic signal {}:{} requires {expected}, but the signal is {actual}",
                signal.slot(),
                signal.generation()
            ),
            Self::TimelineOwnedSignal { signal } => write!(
                formatter,
                "semantic signal {}:{} is owned by an authored timeline",
                signal.slot(),
                signal.generation()
            ),
            Self::NativeOwnedSignal { signal } => write!(
                formatter,
                "semantic signal {}:{} is owned by a native input source",
                signal.slot(),
                signal.generation()
            ),
        }
    }
}

impl std::error::Error for SemanticSignalError {}

impl SemanticStore {
    /// Insert one authored input signal into the authoritative semantic identity space.
    pub fn insert_semantic_input_signal(
        &mut self,
        value: impl Into<SemanticSignalValue>,
    ) -> Result<SemanticNodeId, SemanticSignalError> {
        self.set_last_mutation_writes(0);
        let value = value.into();
        Ok(self.insert_semantic_signal_state(SemanticSignalState::input(value)?))
    }

    /// Insert one authored derived signal after validating its referenced closure.
    ///
    /// A new signal cannot reference its not-yet-allocated identity, so creation
    /// cannot introduce a cycle. Walking the existing dependency closure rejects
    /// stale/non-signal references and infers one stable semantic result kind.
    pub fn insert_semantic_derived_signal(
        &mut self,
        expression: SemanticSignalExpr,
    ) -> Result<SemanticNodeId, SemanticSignalError> {
        self.set_last_mutation_writes(0);
        let mut cache = HashMap::new();
        let value_kind = infer_expression_kind(self, &expression, None, &mut cache)?;
        Ok(self.insert_semantic_signal_state(SemanticSignalState::new(
            SemanticSignalSource::Derived(expression),
            value_kind,
        )))
    }

    pub fn semantic_signal_state(
        &self,
        id: SemanticNodeId,
    ) -> Result<&SemanticSignalState, SemanticSignalError> {
        let node = self
            .node(id)
            .ok_or(SemanticSignalError::UnknownSignal(id))?;
        match node.kind() {
            SemanticNodeKind::Signal(state) => Ok(state),
            _ => Err(SemanticSignalError::NotSignal(id)),
        }
    }

    /// Return the stable authored value kind of one semantic signal in O(1).
    pub fn semantic_signal_value_kind(
        &self,
        id: SemanticNodeId,
    ) -> Result<SemanticSignalValueKind, SemanticSignalError> {
        Ok(self.semantic_signal_state(id)?.value_kind())
    }

    /// Evaluate one authored scalar input track at an explicit authoring time.
    /// Language facades may retain a cursor, while interpolation and track
    /// selection remain shared semantic behavior.
    pub fn semantic_input_scalar_value_at(
        &self,
        id: SemanticNodeId,
        time: f64,
    ) -> Result<f64, SemanticScalarSignalQueryError> {
        if !time.is_finite() {
            return Err(SemanticScalarSignalQueryError::InvalidTime);
        }
        let state = self.semantic_signal_state(id)?;
        let SemanticSignalSource::Input(SemanticSignalValue::Scalar(initial)) = state.source()
        else {
            return Err(
                if matches!(state.source(), SemanticSignalSource::Input(_)) {
                    SemanticScalarSignalQueryError::NonScalarSignal(id)
                } else {
                    SemanticScalarSignalQueryError::NotInputSignal(id)
                },
            );
        };
        Ok(semantic_scalar_signal_value_at(
            state.scalar_timeline(),
            *initial,
            time,
        ))
    }

    pub fn validate_semantic_scalar_signal_track(
        &self,
        track: SemanticScalarSignalTrack,
    ) -> Result<(), SemanticScalarSignalTrackError> {
        let state = self.semantic_signal_state(track.signal)?;
        self.validate_semantic_scalar_signal_entry_after(
            SemanticScalarSignalTimelineEntry::Track(track),
            state.scalar_timeline().last().copied(),
        )
    }

    pub(crate) fn validate_semantic_scalar_signal_track_after(
        &self,
        track: SemanticScalarSignalTrack,
        previous: Option<SemanticScalarSignalTimelineEntry>,
    ) -> Result<(), SemanticScalarSignalTrackError> {
        self.validate_semantic_scalar_signal_entry_after(
            SemanticScalarSignalTimelineEntry::Track(track),
            previous,
        )
    }

    pub fn validate_semantic_scalar_signal_hold(
        &self,
        hold: SemanticScalarSignalHold,
    ) -> Result<(), SemanticScalarSignalTrackError> {
        let state = self.semantic_signal_state(hold.signal)?;
        self.validate_semantic_scalar_signal_entry_after(
            SemanticScalarSignalTimelineEntry::Hold(hold),
            state.scalar_timeline().last().copied(),
        )
    }

    pub(crate) fn validate_semantic_scalar_signal_entry_after(
        &self,
        entry: SemanticScalarSignalTimelineEntry,
        previous: Option<SemanticScalarSignalTimelineEntry>,
    ) -> Result<(), SemanticScalarSignalTrackError> {
        let signal = entry.signal();
        let state = self.semantic_signal_state(signal)?;
        let SemanticSignalSource::Input(SemanticSignalValue::Scalar(initial)) = state.source()
        else {
            return Err(
                if matches!(state.source(), SemanticSignalSource::Input(_)) {
                    SemanticScalarSignalTrackError::NonScalarSignal(signal)
                } else {
                    SemanticScalarSignalTrackError::NotInputSignal(signal)
                },
            );
        };
        if state.native_input().is_some() {
            return Err(SemanticScalarSignalTrackError::NativeOwnedSignal(signal));
        }
        match entry {
            SemanticScalarSignalTimelineEntry::Track(track) => {
                validate_continuous_track_timing(track.timing)?;
                if track.timing.duration == 0.0 {
                    return Err(SemanticScalarSignalTrackError::ZeroDuration(signal));
                }
                let end_time = track.timing.start_time + track.timing.duration;
                if !end_time.is_finite() {
                    return Err(SemanticScalarSignalTrackError::NonFiniteEndTime(signal));
                }
                for value in [track.from, track.to] {
                    if !value.is_finite() {
                        return Err(SemanticScalarSignalTrackError::NonFiniteValue {
                            signal,
                            value,
                        });
                    }
                }
                let (previous_end, expected) =
                    previous.map_or((f64::NEG_INFINITY, *initial), |previous| match previous {
                        SemanticScalarSignalTimelineEntry::Track(previous) => (
                            previous.timing.start_time + previous.timing.duration,
                            previous.to,
                        ),
                        SemanticScalarSignalTimelineEntry::Hold(previous) => {
                            (previous.start_time, previous.value)
                        }
                    });
                if track.timing.start_time < previous_end {
                    return Err(SemanticScalarSignalTrackError::OverlappingTracks {
                        signal,
                        previous_end,
                        next_start: track.timing.start_time,
                    });
                }
                if track.from != expected {
                    return Err(SemanticScalarSignalTrackError::DiscontinuousTrack {
                        signal,
                        expected,
                        actual: track.from,
                    });
                }
            }
            SemanticScalarSignalTimelineEntry::Hold(hold) => {
                if !hold.value.is_finite() {
                    return Err(SemanticScalarSignalTrackError::NonFiniteValue {
                        signal,
                        value: hold.value,
                    });
                }
                if !hold.start_time.is_finite() {
                    return Err(SemanticScalarSignalTrackError::NonFiniteHoldTime(signal));
                }
                let previous_end = previous.map_or(f64::NEG_INFINITY, |previous| match previous {
                    SemanticScalarSignalTimelineEntry::Track(previous) => {
                        previous.timing.start_time + previous.timing.duration
                    }
                    SemanticScalarSignalTimelineEntry::Hold(previous) => previous.start_time,
                });
                if hold.start_time < previous_end {
                    return Err(SemanticScalarSignalTrackError::OverlappingTracks {
                        signal,
                        previous_end,
                        next_start: hold.start_time,
                    });
                }
            }
        }
        Ok(())
    }

    pub(crate) fn add_validated_semantic_scalar_signal_track(
        &mut self,
        track: SemanticScalarSignalTrack,
    ) {
        self.node_mut(track.signal)
            .and_then(|node| node.semantic_signal_state_mut())
            .expect("validated scalar signal remains live")
            .scalar_timeline
            .push(SemanticScalarSignalTimelineEntry::Track(track));
    }

    pub(crate) fn add_validated_semantic_scalar_signal_hold(
        &mut self,
        hold: SemanticScalarSignalHold,
    ) {
        self.node_mut(hold.signal)
            .and_then(|node| node.semantic_signal_state_mut())
            .expect("validated scalar signal remains live")
            .scalar_timeline
            .push(SemanticScalarSignalTimelineEntry::Hold(hold));
    }

    /// Attach or clear the language-neutral native input source for one semantic signal.
    ///
    /// The declaration lives on the authoritative signal node and therefore follows
    /// its generational identity automatically. Validation and mutation are O(1) and
    /// touch exactly that signal slot; no execution identity or scene scan is involved.
    pub fn set_semantic_native_input(
        &mut self,
        id: SemanticNodeId,
        native_input: Option<SemanticNativeInputSource>,
    ) -> Result<bool, SemanticSignalError> {
        self.set_last_mutation_writes(0);
        let state = self.semantic_signal_state(id)?;
        let previous = state.native_input().cloned();

        if let Some(native_input) = native_input.as_ref() {
            if !state.scalar_timeline().is_empty() {
                return Err(SemanticSignalError::TimelineOwnedSignal { signal: id });
            }
            if !matches!(state.source(), SemanticSignalSource::Input(_)) {
                return Err(SemanticSignalError::NativeInputRequiresInputSignal { signal: id });
            }
            let expected = native_input_signal_kind(native_input);
            let actual = state.value_kind();
            if expected != actual {
                return Err(SemanticSignalError::NativeInputTypeMismatch {
                    signal: id,
                    expected,
                    actual,
                });
            }
        }

        if previous == native_input {
            return Ok(false);
        }

        self.node_mut(id)
            .and_then(|node| node.semantic_signal_state_mut())
            .expect("semantic signal existence validated before native input mutation")
            .native_input = native_input;
        self.set_last_mutation_writes(1);
        Ok(true)
    }

    pub fn bind_semantic_native_state_input(
        &mut self,
        id: SemanticNodeId,
        source: NativeStateSource,
    ) -> Result<bool, SemanticSignalError> {
        self.set_semantic_native_input(id, Some(SemanticNativeInputSource::State(source)))
    }

    pub fn bind_semantic_native_event_input(
        &mut self,
        id: SemanticNodeId,
        source: NativeEventSource,
    ) -> Result<bool, SemanticSignalError> {
        self.set_semantic_native_input(id, Some(SemanticNativeInputSource::Event(source)))
    }

    pub fn clear_semantic_native_input(
        &mut self,
        id: SemanticNodeId,
    ) -> Result<bool, SemanticSignalError> {
        self.set_semantic_native_input(id, None)
    }

    /// Replace one signal's authored source while preserving semantic identity and kind.
    ///
    /// Validation completes before the target node is written. Work is proportional
    /// to the new source's dependency closure; unrelated scene nodes are not scanned.
    /// A successful replacement writes exactly the target signal slot, while an
    /// invalid, kind-changing, or identical replacement writes no semantic slots.
    pub fn set_semantic_signal_source(
        &mut self,
        id: SemanticNodeId,
        source: SemanticSignalSource,
    ) -> Result<bool, SemanticSignalError> {
        self.set_last_mutation_writes(0);
        let state = self.semantic_signal_state(id)?;
        let previous = state.source().clone();
        let expected = state.value_kind();
        if previous == source {
            return Ok(false);
        }
        if !state.scalar_timeline().is_empty() {
            return Err(SemanticSignalError::TimelineOwnedSignal { signal: id });
        }
        if state.native_input().is_some() && !matches!(&source, SemanticSignalSource::Input(_)) {
            return Err(SemanticSignalError::NativeInputRequiresInputSignal { signal: id });
        }
        if state.native_input().is_some() {
            return Err(SemanticSignalError::NativeOwnedSignal { signal: id });
        }

        let mut cache = HashMap::new();
        let actual = infer_source_kind(self, &source, Some(id), &mut cache)?;
        if actual != expected {
            return Err(SemanticSignalError::SourceTypeMismatch {
                signal: id,
                expected,
                actual,
            });
        }
        self.unregister_semantic_references_for_owner(id);
        self.node_mut(id)
            .and_then(|node| node.semantic_signal_state_mut())
            .expect("semantic signal existence validated before mutation")
            .source = source;
        self.register_semantic_references_for_owner(id);
        self.set_last_mutation_writes(1);
        Ok(true)
    }
}

/// Shared scalar timeline interpolation used by authored queries and lowered
/// execution. Callers select the applicable non-overlapping track.
pub fn evaluate_scalar_track(from: f64, to: f64, timing: TrackTiming, time: f64) -> f64 {
    let end = timing.start_time + timing.duration;
    if time <= timing.start_time {
        return from;
    }
    if time >= end {
        return to;
    }
    let raw = ((time - timing.start_time) / timing.duration) as f32;
    let progress = timing.easing.evaluate(raw) as f64;
    from + (to - from) * progress
}

fn semantic_scalar_signal_value_at(
    timeline: &[SemanticScalarSignalTimelineEntry],
    initial: f64,
    time: f64,
) -> f64 {
    let next = timeline.partition_point(|entry| entry.start_time() <= time);
    if next == 0 {
        return initial;
    }
    match timeline[next - 1] {
        SemanticScalarSignalTimelineEntry::Track(track) => {
            evaluate_scalar_track(track.from, track.to, track.timing, time)
        }
        SemanticScalarSignalTimelineEntry::Hold(hold) => hold.value,
    }
}

fn native_input_signal_kind(source: &SemanticNativeInputSource) -> SemanticSignalValueKind {
    match source {
        SemanticNativeInputSource::State(
            NativeStateSource::PointerPosition
            | NativeStateSource::ViewportSize
            | NativeStateSource::WheelDelta
            | NativeStateSource::GestureDelta { .. },
        ) => SemanticSignalValueKind::Vec3,
        SemanticNativeInputSource::State(
            NativeStateSource::PointerButton { .. } | NativeStateSource::Key { .. },
        ) => SemanticSignalValueKind::Bool,
        SemanticNativeInputSource::State(NativeStateSource::Control { .. })
        | SemanticNativeInputSource::Event(_) => SemanticSignalValueKind::Scalar,
    }
}

fn validate_value(
    value: &SemanticSignalValue,
) -> Result<SemanticSignalValueKind, SemanticSignalError> {
    if value.is_finite() {
        Ok(value.value_kind())
    } else {
        Err(SemanticSignalError::NonFiniteValue)
    }
}

fn infer_source_kind(
    store: &SemanticStore,
    source: &SemanticSignalSource,
    cycle_target: Option<SemanticNodeId>,
    cache: &mut HashMap<SemanticNodeId, SemanticSignalValueKind>,
) -> Result<SemanticSignalValueKind, SemanticSignalError> {
    match source {
        SemanticSignalSource::Input(value) => validate_value(value),
        SemanticSignalSource::Derived(expression) => {
            infer_expression_kind(store, expression, cycle_target, cache)
        }
    }
}

fn infer_expression_kind(
    store: &SemanticStore,
    expression: &SemanticSignalExpr,
    cycle_target: Option<SemanticNodeId>,
    cache: &mut HashMap<SemanticNodeId, SemanticSignalValueKind>,
) -> Result<SemanticSignalValueKind, SemanticSignalError> {
    match expression {
        SemanticSignalExpr::Constant(value) => validate_value(value),
        SemanticSignalExpr::Signal(id) => {
            infer_signal_dependency_kind(store, *id, cycle_target, cache)
        }
        SemanticSignalExpr::Add(lhs, rhs) => {
            let lhs = infer_expression_kind(store, lhs, cycle_target, cache)?;
            let rhs = infer_expression_kind(store, rhs, cycle_target, cache)?;
            match (lhs, rhs) {
                (SemanticSignalValueKind::Scalar, SemanticSignalValueKind::Scalar) => {
                    Ok(SemanticSignalValueKind::Scalar)
                }
                (SemanticSignalValueKind::Vec3, SemanticSignalValueKind::Vec3) => {
                    Ok(SemanticSignalValueKind::Vec3)
                }
                _ => Err(SemanticSignalError::InvalidBinaryExpression {
                    operation: "add",
                    lhs,
                    rhs,
                }),
            }
        }
        SemanticSignalExpr::Sub(lhs, rhs) => {
            let lhs = infer_expression_kind(store, lhs, cycle_target, cache)?;
            let rhs = infer_expression_kind(store, rhs, cycle_target, cache)?;
            match (lhs, rhs) {
                (SemanticSignalValueKind::Scalar, SemanticSignalValueKind::Scalar) => {
                    Ok(SemanticSignalValueKind::Scalar)
                }
                (SemanticSignalValueKind::Vec3, SemanticSignalValueKind::Vec3) => {
                    Ok(SemanticSignalValueKind::Vec3)
                }
                _ => Err(SemanticSignalError::InvalidBinaryExpression {
                    operation: "sub",
                    lhs,
                    rhs,
                }),
            }
        }
        SemanticSignalExpr::Mul(lhs, rhs) => {
            let lhs = infer_expression_kind(store, lhs, cycle_target, cache)?;
            let rhs = infer_expression_kind(store, rhs, cycle_target, cache)?;
            match (lhs, rhs) {
                (SemanticSignalValueKind::Scalar, SemanticSignalValueKind::Scalar) => {
                    Ok(SemanticSignalValueKind::Scalar)
                }
                (SemanticSignalValueKind::Scalar, SemanticSignalValueKind::Vec3)
                | (SemanticSignalValueKind::Vec3, SemanticSignalValueKind::Scalar) => {
                    Ok(SemanticSignalValueKind::Vec3)
                }
                _ => Err(SemanticSignalError::InvalidBinaryExpression {
                    operation: "mul",
                    lhs,
                    rhs,
                }),
            }
        }
        SemanticSignalExpr::Neg(value) => {
            let operand = infer_expression_kind(store, value, cycle_target, cache)?;
            match operand {
                SemanticSignalValueKind::Scalar | SemanticSignalValueKind::Vec3 => Ok(operand),
                SemanticSignalValueKind::Bool => Err(SemanticSignalError::InvalidUnaryExpression {
                    operation: "neg",
                    operand,
                }),
            }
        }
        SemanticSignalExpr::Sin(value) => {
            infer_scalar_unary_kind(store, value, cycle_target, cache, "sin")
        }
        SemanticSignalExpr::Cos(value) => {
            infer_scalar_unary_kind(store, value, cycle_target, cache, "cos")
        }
    }
}

fn infer_scalar_unary_kind(
    store: &SemanticStore,
    expression: &SemanticSignalExpr,
    cycle_target: Option<SemanticNodeId>,
    cache: &mut HashMap<SemanticNodeId, SemanticSignalValueKind>,
    operation: &'static str,
) -> Result<SemanticSignalValueKind, SemanticSignalError> {
    let operand = infer_expression_kind(store, expression, cycle_target, cache)?;
    if operand == SemanticSignalValueKind::Scalar {
        Ok(SemanticSignalValueKind::Scalar)
    } else {
        Err(SemanticSignalError::InvalidUnaryExpression { operation, operand })
    }
}

fn infer_signal_dependency_kind(
    store: &SemanticStore,
    id: SemanticNodeId,
    cycle_target: Option<SemanticNodeId>,
    cache: &mut HashMap<SemanticNodeId, SemanticSignalValueKind>,
) -> Result<SemanticSignalValueKind, SemanticSignalError> {
    if cycle_target == Some(id) {
        return Err(SemanticSignalError::DependencyCycle(id));
    }
    if let Some(value_kind) = cache.get(&id).copied() {
        return Ok(value_kind);
    }
    let state = store.semantic_signal_state(id)?;
    let value_kind = infer_source_kind(store, state.source(), cycle_target, cache)?;
    debug_assert_eq!(value_kind, state.value_kind());
    cache.insert(id, value_kind);
    Ok(value_kind)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        RateFunction, SemanticMutationTransaction, SemanticMutationTransactionError,
        SemanticObjectState, StoredGeometry,
    };

    fn object(store: &mut SemanticStore, radius: f32) -> SemanticNodeId {
        store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle { radius }))
    }

    #[test]
    fn signals_share_the_scene_global_generational_identity_space() {
        let mut store = SemanticStore::new();
        let object = object(&mut store, 1.0);
        let signal = store.insert_semantic_input_signal(1.25_f64).unwrap();

        assert_ne!(object, signal);
        assert!(matches!(
            store.semantic_signal_state(signal).unwrap().source(),
            SemanticSignalSource::Input(SemanticSignalValue::Scalar(value)) if *value == 1.25
        ));
        assert_eq!(
            store.semantic_signal_value_kind(signal).unwrap(),
            SemanticSignalValueKind::Scalar
        );
        assert_eq!(store.last_mutation_stats().slots_written, 1);
        assert_eq!(store.scene_root_count(), 0);
    }

    #[test]
    fn derived_signal_references_semantic_node_identity() {
        let mut store = SemanticStore::new();
        let input = store.insert_semantic_input_signal(2.0_f64).unwrap();
        let derived = store
            .insert_semantic_derived_signal(SemanticSignalExpr::Add(
                Box::new(SemanticSignalExpr::signal(input)),
                Box::new(SemanticSignalExpr::scalar(3.0)),
            ))
            .unwrap();

        assert!(matches!(
            store.semantic_signal_state(derived).unwrap().source(),
            SemanticSignalSource::Derived(_)
        ));
        assert_eq!(
            store.semantic_signal_value_kind(derived).unwrap(),
            SemanticSignalValueKind::Scalar
        );
        assert_eq!(store.last_mutation_stats().slots_written, 1);
    }

    #[test]
    fn semantic_native_input_declaration_is_signal_owned_typed_and_local() {
        let mut store = SemanticStore::new();
        for index in 0..10_000 {
            object(&mut store, index as f32 + 1.0);
        }
        let key = store.insert_semantic_input_signal(false).unwrap();
        let viewport = store
            .insert_semantic_input_signal(SemanticVec3::new(0.0, 0.0, 0.0))
            .unwrap();
        let event = store.insert_semantic_input_signal(0.0_f64).unwrap();

        let key_source = NativeStateSource::Key {
            code: "Space".to_owned(),
        };
        assert!(store
            .bind_semantic_native_state_input(key, key_source.clone())
            .unwrap());
        assert_eq!(store.last_mutation_stats().slots_written, 1);
        assert_eq!(
            store.semantic_signal_state(key).unwrap().native_input(),
            Some(&SemanticNativeInputSource::State(key_source))
        );

        assert!(store
            .bind_semantic_native_state_input(viewport, NativeStateSource::ViewportSize)
            .unwrap());
        assert!(store
            .bind_semantic_native_event_input(
                event,
                NativeEventSource::KeyPress {
                    code: "Space".to_owned(),
                },
            )
            .unwrap());
        assert_eq!(store.last_mutation_stats().slots_written, 1);

        assert!(store.clear_semantic_native_input(key).unwrap());
        assert_eq!(store.last_mutation_stats().slots_written, 1);
        assert_eq!(
            store.semantic_signal_state(key).unwrap().native_input(),
            None
        );
        assert!(!store.clear_semantic_native_input(key).unwrap());
        assert_eq!(store.last_mutation_stats().slots_written, 0);
    }

    #[test]
    fn semantic_native_input_rejects_derived_and_mismatched_signals_atomically() {
        let mut store = SemanticStore::new();
        let scalar = store.insert_semantic_input_signal(0.0_f64).unwrap();
        let boolean = store.insert_semantic_input_signal(false).unwrap();
        let derived = store
            .insert_semantic_derived_signal(SemanticSignalExpr::signal(scalar))
            .unwrap();

        assert_eq!(
            store.bind_semantic_native_state_input(
                scalar,
                NativeStateSource::Key {
                    code: "KeyA".to_owned(),
                },
            ),
            Err(SemanticSignalError::NativeInputTypeMismatch {
                signal: scalar,
                expected: SemanticSignalValueKind::Bool,
                actual: SemanticSignalValueKind::Scalar,
            })
        );
        assert_eq!(store.last_mutation_stats().slots_written, 0);

        assert_eq!(
            store.bind_semantic_native_event_input(
                boolean,
                NativeEventSource::KeyPress {
                    code: "KeyA".to_owned(),
                },
            ),
            Err(SemanticSignalError::NativeInputTypeMismatch {
                signal: boolean,
                expected: SemanticSignalValueKind::Scalar,
                actual: SemanticSignalValueKind::Bool,
            })
        );
        assert_eq!(store.last_mutation_stats().slots_written, 0);

        assert_eq!(
            store.bind_semantic_native_state_input(
                derived,
                NativeStateSource::Control {
                    name: "zoom".to_owned(),
                },
            ),
            Err(SemanticSignalError::NativeInputRequiresInputSignal { signal: derived })
        );
        assert_eq!(store.last_mutation_stats().slots_written, 0);
    }

    #[test]
    fn attached_native_input_prevents_silent_conversion_to_a_derived_signal() {
        let mut store = SemanticStore::new();
        let dependency = store.insert_semantic_input_signal(1.0_f64).unwrap();
        let target = store.insert_semantic_input_signal(0.0_f64).unwrap();
        store
            .bind_semantic_native_state_input(
                target,
                NativeStateSource::Control {
                    name: "zoom".to_owned(),
                },
            )
            .unwrap();
        let before = store
            .semantic_signal_state(target)
            .unwrap()
            .source()
            .clone();

        assert_eq!(
            store.set_semantic_signal_source(
                target,
                SemanticSignalSource::Derived(SemanticSignalExpr::signal(dependency)),
            ),
            Err(SemanticSignalError::NativeInputRequiresInputSignal { signal: target })
        );
        assert_eq!(
            store.semantic_signal_state(target).unwrap().source(),
            &before
        );
        assert_eq!(store.last_mutation_stats().slots_written, 0);

        store.clear_semantic_native_input(target).unwrap();
        assert!(store
            .set_semantic_signal_source(
                target,
                SemanticSignalSource::Derived(SemanticSignalExpr::signal(dependency)),
            )
            .unwrap());
    }

    #[test]
    fn semantic_expression_kinds_match_the_native_reactive_operator_contract() {
        let mut store = SemanticStore::new();
        let scalar = store.insert_semantic_input_signal(2.0_f64).unwrap();
        let vector = store
            .insert_semantic_input_signal(SemanticVec3::new(1.0, 2.0, 3.0))
            .unwrap();
        let boolean = store.insert_semantic_input_signal(true).unwrap();

        assert_eq!(
            store.semantic_signal_value_kind(boolean).unwrap(),
            SemanticSignalValueKind::Bool
        );

        let vector_sum = store
            .insert_semantic_derived_signal(SemanticSignalExpr::Add(
                Box::new(SemanticSignalExpr::signal(vector)),
                Box::new(SemanticSignalExpr::Constant(SemanticSignalValue::Vec3(
                    SemanticVec3::new(4.0, 5.0, 6.0),
                ))),
            ))
            .unwrap();
        assert_eq!(
            store.semantic_signal_value_kind(vector_sum).unwrap(),
            SemanticSignalValueKind::Vec3
        );

        let scaled = store
            .insert_semantic_derived_signal(SemanticSignalExpr::Mul(
                Box::new(SemanticSignalExpr::signal(scalar)),
                Box::new(SemanticSignalExpr::signal(vector)),
            ))
            .unwrap();
        assert_eq!(
            store.semantic_signal_value_kind(scaled).unwrap(),
            SemanticSignalValueKind::Vec3
        );

        let negated = store
            .insert_semantic_derived_signal(SemanticSignalExpr::Neg(Box::new(
                SemanticSignalExpr::signal(vector),
            )))
            .unwrap();
        assert_eq!(
            store.semantic_signal_value_kind(negated).unwrap(),
            SemanticSignalValueKind::Vec3
        );

        let sine = store
            .insert_semantic_derived_signal(SemanticSignalExpr::Sin(Box::new(
                SemanticSignalExpr::signal(scalar),
            )))
            .unwrap();
        assert_eq!(
            store.semantic_signal_value_kind(sine).unwrap(),
            SemanticSignalValueKind::Scalar
        );
    }

    #[test]
    fn invalid_semantic_expression_kinds_are_rejected_before_insertion() {
        let mut store = SemanticStore::new();
        let scalar = store.insert_semantic_input_signal(1.0_f64).unwrap();
        let vector = store
            .insert_semantic_input_signal(SemanticVec3::new(1.0, 2.0, 3.0))
            .unwrap();
        let boolean = store.insert_semantic_input_signal(true).unwrap();
        let before = store.len();

        assert_eq!(
            store.insert_semantic_derived_signal(SemanticSignalExpr::Add(
                Box::new(SemanticSignalExpr::signal(boolean)),
                Box::new(SemanticSignalExpr::signal(boolean)),
            )),
            Err(SemanticSignalError::InvalidBinaryExpression {
                operation: "add",
                lhs: SemanticSignalValueKind::Bool,
                rhs: SemanticSignalValueKind::Bool,
            })
        );
        assert_eq!(store.len(), before);
        assert_eq!(store.last_mutation_stats().slots_written, 0);

        assert_eq!(
            store.insert_semantic_derived_signal(SemanticSignalExpr::Add(
                Box::new(SemanticSignalExpr::signal(scalar)),
                Box::new(SemanticSignalExpr::signal(vector)),
            )),
            Err(SemanticSignalError::InvalidBinaryExpression {
                operation: "add",
                lhs: SemanticSignalValueKind::Scalar,
                rhs: SemanticSignalValueKind::Vec3,
            })
        );
        assert_eq!(store.len(), before);
        assert_eq!(store.last_mutation_stats().slots_written, 0);

        assert_eq!(
            store.insert_semantic_derived_signal(SemanticSignalExpr::Mul(
                Box::new(SemanticSignalExpr::signal(vector)),
                Box::new(SemanticSignalExpr::signal(vector)),
            )),
            Err(SemanticSignalError::InvalidBinaryExpression {
                operation: "mul",
                lhs: SemanticSignalValueKind::Vec3,
                rhs: SemanticSignalValueKind::Vec3,
            })
        );
        assert_eq!(store.len(), before);
        assert_eq!(store.last_mutation_stats().slots_written, 0);

        assert_eq!(
            store.insert_semantic_derived_signal(SemanticSignalExpr::Sin(Box::new(
                SemanticSignalExpr::signal(vector),
            ))),
            Err(SemanticSignalError::InvalidUnaryExpression {
                operation: "sin",
                operand: SemanticSignalValueKind::Vec3,
            })
        );
        assert_eq!(store.len(), before);
        assert_eq!(store.last_mutation_stats().slots_written, 0);
    }

    #[test]
    fn signal_validation_rejects_stale_and_non_signal_dependencies_before_insertion() {
        let mut store = SemanticStore::new();
        let stale = store.insert_semantic_input_signal(1.0_f64).unwrap();
        store.remove_node(stale).unwrap();
        let replacement = object(&mut store, 2.0);
        assert_eq!(stale.slot(), replacement.slot());
        assert_ne!(stale.generation(), replacement.generation());

        let before = store.len();
        assert_eq!(
            store.insert_semantic_derived_signal(SemanticSignalExpr::signal(stale)),
            Err(SemanticSignalError::UnknownSignal(stale))
        );
        assert_eq!(store.len(), before);
        assert_eq!(store.last_mutation_stats().slots_written, 0);

        assert_eq!(
            store.insert_semantic_derived_signal(SemanticSignalExpr::signal(replacement)),
            Err(SemanticSignalError::NotSignal(replacement))
        );
        assert_eq!(store.len(), before);
        assert_eq!(store.last_mutation_stats().slots_written, 0);
    }

    #[test]
    fn signal_creation_rejects_stale_transitive_dependency_closure() {
        let mut store = SemanticStore::new();
        let input = store.insert_semantic_input_signal(1.0_f64).unwrap();
        let derived = store
            .insert_semantic_derived_signal(SemanticSignalExpr::signal(input))
            .unwrap();
        store.remove_node(input).unwrap();

        let before = store.len();
        assert_eq!(
            store.insert_semantic_derived_signal(SemanticSignalExpr::signal(derived)),
            Err(SemanticSignalError::UnknownSignal(input))
        );
        assert_eq!(store.len(), before);
        assert_eq!(store.last_mutation_stats().slots_written, 0);
    }

    #[test]
    fn non_finite_signal_values_are_rejected_without_mutation() {
        let mut store = SemanticStore::new();
        assert_eq!(
            store.insert_semantic_input_signal(f64::NAN),
            Err(SemanticSignalError::NonFiniteValue)
        );
        assert_eq!(store.len(), 0);
        assert_eq!(store.last_mutation_stats().slots_written, 0);
    }

    #[test]
    fn signal_creation_cost_does_not_scale_with_unrelated_scene_nodes() {
        let mut store = SemanticStore::new();
        for index in 0..10_000 {
            object(&mut store, index as f32 + 1.0);
        }
        let input = store
            .insert_semantic_input_signal(SemanticVec3::new(1.0, 2.0, 3.0))
            .unwrap();
        assert_eq!(store.last_mutation_stats().slots_written, 1);

        let derived = store
            .insert_semantic_derived_signal(SemanticSignalExpr::Neg(Box::new(
                SemanticSignalExpr::signal(input),
            )))
            .unwrap();
        assert_eq!(store.last_mutation_stats().slots_written, 1);
        assert_eq!(
            store.semantic_signal_value_kind(derived).unwrap(),
            SemanticSignalValueKind::Vec3
        );
    }

    #[test]
    fn signal_source_replacement_preserves_identity_kind_and_one_slot_locality() {
        let mut store = SemanticStore::new();
        for index in 0..10_000 {
            object(&mut store, index as f32 + 1.0);
        }
        let dependency = store.insert_semantic_input_signal(2.0_f64).unwrap();
        let target = store.insert_semantic_input_signal(1.0_f64).unwrap();
        let source = SemanticSignalSource::Derived(SemanticSignalExpr::Add(
            Box::new(SemanticSignalExpr::signal(dependency)),
            Box::new(SemanticSignalExpr::scalar(3.0)),
        ));
        let before_len = store.len();

        assert!(store
            .set_semantic_signal_source(target, source.clone())
            .unwrap());
        assert_eq!(store.node(target).unwrap().id(), target);
        assert_eq!(
            store.semantic_signal_state(target).unwrap().source(),
            &source
        );
        assert_eq!(
            store.semantic_signal_value_kind(target).unwrap(),
            SemanticSignalValueKind::Scalar
        );
        assert_eq!(store.len(), before_len);
        assert_eq!(store.last_mutation_stats().slots_written, 1);

        assert!(!store
            .set_semantic_signal_source(target, source.clone())
            .unwrap());
        assert_eq!(
            store.semantic_signal_state(target).unwrap().source(),
            &source
        );
        assert_eq!(store.last_mutation_stats().slots_written, 0);
    }

    #[test]
    fn signal_source_replacement_cannot_change_the_signal_value_kind() {
        let mut store = SemanticStore::new();
        let target = store.insert_semantic_input_signal(1.0_f64).unwrap();
        let vector = store
            .insert_semantic_input_signal(SemanticVec3::new(1.0, 2.0, 3.0))
            .unwrap();
        let before = store
            .semantic_signal_state(target)
            .unwrap()
            .source()
            .clone();

        assert_eq!(
            store.set_semantic_signal_source(
                target,
                SemanticSignalSource::Derived(SemanticSignalExpr::signal(vector)),
            ),
            Err(SemanticSignalError::SourceTypeMismatch {
                signal: target,
                expected: SemanticSignalValueKind::Scalar,
                actual: SemanticSignalValueKind::Vec3,
            })
        );
        assert_eq!(
            store.semantic_signal_state(target).unwrap().source(),
            &before
        );
        assert_eq!(
            store.semantic_signal_value_kind(target).unwrap(),
            SemanticSignalValueKind::Scalar
        );
        assert_eq!(store.last_mutation_stats().slots_written, 0);
    }

    #[test]
    fn stable_signal_kind_allows_recovery_from_a_stale_dependency() {
        let mut store = SemanticStore::new();
        let dependency = store.insert_semantic_input_signal(1.0_f64).unwrap();
        let target = store
            .insert_semantic_derived_signal(SemanticSignalExpr::signal(dependency))
            .unwrap();
        store.remove_node(dependency).unwrap();

        assert_eq!(
            store.semantic_signal_value_kind(target).unwrap(),
            SemanticSignalValueKind::Scalar
        );
        assert!(store
            .set_semantic_signal_source(
                target,
                SemanticSignalSource::Input(SemanticSignalValue::Scalar(7.0)),
            )
            .unwrap());
        assert_eq!(store.last_mutation_stats().slots_written, 1);
        assert!(matches!(
            store.semantic_signal_state(target).unwrap().source(),
            SemanticSignalSource::Input(SemanticSignalValue::Scalar(value)) if *value == 7.0
        ));
    }

    #[test]
    fn signal_source_replacement_rejects_direct_and_indirect_cycles_atomically() {
        let mut store = SemanticStore::new();
        let a = store.insert_semantic_input_signal(1.0_f64).unwrap();
        let b = store
            .insert_semantic_derived_signal(SemanticSignalExpr::signal(a))
            .unwrap();
        let c = store
            .insert_semantic_derived_signal(SemanticSignalExpr::signal(b))
            .unwrap();
        let a_before = store.semantic_signal_state(a).unwrap().source().clone();
        let b_before = store.semantic_signal_state(b).unwrap().source().clone();

        assert_eq!(
            store.set_semantic_signal_source(
                a,
                SemanticSignalSource::Derived(SemanticSignalExpr::signal(c)),
            ),
            Err(SemanticSignalError::DependencyCycle(a))
        );
        assert_eq!(store.semantic_signal_state(a).unwrap().source(), &a_before);
        assert_eq!(store.last_mutation_stats().slots_written, 0);

        assert_eq!(
            store.set_semantic_signal_source(
                b,
                SemanticSignalSource::Derived(SemanticSignalExpr::signal(b)),
            ),
            Err(SemanticSignalError::DependencyCycle(b))
        );
        assert_eq!(store.semantic_signal_state(b).unwrap().source(), &b_before);
        assert_eq!(store.last_mutation_stats().slots_written, 0);
    }

    #[test]
    fn signal_source_replacement_rejects_invalid_dependency_closure_atomically() {
        let mut store = SemanticStore::new();
        let stale = store.insert_semantic_input_signal(1.0_f64).unwrap();
        let transitive = store
            .insert_semantic_derived_signal(SemanticSignalExpr::signal(stale))
            .unwrap();
        let target = store.insert_semantic_input_signal(5.0_f64).unwrap();
        let target_before = store
            .semantic_signal_state(target)
            .unwrap()
            .source()
            .clone();
        store.remove_node(stale).unwrap();

        assert_eq!(
            store.set_semantic_signal_source(
                target,
                SemanticSignalSource::Derived(SemanticSignalExpr::signal(transitive)),
            ),
            Err(SemanticSignalError::UnknownSignal(stale))
        );
        assert_eq!(
            store.semantic_signal_state(target).unwrap().source(),
            &target_before
        );
        assert_eq!(store.last_mutation_stats().slots_written, 0);

        let non_signal = object(&mut store, 2.0);
        assert_eq!(
            store.set_semantic_signal_source(
                target,
                SemanticSignalSource::Derived(SemanticSignalExpr::signal(non_signal)),
            ),
            Err(SemanticSignalError::NotSignal(non_signal))
        );
        assert_eq!(
            store.semantic_signal_state(target).unwrap().source(),
            &target_before
        );
        assert_eq!(store.last_mutation_stats().slots_written, 0);

        assert_eq!(
            store.set_semantic_signal_source(
                target,
                SemanticSignalSource::Input(SemanticSignalValue::Scalar(f64::NAN)),
            ),
            Err(SemanticSignalError::NonFiniteValue)
        );
        assert_eq!(
            store.semantic_signal_state(target).unwrap().source(),
            &target_before
        );
        assert_eq!(store.last_mutation_stats().slots_written, 0);
    }

    #[test]
    fn signal_nodes_are_not_scene_family_or_updater_targets() {
        let mut store = SemanticStore::new();
        let signal = store.insert_semantic_input_signal(true).unwrap();
        let family = store.insert_family();

        assert!(matches!(
            store.add_semantic_scene_nodes(&[signal]),
            Err(super::super::SemanticSceneOperationError::NotSemanticAuthoringNode(id)) if id == signal
        ));
        assert!(matches!(
            store.add_semantic_family_member(family, signal),
            Err(super::super::SemanticSceneOperationError::NotSemanticAuthoringNode(id)) if id == signal
        ));
        let mut transaction = super::super::SemanticMutationTransaction::new();
        transaction.add_updater(signal, super::super::HostCallbackId::new(1), 0.0, None);
        assert!(matches!(
            transaction.apply(&mut store),
            Err(super::super::SemanticMutationTransactionError::Family {
                error: super::super::SemanticSceneOperationError::NotSemanticAuthoringNode(id),
                ..
            }) if id == signal
        ));
    }

    #[test]
    fn native_owned_signal_rejects_direct_writes_before_transaction_commit() {
        let mut store = SemanticStore::new();
        let other = store.insert_semantic_input_signal(1.0_f64).unwrap();
        let native = store.insert_semantic_input_signal(0.0_f64).unwrap();
        store
            .bind_semantic_native_state_input(
                native,
                NativeStateSource::Control {
                    name: "opacity".into(),
                },
            )
            .unwrap();
        let revision = store.scene_revision();
        let mut transaction = SemanticMutationTransaction::new();
        transaction
            .set_signal(other, 2.0_f64)
            .set_signal(native, 0.5_f64);
        assert!(matches!(transaction.apply(&mut store),
            Err(SemanticMutationTransactionError::Signal {
                error: SemanticSignalError::NativeOwnedSignal { signal }, ..
            }) if signal == native
        ));
        assert_eq!(store.scene_revision(), revision);
        assert_eq!(store.semantic_input_scalar_value_at(other, 0.0), Ok(1.0));
        assert_eq!(store.semantic_input_scalar_value_at(native, 0.0), Ok(0.0));
        assert_eq!(
            store.set_semantic_signal_source(native, SemanticSignalSource::Input(0.5_f64.into())),
            Err(SemanticSignalError::NativeOwnedSignal { signal: native })
        );
        store.clear_semantic_native_input(native).unwrap();
        let mut transaction = SemanticMutationTransaction::new();
        transaction
            .set_signal(other, 2.0_f64)
            .set_signal(native, 0.5_f64);
        transaction.apply(&mut store).unwrap();
        assert_eq!(store.semantic_input_scalar_value_at(native, 0.0), Ok(0.5));
    }

    #[test]
    fn scalar_tracks_validate_continuity_ownership_and_commit_atomically() {
        let mut store = SemanticStore::new();
        let signal = store.insert_semantic_input_signal(0.0_f64).unwrap();
        let timing = TrackTiming::new(0.0, 2.0, RateFunction::Linear);
        let mut transaction = SemanticMutationTransaction::new();
        transaction.add_scalar_signal_track(signal, 0.0, 4.0, timing);
        transaction.apply(&mut store).unwrap();
        assert_eq!(store.semantic_input_scalar_value_at(signal, 1.0), Ok(2.0));

        let revision = store.scene_revision();
        let mut invalid = SemanticMutationTransaction::new();
        invalid.add_scalar_signal_track(
            signal,
            3.0,
            8.0,
            TrackTiming::new(2.0, 1.0, RateFunction::Linear),
        );
        assert!(matches!(
            invalid.apply(&mut store),
            Err(SemanticMutationTransactionError::SignalTrack {
                error: SemanticScalarSignalTrackError::DiscontinuousTrack { .. },
                ..
            })
        ));
        assert_eq!(store.scene_revision(), revision);
        assert_eq!(
            store
                .semantic_signal_state(signal)
                .unwrap()
                .scalar_timeline()
                .len(),
            1
        );

        assert_eq!(
            store.bind_semantic_native_state_input(signal, NativeStateSource::PointerPosition),
            Err(SemanticSignalError::TimelineOwnedSignal { signal })
        );

        let other = store.insert_semantic_input_signal(1.0_f64).unwrap();
        let revision = store.scene_revision();
        let mut invalid_end = SemanticMutationTransaction::new();
        invalid_end
            .set_signal(other, 2.0_f64)
            .add_scalar_signal_track(
                signal,
                4.0,
                8.0,
                TrackTiming::new(f64::MAX, f64::MAX, RateFunction::Linear),
            );
        assert!(matches!(
            invalid_end.apply(&mut store),
            Err(SemanticMutationTransactionError::SignalTrack {
                error: SemanticScalarSignalTrackError::NonFiniteEndTime(id),
                ..
            }) if id == signal
        ));
        assert_eq!(store.scene_revision(), revision);
        assert_eq!(store.semantic_input_scalar_value_at(other, 0.0), Ok(1.0));
    }

    #[test]
    fn scalar_holds_release_track_ownership_without_erasing_history() {
        let mut store = SemanticStore::new();
        let signal = store.insert_semantic_input_signal(0.0_f64).unwrap();
        let unrelated = store.insert_semantic_input_signal(1.0_f64).unwrap();
        let mut track = SemanticMutationTransaction::new();
        track.add_scalar_signal_track(
            signal,
            0.0,
            2.0,
            TrackTiming::new(0.0, 2.0, RateFunction::Linear),
        );
        track.apply(&mut store).unwrap();

        let revision = store.scene_revision();
        let mut overlapping = SemanticMutationTransaction::new();
        overlapping
            .set_signal(unrelated, 9.0_f64)
            .set_scalar_signal_at(signal, 3.0, 1.0);
        assert!(matches!(
            overlapping.apply(&mut store),
            Err(SemanticMutationTransactionError::SignalTrack {
                error: SemanticScalarSignalTrackError::OverlappingTracks { .. },
                ..
            })
        ));
        assert_eq!(store.scene_revision(), revision);
        assert_eq!(
            store.semantic_input_scalar_value_at(unrelated, 1.0),
            Ok(1.0)
        );

        let mut release = SemanticMutationTransaction::new();
        release.set_scalar_signal_at(signal, 2.0, 2.0);
        release.apply(&mut store).unwrap();
        let mut resumed = SemanticMutationTransaction::new();
        resumed.add_scalar_signal_track(
            signal,
            2.0,
            4.0,
            TrackTiming::new(3.0, 1.0, RateFunction::Linear),
        );
        resumed.apply(&mut store).unwrap();

        assert_eq!(store.semantic_input_scalar_value_at(signal, 1.0), Ok(1.0));
        assert_eq!(store.semantic_input_scalar_value_at(signal, 2.5), Ok(2.0));
        assert_eq!(store.semantic_input_scalar_value_at(signal, 3.5), Ok(3.0));
        assert_eq!(store.semantic_input_scalar_value_at(signal, 4.0), Ok(4.0));
        assert!(matches!(
            store
                .semantic_signal_state(signal)
                .unwrap()
                .scalar_timeline(),
            [
                SemanticScalarSignalTimelineEntry::Track(_),
                SemanticScalarSignalTimelineEntry::Hold(_),
                SemanticScalarSignalTimelineEntry::Track(_)
            ]
        ));
    }
}
