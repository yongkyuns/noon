//! Canonical native-input declarations over shared semantic signal identity.
//!
//! These handles retain only their owning store and one `SemanticNodeId`.
//! Routing and effective values are lowered into the canonical `ExecutionSession`.

use std::{cell::RefCell, rc::Rc};

use noon_core::{
    NativeEventSource, NativeStateSource, SemanticMutationTransaction, SemanticNativeInputSource,
    SemanticNodeId, SemanticObjectProperty, SemanticSignalValue, SemanticStore, SemanticVec3,
};

use crate::{Mobject, Scene, ValueTracker};

#[derive(Clone, Debug)]
struct NativeSignalHandle {
    store: Rc<RefCell<SemanticStore>>,
    node: SemanticNodeId,
}

impl NativeSignalHandle {
    const fn node_id(&self) -> SemanticNodeId {
        self.node
    }

    fn is_in_store(&self, store: &Rc<RefCell<SemanticStore>>) -> bool {
        Rc::ptr_eq(&self.store, store)
    }

    fn require_store(&self, store: &Rc<RefCell<SemanticStore>>) -> Result<(), String> {
        if !self.is_in_store(store) {
            return Err("native signal belongs to another scene store".into());
        }
        self.store
            .borrow()
            .semantic_signal_state(self.node)
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
}

/// A Vec3-valued semantic input owned by one native state source.
#[derive(Clone, Debug)]
pub struct NativeVectorSignal(NativeSignalHandle);

impl NativeVectorSignal {
    pub const fn node_id(&self) -> SemanticNodeId {
        self.0.node_id()
    }

    pub fn is_in_store(&self, store: &Rc<RefCell<SemanticStore>>) -> bool {
        self.0.is_in_store(store)
    }
}

/// A bool-valued semantic input owned by one native state source.
#[derive(Clone, Debug)]
pub struct NativeBoolSignal(NativeSignalHandle);

impl NativeBoolSignal {
    pub const fn node_id(&self) -> SemanticNodeId {
        self.0.node_id()
    }

    pub fn is_in_store(&self, store: &Rc<RefCell<SemanticStore>>) -> bool {
        self.0.is_in_store(store)
    }
}

impl Scene {
    pub fn pointer_position_signal(&self) -> Result<NativeVectorSignal, String> {
        self.native_vector_signal(NativeStateSource::PointerPosition)
    }

    pub fn viewport_size_signal(&self) -> Result<NativeVectorSignal, String> {
        self.native_vector_signal(NativeStateSource::ViewportSize)
    }

    pub fn wheel_delta_signal(&self) -> Result<NativeVectorSignal, String> {
        self.native_vector_signal(NativeStateSource::WheelDelta)
    }

    pub fn key_state_signal(
        &self,
        code: impl Into<String>,
        initial: bool,
    ) -> Result<NativeBoolSignal, String> {
        let code = nonempty_name("key code", code.into())?;
        self.native_bool_signal(NativeStateSource::Key { code }, initial)
    }

    pub fn control_signal(
        &self,
        name: impl Into<String>,
        initial: f64,
    ) -> Result<ValueTracker, String> {
        let name = nonempty_name("control name", name.into())?;
        let (store, node) = self.native_signal(
            SemanticSignalValue::Scalar(initial),
            SemanticNativeInputSource::State(NativeStateSource::Control { name }),
        )?;
        Ok(ValueTracker::from_semantic_node(store, node))
    }

    pub fn pointer_down_events(&self, button: u8) -> Result<ValueTracker, String> {
        let (store, node) = self.native_signal(
            SemanticSignalValue::Scalar(0.0),
            SemanticNativeInputSource::Event(NativeEventSource::PointerDown { button }),
        )?;
        Ok(ValueTracker::from_semantic_node(store, node))
    }

    pub fn wheel_events(&self) -> Result<ValueTracker, String> {
        let (store, node) = self.native_signal(
            SemanticSignalValue::Scalar(0.0),
            SemanticNativeInputSource::Event(NativeEventSource::Wheel),
        )?;
        Ok(ValueTracker::from_semantic_node(store, node))
    }

    pub fn control_commit_events(&self, name: impl Into<String>) -> Result<ValueTracker, String> {
        let name = nonempty_name("control name", name.into())?;
        let (store, node) = self.native_signal(
            SemanticSignalValue::Scalar(0.0),
            SemanticNativeInputSource::Event(NativeEventSource::ControlCommit { name }),
        )?;
        Ok(ValueTracker::from_semantic_node(store, node))
    }

    /// Bind a native vector input directly to authored translation semantics.
    pub fn bind_native_translation(
        &self,
        object: &Mobject,
        signal: &NativeVectorSignal,
    ) -> Result<(), String> {
        self.require_object(object)?;
        signal.0.require_store(self.store())?;
        self.bind_signal(
            object,
            signal.node_id(),
            SemanticObjectProperty::Translation,
        )
    }

    /// Bind a scalar input/event counter to rotation around the semantic z axis.
    pub fn bind_rotation(&self, object: &Mobject, signal: &ValueTracker) -> Result<(), String> {
        signal.require_store(self.store())?;
        self.require_object(object)?;
        self.bind_signal(object, signal.node_id(), SemanticObjectProperty::RotationZ)
    }

    /// Bind a scalar input to the object's composed opacity.
    pub fn bind_opacity(&self, object: &Mobject, signal: &ValueTracker) -> Result<(), String> {
        signal.require_store(self.store())?;
        self.require_object(object)?;
        self.bind_signal(
            object,
            signal.node_id(),
            SemanticObjectProperty::ObjectOpacity,
        )
    }

    /// Bind a native bool state to whether the object participates in rendering.
    pub fn bind_presence(&self, object: &Mobject, signal: &NativeBoolSignal) -> Result<(), String> {
        signal.0.require_store(self.store())?;
        self.require_object(object)?;
        self.bind_signal(object, signal.node_id(), SemanticObjectProperty::Presence)
    }

    fn native_vector_signal(
        &self,
        source: NativeStateSource,
    ) -> Result<NativeVectorSignal, String> {
        let (store, node) = self.native_signal(
            SemanticSignalValue::Vec3(SemanticVec3::ZERO),
            SemanticNativeInputSource::State(source),
        )?;
        Ok(NativeVectorSignal(NativeSignalHandle { store, node }))
    }

    fn native_bool_signal(
        &self,
        source: NativeStateSource,
        initial: bool,
    ) -> Result<NativeBoolSignal, String> {
        let (store, node) = self.native_signal(
            SemanticSignalValue::Bool(initial),
            SemanticNativeInputSource::State(source),
        )?;
        Ok(NativeBoolSignal(NativeSignalHandle { store, node }))
    }

    fn native_signal(
        &self,
        initial: SemanticSignalValue,
        source: SemanticNativeInputSource,
    ) -> Result<(Rc<RefCell<SemanticStore>>, SemanticNodeId), String> {
        let store = Rc::clone(self.store());
        let mut semantic = store.borrow_mut();
        let node = semantic
            .insert_semantic_input_signal(initial)
            .map_err(|error| error.to_string())?;
        semantic
            .set_semantic_native_input(node, Some(source))
            .expect("fresh type-matched input accepts one native owner");
        let mut scope = SemanticMutationTransaction::new();
        scope.scope_signal(self.root(), node);
        scope
            .apply(&mut semantic)
            .map_err(|error| error.to_string())?;
        drop(semantic);
        Ok((store, node))
    }

    fn bind_signal(
        &self,
        object: &Mobject,
        signal: SemanticNodeId,
        property: SemanticObjectProperty,
    ) -> Result<(), String> {
        self.store()
            .borrow_mut()
            .bind_semantic_signal(signal, object.node_id(), property)
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
}

fn nonempty_name(kind: &str, value: String) -> Result<String, String> {
    if value.trim().is_empty() {
        Err(format!("native input {kind} must not be empty"))
    } else {
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use noon_core::{
        NativeEventOccurrence, NativeInputValue, ReactiveValue, SemanticNativeInputSource, Vec2,
    };

    use super::*;

    #[test]
    fn canonical_native_declarations_and_bindings_drive_one_execution_session() {
        let mut scene = Scene::new();
        let square = scene.square(0.9).unwrap();
        scene.add(&square).unwrap();

        let pointer = scene.pointer_position_signal().unwrap();
        scene.bind_native_translation(&square, &pointer).unwrap();
        let opacity = scene.control_signal("opacity", 1.0).unwrap();
        scene.bind_opacity(&square, &opacity).unwrap();
        let clicks = scene.pointer_down_events(0).unwrap();
        scene.bind_rotation(&square, &clicks).unwrap();

        let key = scene.key_state_signal("Space", false).unwrap();
        scene.bind_presence(&square, &key).unwrap();
        let viewport = scene.viewport_size_signal().unwrap();
        let wheel_delta = scene.wheel_delta_signal().unwrap();
        let wheel = scene.wheel_events().unwrap();
        let commit = scene.control_commit_events("opacity").unwrap();

        let store = scene.store().borrow();
        assert_eq!(
            store
                .semantic_signal_state(key.node_id())
                .unwrap()
                .native_input(),
            Some(&SemanticNativeInputSource::State(NativeStateSource::Key {
                code: "Space".to_owned(),
            }))
        );
        for node in [viewport.node_id(), wheel_delta.node_id()] {
            assert!(store
                .semantic_signal_state(node)
                .unwrap()
                .native_input()
                .is_some());
        }
        for node in [wheel.node_id(), commit.node_id()] {
            assert!(store
                .semantic_signal_state(node)
                .unwrap()
                .native_input()
                .is_some());
        }
        drop(store);

        let mut session = scene.execution_session().unwrap();
        assert!(!session.frame().presences[0]);
        session
            .set_native_state_input(
                NativeStateSource::PointerPosition,
                NativeInputValue::Vec2(Vec2::new(1.5, -0.5)),
            )
            .unwrap();
        session
            .set_native_state_input(
                NativeStateSource::Control {
                    name: "opacity".to_owned(),
                },
                NativeInputValue::Scalar(0.4),
            )
            .unwrap();
        session
            .emit_native_event(NativeEventOccurrence::new(
                0,
                NativeEventSource::PointerDown { button: 0 },
            ))
            .unwrap();
        session
            .set_native_state_input(
                NativeStateSource::Key {
                    code: "Space".to_owned(),
                },
                NativeInputValue::Bool(true),
            )
            .unwrap();

        let frame = &session.frame().objects[0];
        assert_eq!(frame.transform.translation, Vec2::new(1.5, -0.5));
        assert_eq!(frame.transform.rotation, 1.0);
        assert_eq!(frame.style.opacity, 0.4);
        assert!(session.frame().presences[0]);
        assert_eq!(
            session.effective_signal_value(pointer.node_id()),
            Some(&ReactiveValue::Vec2(Vec2::new(1.5, -0.5)))
        );
    }

    #[test]
    fn invalid_names_and_foreign_handles_fail_before_semantic_mutation() {
        let scene = Scene::new();
        let before = scene.store().borrow().slot_capacity();
        assert!(scene.key_state_signal(" ", false).is_err());
        assert!(scene.control_signal("", 1.0).is_err());
        assert!(scene.control_commit_events("\t").is_err());
        assert_eq!(scene.store().borrow().slot_capacity(), before);

        let mut foreign = Scene::new();
        let object = foreign.square(1.0).unwrap();
        foreign.add(&object).unwrap();
        let pointer = scene.pointer_position_signal().unwrap();
        assert!(foreign.bind_native_translation(&object, &pointer).is_err());
        assert!(foreign
            .store()
            .borrow()
            .semantic_object_signal_bindings(object.node_id())
            .unwrap()
            .is_empty());
    }

    #[test]
    fn native_owned_scalar_reuses_tracker_handle_and_rejects_timeline_ownership() {
        let mut scene = Scene::new();
        let control = scene.control_signal("opacity", 1.0).unwrap();
        assert!(scene.set_value(&control, 0.5).is_err());
        assert!(scene.play_value(&control, 0.5).run_time(1.0).is_err());
        assert_eq!(scene.time(), 0.0);
    }
}
