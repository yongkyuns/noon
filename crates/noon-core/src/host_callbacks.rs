use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use crate::ObjectId;

/// Stable language-neutral identity for one host callback slot.
///
/// The callback implementation itself stays in the host language. Noon transports
/// only this identity plus the semantic objects whose coherent frame state the
/// callback needs to observe.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct HostCallbackId(u64);

impl HostCallbackId {
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostCallbackSlot {
    pub id: HostCallbackId,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub objects: Vec<ObjectId>,
}

/// Declarative host-dynamic participation for a semantic scene.
///
/// Slots contain no executable code. A Python/JS/native host owns the callable
/// keyed by `HostCallbackId`; the runtime owns when the callback phase is required
/// and which object state is captured for that phase.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HostCallbackRegistry {
    slots: Vec<HostCallbackSlot>,
    next_callback_id: u64,
}

impl HostCallbackRegistry {
    pub const fn new() -> Self {
        Self {
            slots: Vec::new(),
            next_callback_id: 0,
        }
    }

    pub fn from_slots(slots: Vec<HostCallbackSlot>) -> Result<Self, HostCallbackRegistryError> {
        let mut ids = BTreeSet::new();
        let mut next_callback_id = 0;
        for slot in &slots {
            if !ids.insert(slot.id) {
                return Err(HostCallbackRegistryError::DuplicateCallback(slot.id));
            }
            next_callback_id = next_callback_id.max(
                slot.id
                    .get()
                    .checked_add(1)
                    .ok_or(HostCallbackRegistryError::CallbackIdExhausted)?,
            );
        }
        Ok(Self {
            slots,
            next_callback_id,
        })
    }

    pub fn register(
        &mut self,
        objects: impl IntoIterator<Item = ObjectId>,
    ) -> HostCallbackId {
        let id = HostCallbackId::new(self.next_callback_id);
        self.next_callback_id = self
            .next_callback_id
            .checked_add(1)
            .expect("Noon host callback ID space exhausted");
        let mut unique = Vec::new();
        for object in objects {
            if !unique.contains(&object) {
                unique.push(object);
            }
        }
        self.slots.push(HostCallbackSlot {
            id,
            objects: unique,
        });
        id
    }

    pub fn slots(&self) -> &[HostCallbackSlot] {
        &self.slots
    }

    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostCallbackRegistryError {
    DuplicateCallback(HostCallbackId),
    CallbackIdExhausted,
}

impl std::fmt::Display for HostCallbackRegistryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateCallback(id) => {
                write!(formatter, "duplicate host callback id {}", id.get())
            }
            Self::CallbackIdExhausted => formatter.write_str("Noon host callback ID space exhausted"),
        }
    }
}

impl std::error::Error for HostCallbackRegistryError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn callback_ids_are_stable_and_object_subscriptions_are_deduplicated() {
        let mut registry = HostCallbackRegistry::new();
        let first = ObjectId::new(4);
        let second = ObjectId::new(7);
        assert_eq!(registry.register([first, first, second]), HostCallbackId::new(0));
        assert_eq!(registry.register([]), HostCallbackId::new(1));
        assert_eq!(registry.slots()[0].objects, vec![first, second]);
    }

    #[test]
    fn transported_slots_reject_duplicate_callback_ids() {
        let slot = HostCallbackSlot {
            id: HostCallbackId::new(3),
            objects: vec![ObjectId::new(1)],
        };
        assert_eq!(
            HostCallbackRegistry::from_slots(vec![slot.clone(), slot]),
            Err(HostCallbackRegistryError::DuplicateCallback(HostCallbackId::new(3)))
        );
    }
}
