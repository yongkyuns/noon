use std::collections::BTreeMap;

use noon_core::{
    NativeEventSource, NativeInputBinding, NativeInputDefinition, NativeInputError,
    NativeStateSource, ReactiveError, ReactiveGraphDefinition, ReactiveValue, SignalId,
    SignalTimelineError, TimedSemanticScene, ValueKind,
};

use crate::SceneInstance;

use super::{ReactiveRuntimeStats, TimedSceneInstance, TimedSceneRuntimeError};

const EVENT_SEQUENCE_WRAP: f32 = 1_000_000.0;

/// Work and backpressure-relevant counters for native input dispatch.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NativeInputStats {
    pub state_samples_received: u64,
    pub state_samples_coalesced: u64,
    pub state_dispatches_dropped: u64,
    pub events_received: u64,
    pub event_dispatches_dropped: u64,
    pub reactive_updates: u64,
    pub derived_signals_evaluated: u64,
    pub bindings_invalidated: u64,
}

/// Dense runtime routing from normalized native input sources to reactive signal IDs.
///
/// The router indexes only declared native inputs. Dispatch never scans semantic
/// objects or the full reactive graph. Identical sampled state is coalesced before
/// entering the VM; discrete events always advance their scalar sequence signal.
#[derive(Clone, Debug, Default)]
pub struct NativeInputRouter {
    state: BTreeMap<NativeStateSource, Vec<SignalId>>,
    events: BTreeMap<NativeEventSource, Vec<SignalId>>,
    stats: NativeInputStats,
}

impl NativeInputRouter {
    pub fn from_scene(scene: &TimedSemanticScene) -> Result<Self, SignalTimelineError> {
        scene
            .native_inputs()
            .validate(scene.semantic().reactive())?;
        if let Some(track) = scene
            .signal_timeline()
            .tracks()
            .iter()
            .find(|track| scene.native_inputs().drives(track.signal))
        {
            return Err(SignalTimelineError::ExternallyDrivenSignal(track.signal));
        }
        Ok(Self::from_validated_definition(scene.native_inputs()))
    }

    /// Build routing over an already-lowered native-reactive execution graph.
    ///
    /// This is the current `SemanticStore -> ExecutionSession` entry point. Signal
    /// identity has already been resolved by canonical lowering; the router only
    /// owns source-to-signal lookup and dispatch accounting.
    pub fn from_definition(
        graph: &ReactiveGraphDefinition,
        inputs: &NativeInputDefinition,
    ) -> Result<Self, NativeInputError> {
        inputs.validate(graph)?;
        Ok(Self::from_validated_definition(inputs))
    }

    fn from_validated_definition(inputs: &NativeInputDefinition) -> Self {
        let mut result = Self::default();
        for binding in inputs.bindings() {
            match binding {
                NativeInputBinding::State { source, signal } => {
                    result
                        .state
                        .entry(source.clone())
                        .or_default()
                        .push(*signal);
                }
                NativeInputBinding::Event { source, signal } => {
                    result
                        .events
                        .entry(source.clone())
                        .or_default()
                        .push(*signal);
                }
            }
        }
        result
    }

    pub const fn stats(&self) -> NativeInputStats {
        self.stats
    }

    pub fn reset_stats(&mut self) {
        self.stats = NativeInputStats::default();
    }

    pub fn has_state_source(&self, source: &NativeStateSource) -> bool {
        self.state.contains_key(source)
    }

    pub fn has_event_source(&self, source: &NativeEventSource) -> bool {
        self.events.contains_key(source)
    }

    pub fn dispatch_state(
        &mut self,
        instance: &mut TimedSceneInstance,
        source: &NativeStateSource,
        value: impl Into<ReactiveValue>,
    ) -> Result<bool, TimedSceneRuntimeError> {
        self.dispatch_state_to(instance, source, value.into())
    }

    /// Dispatch sampled state into the current canonical scene runtime.
    pub fn dispatch_state_scene(
        &mut self,
        instance: &mut SceneInstance,
        source: &NativeStateSource,
        value: impl Into<ReactiveValue>,
    ) -> Result<bool, ReactiveError> {
        self.dispatch_state_to(instance, source, value.into())
    }

    fn dispatch_state_to<T: NativeInputTarget>(
        &mut self,
        instance: &mut T,
        source: &NativeStateSource,
        value: ReactiveValue,
    ) -> Result<bool, T::Error> {
        self.stats.state_samples_received += 1;
        let Some(signals) = self.state.get(source) else {
            self.stats.state_dispatches_dropped += 1;
            return Ok(false);
        };
        let mut changed = false;
        for signal in signals {
            if instance.reactive_value(*signal) == Some(&value) {
                continue;
            }
            instance.set_reactive_input(*signal, value.clone())?;
            changed = true;
            self.stats.reactive_updates += 1;
            let reactive = instance.last_reactive_stats();
            self.stats.derived_signals_evaluated += reactive.derived_signals_evaluated as u64;
            self.stats.bindings_invalidated += reactive.bindings_invalidated as u64;
        }
        if !changed {
            self.stats.state_samples_coalesced += 1;
        }
        Ok(changed)
    }

    pub fn emit_event(
        &mut self,
        instance: &mut TimedSceneInstance,
        source: &NativeEventSource,
    ) -> Result<bool, TimedSceneRuntimeError> {
        self.emit_event_to(instance, source)
    }

    /// Dispatch one ordered event occurrence into the current canonical scene runtime.
    pub fn emit_event_scene(
        &mut self,
        instance: &mut SceneInstance,
        source: &NativeEventSource,
    ) -> Result<bool, ReactiveError> {
        self.emit_event_to(instance, source)
    }

    fn emit_event_to<T: NativeInputTarget>(
        &mut self,
        instance: &mut T,
        source: &NativeEventSource,
    ) -> Result<bool, T::Error> {
        self.stats.events_received += 1;
        let Some(signals) = self.events.get(source) else {
            self.stats.event_dispatches_dropped += 1;
            return Ok(false);
        };
        for signal in signals {
            let current = instance
                .reactive_value(*signal)
                .expect("validated native event signal must exist");
            let ReactiveValue::Scalar(current) = current else {
                return Err(ReactiveError::InputTypeMismatch {
                    signal: *signal,
                    expected: ValueKind::Scalar,
                    actual: current.value_kind(),
                }
                .into());
            };
            let next = if *current >= EVENT_SEQUENCE_WRAP {
                0.0
            } else {
                *current + 1.0
            };
            instance.set_reactive_input(*signal, ReactiveValue::Scalar(next))?;
            self.stats.reactive_updates += 1;
            let reactive = instance.last_reactive_stats();
            self.stats.derived_signals_evaluated += reactive.derived_signals_evaluated as u64;
            self.stats.bindings_invalidated += reactive.bindings_invalidated as u64;
        }
        Ok(true)
    }
}

trait NativeInputTarget {
    type Error: From<ReactiveError>;

    fn reactive_value(&self, signal: SignalId) -> Option<&ReactiveValue>;
    fn set_reactive_input(
        &mut self,
        signal: SignalId,
        value: ReactiveValue,
    ) -> Result<(), Self::Error>;
    fn last_reactive_stats(&self) -> ReactiveRuntimeStats;
}

impl NativeInputTarget for TimedSceneInstance {
    type Error = TimedSceneRuntimeError;

    fn reactive_value(&self, signal: SignalId) -> Option<&ReactiveValue> {
        TimedSceneInstance::reactive_value(self, signal)
    }

    fn set_reactive_input(
        &mut self,
        signal: SignalId,
        value: ReactiveValue,
    ) -> Result<(), Self::Error> {
        TimedSceneInstance::set_reactive_input(self, signal, value).map(|_| ())
    }

    fn last_reactive_stats(&self) -> ReactiveRuntimeStats {
        TimedSceneInstance::last_reactive_stats(self)
    }
}

impl NativeInputTarget for SceneInstance {
    type Error = ReactiveError;

    fn reactive_value(&self, signal: SignalId) -> Option<&ReactiveValue> {
        SceneInstance::reactive_value(self, signal)
    }

    fn set_reactive_input(
        &mut self,
        signal: SignalId,
        value: ReactiveValue,
    ) -> Result<(), Self::Error> {
        SceneInstance::set_reactive_input(self, signal, value).map(|_| ())
    }

    fn last_reactive_stats(&self) -> ReactiveRuntimeStats {
        SceneInstance::last_reactive_stats(self)
    }
}

#[cfg(test)]
mod tests {
    use noon_core::{
        GeometryRef, NativeInputDefinition, Property, SemanticScene, SignalTimelineDefinition, Vec2,
    };

    use super::*;

    fn input_scene() -> TimedSemanticScene {
        let mut semantic = SemanticScene::new();
        let object = semantic.add(GeometryRef::rectangle(1.0, 1.0));
        let pointer = semantic.add_input(Vec2::ZERO);
        semantic.bind(pointer, object, Property::Position);
        let visible = semantic.add_input(false);
        semantic.bind(visible, object, Property::Presence);
        let clicks = semantic.add_input(0.0_f32);
        semantic.bind(clicks, object, Property::Rotation);

        let mut inputs = NativeInputDefinition::new();
        inputs
            .bind_state(NativeStateSource::PointerPosition, pointer)
            .bind_state(
                NativeStateSource::Key {
                    code: "Space".to_owned(),
                },
                visible,
            )
            .bind_event(NativeEventSource::PointerDown { button: 0 }, clicks);
        TimedSemanticScene::from_parts_with_native_inputs(
            semantic,
            SignalTimelineDefinition::new(),
            inputs,
        )
        .unwrap()
    }

    #[test]
    fn sampled_state_is_dependency_local_and_identical_samples_coalesce() {
        let scene = input_scene();
        let mut instance = TimedSceneInstance::from_timed(&scene).unwrap();
        let mut router = NativeInputRouter::from_scene(&scene).unwrap();

        assert!(router
            .dispatch_state(
                &mut instance,
                &NativeStateSource::PointerPosition,
                Vec2::new(2.0, -1.0),
            )
            .unwrap());
        assert_eq!(
            instance.frame().objects[0].transform.translation,
            Vec2::new(2.0, -1.0)
        );
        assert!(!router
            .dispatch_state(
                &mut instance,
                &NativeStateSource::PointerPosition,
                Vec2::new(2.0, -1.0),
            )
            .unwrap());
        assert_eq!(router.stats().state_samples_received, 2);
        assert_eq!(router.stats().state_samples_coalesced, 1);
        assert_eq!(router.stats().bindings_invalidated, 1);
    }

    #[test]
    fn repeated_discrete_events_always_advance_the_event_signal() {
        let scene = input_scene();
        let mut instance = TimedSceneInstance::from_timed(&scene).unwrap();
        let mut router = NativeInputRouter::from_scene(&scene).unwrap();
        let source = NativeEventSource::PointerDown { button: 0 };

        router.emit_event(&mut instance, &source).unwrap();
        assert_eq!(instance.frame().objects[0].transform.rotation, 1.0);
        router.emit_event(&mut instance, &source).unwrap();
        assert_eq!(instance.frame().objects[0].transform.rotation, 2.0);
        assert_eq!(router.stats().events_received, 2);
        assert_eq!(router.stats().reactive_updates, 2);
    }

    #[test]
    fn unbound_sources_are_counted_as_dropped_without_scene_work() {
        let scene = input_scene();
        let mut instance = TimedSceneInstance::from_timed(&scene).unwrap();
        let mut router = NativeInputRouter::from_scene(&scene).unwrap();
        assert!(!router
            .dispatch_state(
                &mut instance,
                &NativeStateSource::ViewportSize,
                Vec2::new(640.0, 360.0),
            )
            .unwrap());
        assert_eq!(router.stats().state_dispatches_dropped, 1);
        assert_eq!(router.stats().reactive_updates, 0);
    }
}
