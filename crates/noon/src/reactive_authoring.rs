use std::ops::{Deref, DerefMut};

use noon_core::{
    Property, ReactiveExpr, ReactiveGraphDefinition, ReactiveValue, SemanticScene, SignalId, Vec2,
};

use crate::{Mobject, Scene};

/// Stable scalar signal handle with Manim-compatible `ValueTracker` vocabulary.
///
/// The handle contains no evaluator or callback. Values and dependencies are stored
/// in the core `ReactiveGraphDefinition` owned by [`ReactiveScene`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ValueTracker {
    signal: SignalId,
}

impl ValueTracker {
    pub const fn signal_id(self) -> SignalId {
        self.signal
    }
}

/// Stable handle for a derived or input vector signal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct VectorSignal {
    signal: SignalId,
}

impl VectorSignal {
    pub const fn signal_id(self) -> SignalId {
        self.signal
    }
}

/// Reactive-capable authoring facade that preserves the existing [`Scene`] API.
///
/// `ReactiveScene` owns the established deterministic `Scene` plus one native
/// `ReactiveGraphDefinition`. `Deref`/`DerefMut` keep ordinary shape, layout and
/// animation authoring unchanged. Calling [`ReactiveScene::semantic_scene`] lowers
/// both parts into the single core [`SemanticScene`] consumed by the runtime.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ReactiveScene {
    scene: Scene,
    reactive: ReactiveGraphDefinition,
}

impl ReactiveScene {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn value_tracker(&mut self, value: f32) -> ValueTracker {
        ValueTracker {
            signal: self.reactive.add_input(value),
        }
    }

    pub fn vector_signal(&mut self, value: Vec2) -> VectorSignal {
        VectorSignal {
            signal: self.reactive.add_input(value),
        }
    }

    /// Derive a vector position as `offset + tracker * direction`.
    ///
    /// This common tracker pattern stays fully declarative and native at runtime.
    pub fn position_from_tracker(
        &mut self,
        tracker: ValueTracker,
        direction: Vec2,
        offset: Vec2,
    ) -> VectorSignal {
        let scaled = ReactiveExpr::Mul(
            Box::new(ReactiveExpr::signal(tracker.signal)),
            Box::new(ReactiveExpr::vec2(direction)),
        );
        VectorSignal {
            signal: self.reactive.add_derived(ReactiveExpr::Add(
                Box::new(ReactiveExpr::vec2(offset)),
                Box::new(scaled),
            )),
        }
    }

    pub fn bind_rotation(&mut self, object: Mobject, tracker: ValueTracker) -> &mut Self {
        self.reactive
            .bind(tracker.signal, object.id(), Property::Rotation);
        self
    }

    pub fn bind_opacity(&mut self, object: Mobject, tracker: ValueTracker) -> &mut Self {
        self.reactive
            .bind(tracker.signal, object.id(), Property::Opacity);
        self
    }

    pub fn bind_appearance(&mut self, object: Mobject, tracker: ValueTracker) -> &mut Self {
        self.reactive
            .bind(tracker.signal, object.id(), Property::Appearance);
        self
    }

    pub fn bind_reveal(&mut self, object: Mobject, tracker: ValueTracker) -> &mut Self {
        self.reactive
            .bind(tracker.signal, object.id(), Property::Reveal);
        self
    }

    pub fn bind_morph(&mut self, object: Mobject, tracker: ValueTracker) -> &mut Self {
        self.reactive
            .bind(tracker.signal, object.id(), Property::Morph);
        self
    }

    pub fn bind_position(&mut self, object: Mobject, signal: VectorSignal) -> &mut Self {
        self.reactive
            .bind(signal.signal, object.id(), Property::Position);
        self
    }

    /// Low-level escape hatch for new property/signal combinations.
    ///
    /// The core reactive compiler remains authoritative for type and driver
    /// validation, so this does not duplicate semantic rules in the facade.
    pub fn bind_signal(
        &mut self,
        signal: SignalId,
        object: Mobject,
        property: Property,
    ) -> &mut Self {
        self.reactive.bind(signal, object.id(), property);
        self
    }

    pub fn reactive_graph(&self) -> &ReactiveGraphDefinition {
        &self.reactive
    }

    pub fn semantic_scene(&self) -> SemanticScene {
        let mut semantic = SemanticScene::from_definition(self.scene.definition().clone());
        *semantic.reactive_mut() = self.reactive.clone();
        semantic
    }

    pub fn into_semantic_scene(self) -> SemanticScene {
        let mut semantic = SemanticScene::from_definition(self.scene.into_definition());
        *semantic.reactive_mut() = self.reactive;
        semantic
    }
}

impl Deref for ReactiveScene {
    type Target = Scene;

    fn deref(&self) -> &Self::Target {
        &self.scene
    }
}

impl DerefMut for ReactiveScene {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.scene
    }
}

impl From<ReactiveScene> for SemanticScene {
    fn from(value: ReactiveScene) -> Self {
        value.into_semantic_scene()
    }
}

#[cfg(test)]
mod tests {
    use noon_core::{ObjectExecutionClass, Property, ReactiveValue, RIGHT, UP};

    use super::*;
    use crate::{Circle, Square};

    #[test]
    fn reactive_scene_preserves_existing_scene_authoring_ergonomics() {
        let mut scene = ReactiveScene::new();
        let circle = scene.add(Circle::new(0.5).shift(RIGHT));
        let square = scene.add(Square::new(1.0));
        scene.edit(square).unwrap().next_to(circle, UP, 0.25).unwrap();

        assert_eq!(scene.definition().objects().len(), 2);
        assert_eq!(scene.snapshot(circle).unwrap().transform.translation, RIGHT);
    }

    #[test]
    fn value_tracker_lowers_to_core_reactive_graph_without_frontend_evaluator() {
        let mut scene = ReactiveScene::new();
        let square = scene.add(Square::new(1.0));
        let angle = scene.value_tracker(0.25);
        scene.bind_rotation(square, angle);

        let semantic = scene.semantic_scene();
        let program = semantic.compile_reactive().expect("reactive graph must compile");
        let state = program.instantiate();

        assert_eq!(state.value(angle.signal_id()), Some(&ReactiveValue::Scalar(0.25)));
        assert_eq!(
            program.analysis().class_for(square.id()),
            Some(ObjectExecutionClass::Reactive)
        );
        assert_eq!(semantic.reactive().bindings()[0].property, Property::Rotation);
    }

    #[test]
    fn scalar_tracker_can_drive_native_vector_expression() {
        let mut scene = ReactiveScene::new();
        let circle = scene.add(Circle::new(0.5));
        let progress = scene.value_tracker(2.0);
        let position = scene.position_from_tracker(progress, RIGHT, UP);
        scene.bind_position(circle, position);

        let semantic = scene.semantic_scene();
        let program = semantic.compile_reactive().expect("reactive graph must compile");
        let state = program.instantiate();

        assert_eq!(
            state.value(position.signal_id()),
            Some(&ReactiveValue::Vec2(Vec2::new(2.0, 1.0)))
        );
    }
}
