use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use super::{
    HostCallbackReplayClass, HostReadModel, SemanticNodeId, SemanticNodeKind, SemanticStore,
};

/// Stable authored identity for one host-language callback declaration.
///
/// This identifies a declaration only; executable Python/JS/native callback code
/// remains owned by the host and is resolved after semantic lowering.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SemanticHostCallbackId(u64);

impl SemanticHostCallbackId {
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Authored host callback participation stored directly with the Semantic Scene.
///
/// Subscriptions use semantic identity, never legacy object IDs, execution slots,
/// renderer identities, or transport-local handles. Activation bounds are semantic
/// authored time: start is inclusive and end is exclusive.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SemanticHostCallbackDeclaration {
    pub id: SemanticHostCallbackId,
    subscriptions: Vec<SemanticNodeId>,
    pub replay_class: HostCallbackReplayClass,
    pub read_model: HostReadModel,
    pub active_from: Option<OrderedSemanticTime>,
    pub inactive_from: Option<OrderedSemanticTime>,
}

impl SemanticHostCallbackDeclaration {
    pub fn subscriptions(&self) -> &[SemanticNodeId] {
        &self.subscriptions
    }

    pub fn is_active_at(&self, time: f64) -> bool {
        if !time.is_finite() {
            return false;
        }
        self.active_from.is_none_or(|start| time >= start.get())
            && self.inactive_from.is_none_or(|end| time < end.get())
    }
}

/// Finite non-negative semantic authored time with total equality semantics.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OrderedSemanticTime(u64);

impl OrderedSemanticTime {
    pub fn new(value: f64) -> Option<Self> {
        (value.is_finite() && value >= 0.0).then(|| Self(value.to_bits()))
    }

    pub fn get(self) -> f64 {
        f64::from_bits(self.0)
    }
}

/// Authoritative scene-owned collection of host callback declarations.
///
/// This type owns declaration identity/order only. SemanticStore owns admission so
/// every subscribed NodeId is generation-checked before a declaration is committed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SemanticHostDeclarations {
    declarations: Vec<SemanticHostCallbackDeclaration>,
    next_id: u64,
}

impl SemanticHostDeclarations {
    pub fn declarations(&self) -> &[SemanticHostCallbackDeclaration] {
        &self.declarations
    }

    pub fn declaration(
        &self,
        id: SemanticHostCallbackId,
    ) -> Option<&SemanticHostCallbackDeclaration> {
        self.declarations.iter().find(|entry| entry.id == id)
    }

    fn register(
        &mut self,
        subscriptions: Vec<SemanticNodeId>,
        replay_class: HostCallbackReplayClass,
        read_model: HostReadModel,
        active_from: Option<OrderedSemanticTime>,
        inactive_from: Option<OrderedSemanticTime>,
    ) -> Result<SemanticHostCallbackId, SemanticHostDeclarationError> {
        if matches!((active_from, inactive_from), (Some(start), Some(end)) if end.get() < start.get())
        {
            return Err(SemanticHostDeclarationError::InvalidActivationWindow);
        }
        let id = SemanticHostCallbackId(self.next_id);
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or(SemanticHostDeclarationError::DeclarationIdExhausted)?;
        self.declarations.push(SemanticHostCallbackDeclaration {
            id,
            subscriptions,
            replay_class,
            read_model,
            active_from,
            inactive_from,
        });
        Ok(id)
    }

    pub(crate) fn remove_node_subscription(&mut self, id: SemanticNodeId) {
        for declaration in &mut self.declarations {
            declaration.subscriptions.retain(|candidate| *candidate != id);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SemanticHostDeclarationError {
    UnknownNode(SemanticNodeId),
    NotSemanticAuthoringNode(SemanticNodeId),
    InvalidActivationTime,
    InvalidActivationWindow,
    DeclarationIdExhausted,
}

impl std::fmt::Display for SemanticHostDeclarationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownNode(id) => write!(
                formatter,
                "unknown semantic node {}:{}",
                id.slot(),
                id.generation()
            ),
            Self::NotSemanticAuthoringNode(id) => write!(
                formatter,
                "semantic node {}:{} is not a target semantic object or family",
                id.slot(),
                id.generation()
            ),
            Self::InvalidActivationTime => {
                formatter.write_str("host declaration activation times must be finite and non-negative")
            }
            Self::InvalidActivationWindow => {
                formatter.write_str("host declaration end time precedes its start time")
            }
            Self::DeclarationIdExhausted => {
                formatter.write_str("Noon semantic host declaration ID space exhausted")
            }
        }
    }
}

impl std::error::Error for SemanticHostDeclarationError {}

impl SemanticStore {
    pub fn semantic_host_declarations(&self) -> &SemanticHostDeclarations {
        &self.host_declarations
    }

    /// Declare host-language callback participation against semantic identities.
    ///
    /// Admission is atomic: all subscriptions and activation bounds are validated
    /// before declaration identity is allocated or scene declaration state changes.
    pub fn declare_host_callback(
        &mut self,
        subscriptions: impl IntoIterator<Item = SemanticNodeId>,
        replay_class: HostCallbackReplayClass,
        read_model: HostReadModel,
        active_from: Option<f64>,
        inactive_from: Option<f64>,
    ) -> Result<SemanticHostCallbackId, SemanticHostDeclarationError> {
        let active_from = validate_time(active_from)?;
        let inactive_from = validate_time(inactive_from)?;
        if matches!((active_from, inactive_from), (Some(start), Some(end)) if end.get() < start.get())
        {
            return Err(SemanticHostDeclarationError::InvalidActivationWindow);
        }

        let mut seen = HashSet::new();
        let mut validated = Vec::new();
        for id in subscriptions {
            let node = self
                .node(id)
                .ok_or(SemanticHostDeclarationError::UnknownNode(id))?;
            let is_target = match node.kind() {
                SemanticNodeKind::Family => true,
                SemanticNodeKind::AuthoringObject => node.semantic_object_state().is_some(),
                SemanticNodeKind::Object(_) => false,
            };
            if !is_target {
                return Err(SemanticHostDeclarationError::NotSemanticAuthoringNode(id));
            }
            if seen.insert(id) {
                validated.push(id);
            }
        }

        self.host_declarations.register(
            validated,
            replay_class,
            read_model,
            active_from,
            inactive_from,
        )
    }
}

fn validate_time(
    value: Option<f64>,
) -> Result<Option<OrderedSemanticTime>, SemanticHostDeclarationError> {
    value
        .map(|value| {
            OrderedSemanticTime::new(value).ok_or(SemanticHostDeclarationError::InvalidActivationTime)
        })
        .transpose()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SemanticObjectState, StoredGeometry};

    fn object(store: &mut SemanticStore, radius: f32) -> SemanticNodeId {
        store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle { radius }))
    }

    #[test]
    fn declarations_use_semantic_identity_and_preserve_subscription_order() {
        let mut store = SemanticStore::new();
        let first = object(&mut store, 1.0);
        let second = object(&mut store, 2.0);
        let family = store.insert_family();

        let id = store
            .declare_host_callback(
                [first, first, family, second],
                HostCallbackReplayClass::Pure,
                HostReadModel::DeclaredSnapshot,
                Some(1.0),
                Some(3.0),
            )
            .unwrap();
        let declaration = store.semantic_host_declarations().declaration(id).unwrap();

        assert_eq!(id.get(), 0);
        assert_eq!(declaration.subscriptions(), &[first, family, second]);
        assert_eq!(declaration.replay_class, HostCallbackReplayClass::Pure);
        assert_eq!(declaration.read_model, HostReadModel::DeclaredSnapshot);
        assert!(!declaration.is_active_at(1.0 - f64::EPSILON));
        assert!(declaration.is_active_at(1.0));
        assert!(!declaration.is_active_at(3.0));
    }

    #[test]
    fn invalid_declaration_is_rejected_before_identity_allocation() {
        let mut store = SemanticStore::new();
        let object = object(&mut store, 1.0);
        assert_eq!(
            store.declare_host_callback(
                [object],
                HostCallbackReplayClass::Opaque,
                HostReadModel::EngineLocalSemanticView,
                Some(2.0),
                Some(1.0),
            ),
            Err(SemanticHostDeclarationError::InvalidActivationWindow)
        );
        assert!(store.semantic_host_declarations().declarations().is_empty());

        let id = store
            .declare_host_callback(
                [object],
                HostCallbackReplayClass::Opaque,
                HostReadModel::EngineLocalSemanticView,
                None,
                None,
            )
            .unwrap();
        assert_eq!(id.get(), 0);
    }

    #[test]
    fn stale_and_state_less_handles_fail_at_declaration_boundary() {
        let mut store = SemanticStore::new();
        let stale = object(&mut store, 1.0);
        store.remove_node(stale).unwrap();
        let replacement = object(&mut store, 2.0);
        assert_eq!(stale.slot(), replacement.slot());
        assert_ne!(stale.generation(), replacement.generation());
        assert_eq!(
            store.declare_host_callback(
                [stale],
                HostCallbackReplayClass::Pure,
                HostReadModel::DeclaredSnapshot,
                None,
                None,
            ),
            Err(SemanticHostDeclarationError::UnknownNode(stale))
        );

        let identity_only = store.insert_authoring_object();
        assert_eq!(
            store.declare_host_callback(
                [identity_only],
                HostCallbackReplayClass::Pure,
                HostReadModel::DeclaredSnapshot,
                None,
                None,
            ),
            Err(SemanticHostDeclarationError::NotSemanticAuthoringNode(
                identity_only
            ))
        );
        assert!(store.semantic_host_declarations().declarations().is_empty());
    }

    #[test]
    fn detach_preserves_declarations_and_delete_cleans_only_removed_subscription() {
        let mut store = SemanticStore::new();
        let first = object(&mut store, 1.0);
        let second = object(&mut store, 2.0);
        let id = store
            .declare_host_callback(
                [first, second],
                HostCallbackReplayClass::StatefulDeterministic,
                HostReadModel::EngineLocalSemanticView,
                None,
                None,
            )
            .unwrap();

        store.attach_semantic_object(first).unwrap();
        store.detach_semantic_object(first).unwrap();
        assert_eq!(
            store.semantic_host_declarations().declaration(id).unwrap().subscriptions(),
            &[first, second]
        );

        store.remove_node(first).unwrap();
        assert_eq!(
            store.semantic_host_declarations().declaration(id).unwrap().subscriptions(),
            &[second]
        );
    }
}
