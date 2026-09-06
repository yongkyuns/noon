use std::collections::{HashMap, HashSet};

use noon_core::{
    NativeEventSource, NativeStateSource, PreparedSemanticMutationTransaction, Property,
    ReactiveError, ReactiveExpr, ReactiveGraphDefinition, ReactiveValue, SemanticMutation,
    SemanticNativeInputSource, SemanticNodeId, SemanticObjectProperty, SemanticScalarSignalHold,
    SemanticScalarSignalTimelineEntry, SemanticScalarSignalTrack, SemanticSignalError,
    SemanticSignalExpr, SemanticSignalSource, SemanticSignalValue, SemanticStore, SignalDefinition,
    SignalId, SignalSource,
};

use super::SemanticExecutionProjection;

/// Execution-facing native-reactive projection derived from authoritative semantic
/// signal identity and object bindings.
///
/// This is not a second authored graph. `SignalId` values are deterministic
/// compatibility encodings used only by the existing native reactive compiler/VM;
/// semantic `SemanticNodeId` remains authoritative. Native input routing is derived
/// from signal-owned semantic declarations while the same dependency closure is
/// lowered, so platform hosts never need execution signal identity.
#[derive(Clone, Debug, PartialEq)]
pub struct SemanticReactiveProjection {
    graph: ReactiveGraphDefinition,
    signal_ids: HashMap<SemanticNodeId, SignalId>,
    native_state_targets: HashMap<NativeStateSource, Vec<SignalId>>,
    native_event_targets: HashMap<NativeEventSource, Vec<SignalId>>,
    native_signals: HashSet<SemanticNodeId>,
    scalar_timeline: Vec<CompiledScalarSignalTimelineEntry>,
    timeline_signals: HashSet<SemanticNodeId>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CompiledScalarSignalTrack {
    semantic_signal: SemanticNodeId,
    execution_signal: SignalId,
    from: f32,
    to: f32,
    timing: noon_core::TrackTiming,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CompiledScalarSignalHold {
    semantic_signal: SemanticNodeId,
    execution_signal: SignalId,
    value: f32,
    start_time: f64,
}

impl CompiledScalarSignalHold {
    pub const fn semantic_signal(self) -> SemanticNodeId {
        self.semantic_signal
    }

    pub const fn execution_signal(self) -> SignalId {
        self.execution_signal
    }

    pub const fn value(self) -> f32 {
        self.value
    }

    pub const fn start_time(self) -> f64 {
        self.start_time
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CompiledScalarSignalTimelineEntry {
    Track(CompiledScalarSignalTrack),
    Hold(CompiledScalarSignalHold),
}

impl CompiledScalarSignalTimelineEntry {
    pub const fn semantic_signal(self) -> SemanticNodeId {
        match self {
            Self::Track(track) => track.semantic_signal(),
            Self::Hold(hold) => hold.semantic_signal(),
        }
    }

    pub const fn execution_signal(self) -> SignalId {
        match self {
            Self::Track(track) => track.execution_signal(),
            Self::Hold(hold) => hold.execution_signal(),
        }
    }

    pub const fn start_time(self) -> f64 {
        match self {
            Self::Track(track) => track.timing().start_time,
            Self::Hold(hold) => hold.start_time(),
        }
    }
}

impl CompiledScalarSignalTrack {
    pub const fn semantic_signal(self) -> SemanticNodeId {
        self.semantic_signal
    }

    pub const fn execution_signal(self) -> SignalId {
        self.execution_signal
    }

    pub const fn from(self) -> f32 {
        self.from
    }

    pub const fn to(self) -> f32 {
        self.to
    }

    pub const fn timing(self) -> noon_core::TrackTiming {
        self.timing
    }
}

impl SemanticReactiveProjection {
    pub fn graph(&self) -> &ReactiveGraphDefinition {
        &self.graph
    }

    pub fn execution_signal_id(&self, semantic_id: SemanticNodeId) -> Option<SignalId> {
        self.signal_ids.get(&semantic_id).copied()
    }

    /// Execution targets for one normalized sampled native source.
    ///
    /// This is an execution-plan detail consumed by `ExecutionSession`; public
    /// platform hosts continue to operate only on `NativeStateSource` values.
    pub fn native_state_targets(&self, source: &NativeStateSource) -> &[SignalId] {
        self.native_state_targets
            .get(source)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// Execution targets for one normalized discrete native event source.
    pub fn native_event_targets(&self, source: &NativeEventSource) -> &[SignalId] {
        self.native_event_targets
            .get(source)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// Whether a lowered signal is owned by a native source rather than direct writes.
    pub fn is_native_owned(&self, signal: SemanticNodeId) -> bool {
        self.native_signals.contains(&signal)
    }

    pub fn signal_count(&self) -> usize {
        self.signal_ids.len()
    }

    pub fn scalar_timeline(&self) -> &[CompiledScalarSignalTimelineEntry] {
        &self.scalar_timeline
    }

    /// Move scalar timeline entries into the runtime-owned derived event index while
    /// retaining semantic/execution signal mappings in this projection.
    pub fn take_scalar_timeline(&mut self) -> Vec<CompiledScalarSignalTimelineEntry> {
        std::mem::take(&mut self.scalar_timeline)
    }

    pub fn timeline_owns(&self, semantic_signal: SemanticNodeId) -> bool {
        self.timeline_signals.contains(&semantic_signal)
    }

    pub fn is_empty(&self) -> bool {
        self.signal_ids.is_empty() && self.graph.bindings().is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SemanticReactiveLoweringError {
    Signal(SemanticSignalError),
    Reactive(ReactiveError),
    NonFiniteSignalValue {
        signal: SemanticNodeId,
    },
    SignalValueOutOfRange {
        signal: SemanticNodeId,
    },
    UnsupportedProperty {
        target: SemanticNodeId,
        property: SemanticObjectProperty,
    },
    DependencyCycle(SemanticNodeId),
}

#[derive(Clone, Debug, PartialEq)]
pub enum PreparedScalarSignalTimelineError {
    ExpectedSingleEntry,
    UnsupportedMutation { index: usize },
    UnknownExecutionSignal(SemanticNodeId),
    Lowering(SemanticReactiveLoweringError),
}

impl std::fmt::Display for PreparedScalarSignalTimelineError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ExpectedSingleEntry => formatter
                .write_str("prepared scalar publication requires exactly one timeline entry"),
            Self::UnsupportedMutation { index } => write!(
                formatter,
                "prepared scalar publication does not support semantic mutation {index}"
            ),
            Self::UnknownExecutionSignal(signal) => write!(
                formatter,
                "semantic signal {}:{} is not lowered into this execution session",
                signal.slot(),
                signal.generation()
            ),
            Self::Lowering(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for PreparedScalarSignalTimelineError {}

impl From<SemanticReactiveLoweringError> for PreparedScalarSignalTimelineError {
    fn from(value: SemanticReactiveLoweringError) -> Self {
        Self::Lowering(value)
    }
}

/// Lower exactly one already-preflighted scalar timeline entry without rebuilding the graph.
pub fn lower_prepared_scalar_signal_timeline_entry(
    prepared: &PreparedSemanticMutationTransaction<'_>,
    projection: &SemanticReactiveProjection,
) -> Result<CompiledScalarSignalTimelineEntry, PreparedScalarSignalTimelineError> {
    let mut lowered = None;
    for (index, mutation) in prepared.candidate_mutations().enumerate() {
        let entry = match mutation {
            SemanticMutation::AddScalarSignalTrack {
                signal,
                from,
                to,
                timing,
            } => SemanticScalarSignalTimelineEntry::Track(SemanticScalarSignalTrack::new(
                *signal, *from, *to, *timing,
            )),
            SemanticMutation::SetScalarSignalAt {
                signal,
                value,
                time,
            } => SemanticScalarSignalTimelineEntry::Hold(SemanticScalarSignalHold::new(
                *signal, *value, *time,
            )),
            _ => return Err(PreparedScalarSignalTimelineError::UnsupportedMutation { index }),
        };
        if lowered.is_some() {
            return Err(PreparedScalarSignalTimelineError::ExpectedSingleEntry);
        }
        let execution_signal = projection.execution_signal_id(entry.signal()).ok_or(
            PreparedScalarSignalTimelineError::UnknownExecutionSignal(entry.signal()),
        )?;
        lowered = Some(lower_scalar_timeline_entry(entry, execution_signal)?);
    }
    lowered.ok_or(PreparedScalarSignalTimelineError::ExpectedSingleEntry)
}

impl From<SemanticSignalError> for SemanticReactiveLoweringError {
    fn from(value: SemanticSignalError) -> Self {
        Self::Signal(value)
    }
}

impl From<ReactiveError> for SemanticReactiveLoweringError {
    fn from(value: ReactiveError) -> Self {
        Self::Reactive(value)
    }
}

impl std::fmt::Display for SemanticReactiveLoweringError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Signal(error) => error.fmt(formatter),
            Self::Reactive(error) => error.fmt(formatter),
            Self::NonFiniteSignalValue { signal } => write!(
                formatter,
                "semantic signal {}:{} contains a non-finite execution value",
                signal.slot(),
                signal.generation()
            ),
            Self::SignalValueOutOfRange { signal } => write!(
                formatter,
                "semantic signal {}:{} cannot lower to the current f32 execution domain",
                signal.slot(),
                signal.generation()
            ),
            Self::UnsupportedProperty { target, property } => write!(
                formatter,
                "semantic property {property:?} on object {}:{} has no native reactive execution property yet",
                target.slot(),
                target.generation()
            ),
            Self::DependencyCycle(signal) => write!(
                formatter,
                "semantic signal {}:{} contains a dependency cycle during execution lowering",
                signal.slot(),
                signal.generation()
            ),
        }
    }
}

impl std::error::Error for SemanticReactiveLoweringError {}

/// Lower only the native-reactive dependency closure reachable from visible object
/// bindings in one semantic execution projection.
///
/// Unbound semantic signals remain authored scene state but do not consume execution
/// VM slots. Dependencies are generation-checked through `SemanticStore`, and the
/// resulting `ReactiveGraphDefinition` is the existing native execution graph rather
/// than a new reactive runtime model. Native input routes are captured only for the
/// same lowered dependency closure, avoiding a second total-scene discovery pass.
pub fn lower_semantic_reactive_projection(
    store: &SemanticStore,
    projection: &SemanticExecutionProjection,
) -> Result<SemanticReactiveProjection, SemanticReactiveLoweringError> {
    let mut lowerer = ReactiveLowerer {
        store,
        definitions: Vec::new(),
        bindings: Vec::new(),
        signal_ids: HashMap::new(),
        native_state_targets: HashMap::new(),
        native_event_targets: HashMap::new(),
        native_signals: HashSet::new(),
        visiting: HashSet::new(),
    };

    for object in projection.objects() {
        for binding in &object.signal_bindings {
            let property = lower_property(object.semantic_id, binding.property())?;
            let signal = lowerer.lower_signal(binding.signal())?;
            lowerer.bindings.push(noon_core::ReactiveBinding {
                signal,
                object: object.execution_id,
                property,
            });
        }
    }

    let mut lowered_signals = lowerer.signal_ids.keys().copied().collect::<Vec<_>>();
    lowered_signals.sort();
    let definition_indices = lowerer
        .definitions
        .iter()
        .enumerate()
        .map(|(index, definition)| (definition.id, index))
        .collect::<HashMap<_, _>>();
    let mut scalar_timeline = Vec::new();
    for semantic_signal in lowered_signals {
        let execution_signal = lowerer.signal_ids[&semantic_signal];
        let semantic_timeline = store
            .semantic_signal_state(semantic_signal)?
            .scalar_timeline();
        if !semantic_timeline.is_empty() {
            let initial = store
                .semantic_input_scalar_value_at(semantic_signal, 0.0)
                .expect("validated scalar tracks remain attached to a scalar input signal");
            let definition_index = definition_indices[&execution_signal];
            lowerer.definitions[definition_index].source = SignalSource::Input(
                ReactiveValue::Scalar(lower_scalar(semantic_signal, initial)?),
            );
        }
        for entry in semantic_timeline {
            scalar_timeline.push(lower_scalar_timeline_entry(*entry, execution_signal)?);
        }
    }
    let graph = ReactiveGraphDefinition::from_parts(lowerer.definitions, lowerer.bindings)?;
    let timeline_signals = scalar_timeline
        .iter()
        .map(|entry| entry.semantic_signal())
        .collect();
    Ok(SemanticReactiveProjection {
        graph,
        signal_ids: lowerer.signal_ids,
        native_state_targets: lowerer.native_state_targets,
        native_event_targets: lowerer.native_event_targets,
        native_signals: lowerer.native_signals,
        scalar_timeline,
        timeline_signals,
    })
}

struct ReactiveLowerer<'a> {
    store: &'a SemanticStore,
    definitions: Vec<SignalDefinition>,
    bindings: Vec<noon_core::ReactiveBinding>,
    signal_ids: HashMap<SemanticNodeId, SignalId>,
    native_state_targets: HashMap<NativeStateSource, Vec<SignalId>>,
    native_event_targets: HashMap<NativeEventSource, Vec<SignalId>>,
    native_signals: HashSet<SemanticNodeId>,
    visiting: HashSet<SemanticNodeId>,
}

impl ReactiveLowerer<'_> {
    fn lower_signal(
        &mut self,
        semantic_id: SemanticNodeId,
    ) -> Result<SignalId, SemanticReactiveLoweringError> {
        if let Some(signal) = self.signal_ids.get(&semantic_id).copied() {
            return Ok(signal);
        }
        if !self.visiting.insert(semantic_id) {
            return Err(SemanticReactiveLoweringError::DependencyCycle(semantic_id));
        }

        let state = self.store.semantic_signal_state(semantic_id)?;
        let source = state.source().clone();
        let native_input = state.native_input().cloned();
        let signal = compatibility_signal_id(semantic_id);
        let source = self.lower_source(semantic_id, &source)?;
        self.visiting.remove(&semantic_id);
        self.signal_ids.insert(semantic_id, signal);
        self.definitions
            .push(SignalDefinition { id: signal, source });
        if native_input.is_some() {
            self.native_signals.insert(semantic_id);
        }
        match native_input {
            Some(SemanticNativeInputSource::State(source)) => {
                self.native_state_targets
                    .entry(source)
                    .or_default()
                    .push(signal);
            }
            Some(SemanticNativeInputSource::Event(source)) => {
                self.native_event_targets
                    .entry(source)
                    .or_default()
                    .push(signal);
            }
            None => {}
        }
        Ok(signal)
    }

    fn lower_source(
        &mut self,
        owner: SemanticNodeId,
        source: &SemanticSignalSource,
    ) -> Result<SignalSource, SemanticReactiveLoweringError> {
        match source {
            SemanticSignalSource::Input(value) => {
                Ok(SignalSource::Input(lower_value(owner, value)?))
            }
            SemanticSignalSource::Derived(expression) => Ok(SignalSource::Derived(
                self.lower_expression(owner, expression)?,
            )),
        }
    }

    fn lower_expression(
        &mut self,
        owner: SemanticNodeId,
        expression: &SemanticSignalExpr,
    ) -> Result<ReactiveExpr, SemanticReactiveLoweringError> {
        match expression {
            SemanticSignalExpr::Constant(value) => {
                Ok(ReactiveExpr::Constant(lower_value(owner, value)?))
            }
            SemanticSignalExpr::Signal(signal) => {
                Ok(ReactiveExpr::Signal(self.lower_signal(*signal)?))
            }
            SemanticSignalExpr::Add(lhs, rhs) => Ok(ReactiveExpr::Add(
                Box::new(self.lower_expression(owner, lhs)?),
                Box::new(self.lower_expression(owner, rhs)?),
            )),
            SemanticSignalExpr::Sub(lhs, rhs) => Ok(ReactiveExpr::Sub(
                Box::new(self.lower_expression(owner, lhs)?),
                Box::new(self.lower_expression(owner, rhs)?),
            )),
            SemanticSignalExpr::Mul(lhs, rhs) => Ok(ReactiveExpr::Mul(
                Box::new(self.lower_expression(owner, lhs)?),
                Box::new(self.lower_expression(owner, rhs)?),
            )),
            SemanticSignalExpr::Neg(value) => Ok(ReactiveExpr::Neg(Box::new(
                self.lower_expression(owner, value)?,
            ))),
            SemanticSignalExpr::Sin(value) => Ok(ReactiveExpr::Sin(Box::new(
                self.lower_expression(owner, value)?,
            ))),
            SemanticSignalExpr::Cos(value) => Ok(ReactiveExpr::Cos(Box::new(
                self.lower_expression(owner, value)?,
            ))),
        }
    }
}

fn lower_value(
    signal: SemanticNodeId,
    value: &SemanticSignalValue,
) -> Result<ReactiveValue, SemanticReactiveLoweringError> {
    match value {
        SemanticSignalValue::Bool(value) => Ok(ReactiveValue::Bool(*value)),
        SemanticSignalValue::Scalar(value) => {
            Ok(ReactiveValue::Scalar(lower_scalar(signal, *value)?))
        }
        SemanticSignalValue::Vec3(value) => {
            value
                .lower_xy_f32()
                .map(ReactiveValue::Vec2)
                .map_err(|error| match error {
                    noon_core::SemanticLoweringError::NonFiniteVector(_) => {
                        SemanticReactiveLoweringError::NonFiniteSignalValue { signal }
                    }
                    noon_core::SemanticLoweringError::CoordinateOutOfRange(_) => {
                        SemanticReactiveLoweringError::SignalValueOutOfRange { signal }
                    }
                })
        }
    }
}

fn lower_scalar(signal: SemanticNodeId, value: f64) -> Result<f32, SemanticReactiveLoweringError> {
    if !value.is_finite() {
        return Err(SemanticReactiveLoweringError::NonFiniteSignalValue { signal });
    }
    if value.abs() > f32::MAX as f64 {
        return Err(SemanticReactiveLoweringError::SignalValueOutOfRange { signal });
    }
    Ok(value as f32)
}

fn lower_scalar_timeline_entry(
    entry: SemanticScalarSignalTimelineEntry,
    execution_signal: SignalId,
) -> Result<CompiledScalarSignalTimelineEntry, SemanticReactiveLoweringError> {
    let semantic_signal = entry.signal();
    Ok(match entry {
        SemanticScalarSignalTimelineEntry::Track(track) => {
            CompiledScalarSignalTimelineEntry::Track(CompiledScalarSignalTrack {
                semantic_signal,
                execution_signal,
                from: lower_scalar(semantic_signal, track.from())?,
                to: lower_scalar(semantic_signal, track.to())?,
                timing: track.timing(),
            })
        }
        SemanticScalarSignalTimelineEntry::Hold(hold) => {
            CompiledScalarSignalTimelineEntry::Hold(CompiledScalarSignalHold {
                semantic_signal,
                execution_signal,
                value: lower_scalar(semantic_signal, hold.value())?,
                start_time: hold.start_time(),
            })
        }
    })
}

fn lower_property(
    target: SemanticNodeId,
    property: SemanticObjectProperty,
) -> Result<Property, SemanticReactiveLoweringError> {
    match property {
        SemanticObjectProperty::Presence => Ok(Property::Presence),
        SemanticObjectProperty::Translation => Ok(Property::Position),
        SemanticObjectProperty::Scale => Ok(Property::Scale),
        SemanticObjectProperty::RotationZ => Ok(Property::Rotation),
        SemanticObjectProperty::ObjectOpacity => Ok(Property::Opacity),
        SemanticObjectProperty::FillOpacity
        | SemanticObjectProperty::StrokeOpacity
        | SemanticObjectProperty::StrokeWidth => {
            Err(SemanticReactiveLoweringError::UnsupportedProperty { target, property })
        }
    }
}

/// One-to-one compatibility encoding for semantic signal identity at the execution
/// boundary. It introduces no allocator and cannot alias another live generation.
fn compatibility_signal_id(id: SemanticNodeId) -> SignalId {
    let raw = (u64::from(id.generation()) << 32) | u64::from(id.slot());
    SignalId::new(raw)
}

#[cfg(test)]
mod tests {
    use noon_core::{
        NativeEventSource, NativeStateSource, RateFunction, ReactiveBinding,
        SemanticMutationTransaction, SemanticObjectState, SemanticSignalExpr, SemanticVec3,
        StoredGeometry, TrackTiming, Vec2,
    };

    use super::*;
    use crate::SemanticExecutionIndex;

    fn visible_circle(store: &mut SemanticStore) -> SemanticNodeId {
        let object =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        store.attach_to_scene(object).unwrap();
        object
    }

    fn projection(
        store: &SemanticStore,
        index: &mut SemanticExecutionIndex,
    ) -> SemanticExecutionProjection {
        index.lower_scene(store).unwrap()
    }

    #[test]
    fn lowers_only_bound_signal_dependency_closure_into_existing_native_graph() {
        let mut store = SemanticStore::new();
        let input = store.insert_semantic_input_signal(2.0_f64).unwrap();
        let derived = store
            .insert_semantic_derived_signal(SemanticSignalExpr::Add(
                Box::new(SemanticSignalExpr::signal(input)),
                Box::new(SemanticSignalExpr::scalar(3.0)),
            ))
            .unwrap();
        let unrelated = store.insert_semantic_input_signal(99.0_f64).unwrap();
        let object = visible_circle(&mut store);
        store
            .bind_semantic_signal(derived, object, SemanticObjectProperty::ObjectOpacity)
            .unwrap();

        let mut index = SemanticExecutionIndex::new();
        let execution = projection(&store, &mut index);
        let reactive = lower_semantic_reactive_projection(&store, &execution).unwrap();

        assert_eq!(reactive.signal_count(), 2);
        assert!(reactive.execution_signal_id(input).is_some());
        assert!(reactive.execution_signal_id(derived).is_some());
        assert_eq!(reactive.execution_signal_id(unrelated), None);
        assert_eq!(reactive.graph().signals().len(), 2);
        assert_eq!(
            reactive.graph().bindings(),
            &[ReactiveBinding {
                signal: reactive.execution_signal_id(derived).unwrap(),
                object: index.execution_object_id(object).unwrap(),
                property: Property::Opacity,
            }]
        );

        let input_id = reactive.execution_signal_id(input).unwrap();
        let derived_id = reactive.execution_signal_id(derived).unwrap();
        assert!(reactive.graph().signals().iter().any(|signal| {
            signal.id == input_id
                && signal.source == SignalSource::Input(ReactiveValue::Scalar(2.0))
        }));
        assert!(reactive.graph().signals().iter().any(|signal| {
            signal.id == derived_id
                && signal.source
                    == SignalSource::Derived(ReactiveExpr::Add(
                        Box::new(ReactiveExpr::Signal(input_id)),
                        Box::new(ReactiveExpr::Constant(ReactiveValue::Scalar(3.0))),
                    ))
        }));
    }

    #[test]
    fn native_input_routes_are_lowered_with_the_active_reactive_dependency_closure() {
        let mut store = SemanticStore::new();
        let control = store.insert_semantic_input_signal(0.25_f64).unwrap();
        let event = store.insert_semantic_input_signal(0.0_f64).unwrap();
        let unrelated = store.insert_semantic_input_signal(false).unwrap();
        let control_source = NativeStateSource::Control {
            name: "opacity".to_owned(),
        };
        let event_source = NativeEventSource::KeyPress {
            code: "Space".to_owned(),
        };
        store
            .bind_semantic_native_state_input(control, control_source.clone())
            .unwrap();
        store
            .bind_semantic_native_event_input(event, event_source.clone())
            .unwrap();
        store
            .bind_semantic_native_state_input(
                unrelated,
                NativeStateSource::Key {
                    code: "KeyZ".to_owned(),
                },
            )
            .unwrap();
        let object = visible_circle(&mut store);
        store
            .bind_semantic_signal(control, object, SemanticObjectProperty::ObjectOpacity)
            .unwrap();
        let derived = store
            .insert_semantic_derived_signal(SemanticSignalExpr::Add(
                Box::new(SemanticSignalExpr::signal(event)),
                Box::new(SemanticSignalExpr::scalar(1.0)),
            ))
            .unwrap();
        store
            .bind_semantic_signal(derived, object, SemanticObjectProperty::RotationZ)
            .unwrap();

        let mut index = SemanticExecutionIndex::new();
        let execution = projection(&store, &mut index);
        let reactive = lower_semantic_reactive_projection(&store, &execution).unwrap();
        let control_id = reactive.execution_signal_id(control).unwrap();
        let event_id = reactive.execution_signal_id(event).unwrap();

        assert_eq!(
            reactive.native_state_targets(&control_source),
            &[control_id]
        );
        assert_eq!(reactive.native_event_targets(&event_source), &[event_id]);
        assert!(reactive
            .native_state_targets(&NativeStateSource::Key {
                code: "KeyZ".to_owned(),
            })
            .is_empty());
    }

    #[test]
    fn vector_signals_lower_explicitly_to_current_xy_execution_domain() {
        let mut store = SemanticStore::new();
        let signal = store
            .insert_semantic_input_signal(SemanticVec3::new(4.0, -2.0, 17.0))
            .unwrap();
        let object = visible_circle(&mut store);
        store
            .bind_semantic_signal(signal, object, SemanticObjectProperty::Translation)
            .unwrap();

        let mut index = SemanticExecutionIndex::new();
        let execution = projection(&store, &mut index);
        let reactive = lower_semantic_reactive_projection(&store, &execution).unwrap();
        let signal_id = reactive.execution_signal_id(signal).unwrap();

        assert!(reactive.graph().signals().iter().any(|definition| {
            definition.id == signal_id
                && definition.source
                    == SignalSource::Input(ReactiveValue::Vec2(Vec2::new(4.0, -2.0)))
        }));
        assert_eq!(reactive.graph().bindings()[0].property, Property::Position);
    }

    #[test]
    fn unsupported_style_channels_fail_instead_of_collapsing_semantics() {
        let mut store = SemanticStore::new();
        let signal = store.insert_semantic_input_signal(0.5_f64).unwrap();
        let object = visible_circle(&mut store);
        store
            .bind_semantic_signal(signal, object, SemanticObjectProperty::FillOpacity)
            .unwrap();

        let mut index = SemanticExecutionIndex::new();
        let execution = projection(&store, &mut index);
        assert_eq!(
            lower_semantic_reactive_projection(&store, &execution).unwrap_err(),
            SemanticReactiveLoweringError::UnsupportedProperty {
                target: object,
                property: SemanticObjectProperty::FillOpacity,
            }
        );
    }

    #[test]
    fn stale_dependency_generation_fails_closed_during_lowering() {
        let mut store = SemanticStore::new();
        let dependency = store.insert_semantic_input_signal(1.0_f64).unwrap();
        let derived = store
            .insert_semantic_derived_signal(SemanticSignalExpr::signal(dependency))
            .unwrap();
        let object = visible_circle(&mut store);
        store
            .bind_semantic_signal(derived, object, SemanticObjectProperty::ObjectOpacity)
            .unwrap();

        store.remove_node(dependency).unwrap();
        let replacement = store.insert_semantic_input_signal(2.0_f64).unwrap();
        assert_eq!(dependency.slot(), replacement.slot());
        assert_ne!(dependency.generation(), replacement.generation());

        let mut index = SemanticExecutionIndex::new();
        let execution = projection(&store, &mut index);
        assert!(matches!(
            lower_semantic_reactive_projection(&store, &execution),
            Err(SemanticReactiveLoweringError::Signal(
                SemanticSignalError::UnknownSignal(id)
            )) if id == dependency
        ));
    }

    #[test]
    fn lowers_only_reachable_scalar_tracks_with_semantic_ownership_index() {
        let mut store = SemanticStore::new();
        let reachable = store.insert_semantic_input_signal(0.0_f64).unwrap();
        let unrelated = store.insert_semantic_input_signal(10.0_f64).unwrap();
        let object = visible_circle(&mut store);
        store
            .bind_semantic_signal(reachable, object, SemanticObjectProperty::RotationZ)
            .unwrap();
        let mut transaction = SemanticMutationTransaction::new();
        transaction.add_scalar_signal_track(
            reachable,
            0.0,
            4.0,
            TrackTiming::new(0.0, 2.0, RateFunction::Linear),
        );
        transaction.add_scalar_signal_track(
            unrelated,
            10.0,
            20.0,
            TrackTiming::new(0.0, 2.0, RateFunction::Linear),
        );
        transaction.apply(&mut store).unwrap();

        let mut index = SemanticExecutionIndex::new();
        let execution = projection(&store, &mut index);
        let reactive = lower_semantic_reactive_projection(&store, &execution).unwrap();
        assert_eq!(reactive.scalar_timeline().len(), 1);
        let CompiledScalarSignalTimelineEntry::Track(track) = reactive.scalar_timeline()[0] else {
            panic!("expected one lowered scalar track")
        };
        assert_eq!(track.semantic_signal(), reachable);
        assert!(reactive.timeline_owns(reachable));
        assert!(!reactive.timeline_owns(unrelated));
    }
}
