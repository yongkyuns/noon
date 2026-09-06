use super::{
    SemanticNodeId, SemanticObjectProperty, SemanticSceneOperationError, SemanticSignalBinding,
    SemanticSignalError, SemanticSignalValueKind, SemanticStore,
};

/// Failure while authoring one native-reactive signal-to-property binding.
#[derive(Clone, Debug, PartialEq)]
pub enum SemanticSignalBindingError {
    Signal(SemanticSignalError),
    Target(SemanticSceneOperationError),
    TypeMismatch {
        signal: SemanticNodeId,
        target: SemanticNodeId,
        property: SemanticObjectProperty,
        expected: SemanticSignalValueKind,
        actual: SemanticSignalValueKind,
    },
    PropertyAlreadyBound {
        target: SemanticNodeId,
        property: SemanticObjectProperty,
        existing_signal: SemanticNodeId,
    },
}

impl std::fmt::Display for SemanticSignalBindingError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Signal(error) => error.fmt(formatter),
            Self::Target(error) => error.fmt(formatter),
            Self::TypeMismatch {
                signal,
                target,
                property,
                expected,
                actual,
            } => write!(
                formatter,
                "semantic signal {}:{} is {actual}, but {:?} on target {}:{} requires {expected}",
                signal.slot(),
                signal.generation(),
                property,
                target.slot(),
                target.generation()
            ),
            Self::PropertyAlreadyBound {
                target,
                property,
                existing_signal,
            } => write!(
                formatter,
                "semantic property {:?} on target {}:{} is already bound to signal {}:{}",
                property,
                target.slot(),
                target.generation(),
                existing_signal.slot(),
                existing_signal.generation()
            ),
        }
    }
}

impl std::error::Error for SemanticSignalBindingError {}

impl From<SemanticSignalError> for SemanticSignalBindingError {
    fn from(value: SemanticSignalError) -> Self {
        Self::Signal(value)
    }
}

impl From<SemanticSceneOperationError> for SemanticSignalBindingError {
    fn from(value: SemanticSceneOperationError) -> Self {
        Self::Target(value)
    }
}

impl SemanticStore {
    /// Return authored signal bindings for one target semantic object.
    ///
    /// The declaration lives with target semantic object state. Lowering may map
    /// the signal/property pair to runtime slots later; no runtime slot or legacy
    /// `Property`/`SignalId` identity is stored here.
    pub fn semantic_object_signal_bindings(
        &self,
        target: SemanticNodeId,
    ) -> Result<&[SemanticSignalBinding], SemanticSignalBindingError> {
        Ok(self
            .semantic_object_state_checked(target)?
            .signal_bindings())
    }

    /// Bind one semantic signal to one typed semantic object property.
    ///
    /// One target property has at most one authored driver. Validation happens
    /// before mutation and touches only the source signal plus target object.
    pub fn bind_semantic_signal(
        &mut self,
        signal: SemanticNodeId,
        target: SemanticNodeId,
        property: SemanticObjectProperty,
    ) -> Result<bool, SemanticSignalBindingError> {
        self.set_last_mutation_writes(0);
        let actual = self.semantic_signal_value_kind(signal)?;
        let expected = property.value_kind();
        let state = self.semantic_object_state_checked(target)?;

        if actual != expected {
            return Err(SemanticSignalBindingError::TypeMismatch {
                signal,
                target,
                property,
                expected,
                actual,
            });
        }

        if let Some(existing) = state
            .signal_bindings()
            .iter()
            .find(|binding| binding.property() == property)
            .copied()
        {
            if existing.signal() == signal {
                return Ok(false);
            }
            return Err(SemanticSignalBindingError::PropertyAlreadyBound {
                target,
                property,
                existing_signal: existing.signal(),
            });
        }

        self.node_mut(target)
            .and_then(|node| node.semantic_object_state_mut())
            .expect("semantic binding target validated before mutation")
            .signal_bindings_mut()
            .push(SemanticSignalBinding::new(signal, property));
        self.register_semantic_references_for_owner(target);
        self.set_last_mutation_writes(1);
        Ok(true)
    }

    /// Remove the authored driver for one target property, if present.
    ///
    /// Removal is keyed by target/property rather than source identity so stale
    /// generation-safe bindings can still be explicitly cleaned. A1.5 `RemoveNode`
    /// owns automatic reverse cleanup when source node deletion becomes a semantic
    /// transaction instead of the current low-level storage primitive.
    pub fn remove_semantic_signal_binding(
        &mut self,
        target: SemanticNodeId,
        property: SemanticObjectProperty,
    ) -> Result<Option<SemanticSignalBinding>, SemanticSignalBindingError> {
        self.set_last_mutation_writes(0);
        let state = self.semantic_object_state_checked(target)?;
        let Some(index) = state
            .signal_bindings()
            .iter()
            .position(|binding| binding.property() == property)
        else {
            return Ok(None);
        };

        self.unregister_semantic_references_for_owner(target);
        let removed = self
            .node_mut(target)
            .and_then(|node| node.semantic_object_state_mut())
            .expect("semantic binding target validated before mutation")
            .signal_bindings_mut()
            .remove(index);
        self.register_semantic_references_for_owner(target);
        self.set_last_mutation_writes(1);
        Ok(Some(removed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SemanticObjectState, SemanticVec3, StoredGeometry};

    fn object(store: &mut SemanticStore, radius: f32) -> SemanticNodeId {
        store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle { radius }))
    }

    #[test]
    fn scalar_and_vector_signals_bind_only_to_matching_semantic_properties() {
        let mut store = SemanticStore::new();
        let scalar = store.insert_semantic_input_signal(0.5_f64).unwrap();
        let vector = store
            .insert_semantic_input_signal(SemanticVec3::new(1.0, 2.0, 3.0))
            .unwrap();
        let boolean = store.insert_semantic_input_signal(true).unwrap();
        let target = object(&mut store, 1.0);

        assert!(store
            .bind_semantic_signal(vector, target, SemanticObjectProperty::Translation)
            .unwrap());
        assert!(store
            .bind_semantic_signal(scalar, target, SemanticObjectProperty::ObjectOpacity)
            .unwrap());
        assert_eq!(store.last_mutation_stats().slots_written, 1);
        assert!(store
            .bind_semantic_signal(boolean, target, SemanticObjectProperty::Presence)
            .unwrap());
        assert_eq!(store.last_mutation_stats().slots_written, 1);

        assert!(matches!(
            store.bind_semantic_signal(scalar, target, SemanticObjectProperty::Scale),
            Err(SemanticSignalBindingError::TypeMismatch {
                expected: SemanticSignalValueKind::Vec3,
                actual: SemanticSignalValueKind::Scalar,
                ..
            })
        ));
        assert_eq!(store.last_mutation_stats().slots_written, 0);

        assert!(matches!(
            store.bind_semantic_signal(boolean, target, SemanticObjectProperty::StrokeWidth),
            Err(SemanticSignalBindingError::TypeMismatch {
                expected: SemanticSignalValueKind::Scalar,
                actual: SemanticSignalValueKind::Bool,
                ..
            })
        ));
        assert_eq!(store.last_mutation_stats().slots_written, 0);
    }

    #[test]
    fn one_target_property_has_one_ordered_authored_driver() {
        let mut store = SemanticStore::new();
        let first = store.insert_semantic_input_signal(0.25_f64).unwrap();
        let second = store.insert_semantic_input_signal(0.75_f64).unwrap();
        let target = object(&mut store, 1.0);

        assert!(store
            .bind_semantic_signal(first, target, SemanticObjectProperty::ObjectOpacity)
            .unwrap());
        assert!(!store
            .bind_semantic_signal(first, target, SemanticObjectProperty::ObjectOpacity)
            .unwrap());
        assert_eq!(store.last_mutation_stats().slots_written, 0);

        assert_eq!(
            store.bind_semantic_signal(second, target, SemanticObjectProperty::ObjectOpacity),
            Err(SemanticSignalBindingError::PropertyAlreadyBound {
                target,
                property: SemanticObjectProperty::ObjectOpacity,
                existing_signal: first,
            })
        );
        assert_eq!(store.last_mutation_stats().slots_written, 0);
        assert_eq!(
            store.semantic_object_signal_bindings(target).unwrap(),
            &[SemanticSignalBinding::new(
                first,
                SemanticObjectProperty::ObjectOpacity,
            )]
        );
    }

    #[test]
    fn binding_rejects_stale_non_signal_and_non_object_endpoints_before_mutation() {
        let mut store = SemanticStore::new();
        let stale = store.insert_semantic_input_signal(1.0_f64).unwrap();
        store.remove_node(stale).unwrap();
        let non_signal = object(&mut store, 1.0);
        let target = object(&mut store, 2.0);
        let family = store.insert_family();

        assert!(matches!(
            store.bind_semantic_signal(stale, target, SemanticObjectProperty::ObjectOpacity),
            Err(SemanticSignalBindingError::Signal(SemanticSignalError::UnknownSignal(id))) if id == stale
        ));
        assert_eq!(store.last_mutation_stats().slots_written, 0);

        assert!(matches!(
            store.bind_semantic_signal(non_signal, target, SemanticObjectProperty::ObjectOpacity),
            Err(SemanticSignalBindingError::Signal(SemanticSignalError::NotSignal(id))) if id == non_signal
        ));
        assert_eq!(store.last_mutation_stats().slots_written, 0);

        let scalar = store.insert_semantic_input_signal(1.0_f64).unwrap();
        assert!(matches!(
            store.bind_semantic_signal(scalar, family, SemanticObjectProperty::ObjectOpacity),
            Err(SemanticSignalBindingError::Target(
                SemanticSceneOperationError::NotSemanticObject(id)
            )) if id == family
        ));
        assert_eq!(store.last_mutation_stats().slots_written, 0);
    }

    #[test]
    fn binding_mutation_is_local_with_large_unrelated_scene_state() {
        let mut store = SemanticStore::new();
        for index in 0..10_000 {
            object(&mut store, index as f32 + 1.0);
        }
        let signal = store
            .insert_semantic_input_signal(SemanticVec3::new(3.0, 4.0, 5.0))
            .unwrap();
        let target = object(&mut store, 0.5);

        assert!(store
            .bind_semantic_signal(signal, target, SemanticObjectProperty::Translation)
            .unwrap());
        assert_eq!(store.last_mutation_stats().slots_written, 1);
        assert_eq!(
            store.semantic_object_signal_bindings(target).unwrap(),
            &[SemanticSignalBinding::new(
                signal,
                SemanticObjectProperty::Translation,
            )]
        );

        assert_eq!(
            store
                .remove_semantic_signal_binding(target, SemanticObjectProperty::Translation)
                .unwrap(),
            Some(SemanticSignalBinding::new(
                signal,
                SemanticObjectProperty::Translation,
            ))
        );
        assert_eq!(store.last_mutation_stats().slots_written, 1);
        assert!(store
            .semantic_object_signal_bindings(target)
            .unwrap()
            .is_empty());
        assert_eq!(
            store
                .remove_semantic_signal_binding(target, SemanticObjectProperty::Translation)
                .unwrap(),
            None
        );
        assert_eq!(store.last_mutation_stats().slots_written, 0);
    }

    #[test]
    fn deleted_signal_never_retargets_existing_binding_after_slot_reuse() {
        // Temporary A1.4/A1.5 seam: raw storage-level signal deletion may leave
        // a stale binding declaration until A1.5 `RemoveNode` owns reverse cleanup.
        // Generational identity must still make that declaration fail closed.
        let mut store = SemanticStore::new();
        let signal = store.insert_semantic_input_signal(0.5_f64).unwrap();
        let target = object(&mut store, 1.0);
        store
            .bind_semantic_signal(signal, target, SemanticObjectProperty::ObjectOpacity)
            .unwrap();

        store.remove_node(signal).unwrap();
        let replacement = store.insert_semantic_input_signal(0.75_f64).unwrap();
        assert_eq!(signal.slot(), replacement.slot());
        assert_ne!(signal.generation(), replacement.generation());

        let binding = store.semantic_object_signal_bindings(target).unwrap()[0];
        assert_eq!(binding.signal(), signal);
        assert_eq!(
            store.semantic_signal_value_kind(binding.signal()),
            Err(SemanticSignalError::UnknownSignal(signal))
        );

        assert_eq!(
            store
                .remove_semantic_signal_binding(target, SemanticObjectProperty::ObjectOpacity)
                .unwrap(),
            Some(binding)
        );
        assert_eq!(store.last_mutation_stats().slots_written, 1);
    }

    #[test]
    fn semantic_object_copy_carries_its_authored_signal_bindings() {
        let mut store = SemanticStore::new();
        let signal = store.insert_semantic_input_signal(0.5_f64).unwrap();
        let target = object(&mut store, 1.0);
        store
            .bind_semantic_signal(signal, target, SemanticObjectProperty::ObjectOpacity)
            .unwrap();

        let copy = store.copy_semantic_object(target).unwrap();
        assert_eq!(
            store.semantic_object_signal_bindings(copy).unwrap(),
            store.semantic_object_signal_bindings(target).unwrap()
        );
    }
}
