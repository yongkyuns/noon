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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HostCallbackSlot {
    pub id: HostCallbackId,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub objects: Vec<ObjectId>,
    /// Registration time. The callback is active at this exact time.
    ///
    /// The serialized field name is retained for wire compatibility even though
    /// Manim updater semantics use an inclusive start boundary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_after: Option<f64>,
    /// Removal time. The callback is inactive at this exact time.
    ///
    /// The serialized field name is retained for wire compatibility even though
    /// Manim updater semantics use an exclusive end boundary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_through: Option<f64>,
}

impl HostCallbackSlot {
    pub fn is_active_at(&self, time: f64) -> bool {
        time.is_finite()
            && self.active_after.is_none_or(|start| time >= start)
            && self.active_through.is_none_or(|end| time < end)
    }

    fn validate_schedule(&self) -> Result<(), HostCallbackRegistryError> {
        if self
            .active_after
            .is_some_and(|value| !value.is_finite() || value < 0.0)
            || self
                .active_through
                .is_some_and(|value| !value.is_finite() || value < 0.0)
            || matches!((self.active_after, self.active_through), (Some(start), Some(end)) if end < start)
        {
            return Err(HostCallbackRegistryError::InvalidActivationWindow(self.id));
        }
        Ok(())
    }

    fn deduplicate_objects(&mut self) {
        let mut seen = BTreeSet::new();
        self.objects.retain(|object| seen.insert(*object));
    }
}

/// Declarative host-dynamic participation for a semantic scene.
///
/// Slots contain no executable code. A Python/JS/native host owns the callable
/// keyed by `HostCallbackId`; the runtime owns when the callback phase is required
/// and which object state is captured for that phase.
#[derive(Clone, Debug, Default, PartialEq)]
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

    pub fn from_slots(mut slots: Vec<HostCallbackSlot>) -> Result<Self, HostCallbackRegistryError> {
        let mut ids = BTreeSet::new();
        let mut next_callback_id = 0;
        for slot in &mut slots {
            slot.validate_schedule()?;
            if !ids.insert(slot.id) {
                return Err(HostCallbackRegistryError::DuplicateCallback(slot.id));
            }
            slot.deduplicate_objects();
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

    pub fn register(&mut self, objects: impl IntoIterator<Item = ObjectId>) -> HostCallbackId {
        let id = HostCallbackId::new(self.next_callback_id);
        self.next_callback_id = self
            .next_callback_id
            .checked_add(1)
            .expect("Noon host callback ID space exhausted");
        let mut slot = HostCallbackSlot {
            id,
            objects: objects.into_iter().collect(),
            active_after: None,
            active_through: None,
        };
        slot.deduplicate_objects();
        self.slots.push(slot);
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
    InvalidActivationWindow(HostCallbackId),
    CallbackIdExhausted,
}

impl std::fmt::Display for HostCallbackRegistryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateCallback(id) => {
                write!(formatter, "duplicate host callback id {}", id.get())
            }
            Self::InvalidActivationWindow(id) => write!(
                formatter,
                "host callback {} has an invalid activation window",
                id.get()
            ),
            Self::CallbackIdExhausted => {
                formatter.write_str("Noon host callback ID space exhausted")
            }
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
        assert_eq!(
            registry.register([first, first, second]),
            HostCallbackId::new(0)
        );
        assert_eq!(registry.register([]), HostCallbackId::new(1));
        assert_eq!(registry.slots()[0].objects, vec![first, second]);
    }

    #[test]
    fn local_and_transported_subscriptions_share_canonical_order() {
        let first = ObjectId::new(4);
        let second = ObjectId::new(7);
        let third = ObjectId::new(9);
        let objects = vec![second, first, second, third, first, third];

        let mut local = HostCallbackRegistry::new();
        local.register(objects.clone());
        let transported = HostCallbackRegistry::from_slots(vec![HostCallbackSlot {
            id: HostCallbackId::new(0),
            objects,
            active_after: None,
            active_through: None,
        }])
        .unwrap();

        assert_eq!(local.slots()[0].objects, vec![second, first, third]);
        assert_eq!(transported.slots()[0].objects, local.slots()[0].objects);
    }

    #[test]
    fn scheduled_slot_uses_inclusive_start_and_exclusive_end() {
        let slot = HostCallbackSlot {
            id: HostCallbackId::new(2),
            objects: vec![],
            active_after: Some(1.0),
            active_through: Some(2.0),
        };
        HostCallbackRegistry::from_slots(vec![slot.clone()]).unwrap();
        assert!(!slot.is_active_at(1.0 - f64::EPSILON));
        assert!(slot.is_active_at(1.0));
        assert!(slot.is_active_at(2.0 - f64::EPSILON));
        assert!(!slot.is_active_at(2.0));
    }

    #[test]
    fn zero_width_slot_is_never_active() {
        let slot = HostCallbackSlot {
            id: HostCallbackId::new(4),
            objects: vec![],
            active_after: Some(0.0),
            active_through: Some(0.0),
        };
        HostCallbackRegistry::from_slots(vec![slot.clone()]).unwrap();
        assert!(!slot.is_active_at(0.0));
        assert!(!slot.is_active_at(f64::EPSILON));
    }

    #[test]
    fn transported_slots_reject_invalid_activation_windows() {
        let slot = HostCallbackSlot {
            id: HostCallbackId::new(5),
            objects: vec![],
            active_after: Some(3.0),
            active_through: Some(2.0),
        };
        assert_eq!(
            HostCallbackRegistry::from_slots(vec![slot]),
            Err(HostCallbackRegistryError::InvalidActivationWindow(
                HostCallbackId::new(5)
            ))
        );
    }

    #[test]
    fn transported_slots_reject_duplicate_callback_ids() {
        let slot = HostCallbackSlot {
            id: HostCallbackId::new(3),
            objects: vec![ObjectId::new(1)],
            active_after: None,
            active_through: None,
        };
        assert_eq!(
            HostCallbackRegistry::from_slots(vec![slot.clone(), slot]),
            Err(HostCallbackRegistryError::DuplicateCallback(
                HostCallbackId::new(3)
            ))
        );
    }
}
