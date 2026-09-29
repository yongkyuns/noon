//! Derived transport residency, shared by the producer and installed receiver.
//! Incrementals visit touched slots/roots only; full snapshots replace the index.
use std::collections::HashMap;

use crate::{
    RetainedExecutionDeltaEnvelope, RetainedResourceRetirements, TransportImageResourceHandle,
    TransportObjectContent, TransportSlotId, TransportTextResourceHandle,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum ResourceRoot {
    Image(TransportImageResourceHandle),
    Text(TransportTextResourceHandle),
}

impl ResourceRoot {
    fn from_content(content: &TransportObjectContent) -> Option<Self> {
        match content {
            TransportObjectContent::Image { image, .. } => Some(Self::Image(*image)),
            TransportObjectContent::Text { text } => Some(Self::Text(*text)),
            TransportObjectContent::Geometry { .. } => None,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ResourceResidency {
    slots: HashMap<TransportSlotId, ResourceRoot>,
    references: HashMap<ResourceRoot, usize>,
}

pub(crate) struct StagedResourceResidency {
    snapshot: Option<ResourceResidency>,
    updates: HashMap<TransportSlotId, Option<ResourceRoot>>,
    deltas: HashMap<ResourceRoot, isize>,
}

impl ResourceResidency {
    pub(crate) fn stage(&self, delta: &RetainedExecutionDeltaEnvelope) -> StagedResourceResidency {
        if delta.snapshot {
            let mut snapshot = Self::default();
            for object in &delta.objects {
                if let Some(root) = ResourceRoot::from_content(&object.content) {
                    snapshot.slots.insert(object.slot, root);
                    *snapshot.references.entry(root).or_default() += 1;
                }
            }
            return StagedResourceResidency {
                snapshot: Some(snapshot),
                updates: HashMap::new(),
                deltas: HashMap::new(),
            };
        }
        let mut updates = HashMap::new();
        for slot in &delta.removed_slots {
            updates.insert(*slot, None);
        }
        for object in &delta.objects {
            updates.insert(object.slot, ResourceRoot::from_content(&object.content));
        }
        let mut deltas = HashMap::new();
        for (&slot, &next) in &updates {
            let old = self.slots.get(&slot).copied();
            if old == next {
                continue;
            }
            if let Some(root) = old {
                *deltas.entry(root).or_default() -= 1;
            }
            if let Some(root) = next {
                *deltas.entry(root).or_default() += 1;
            }
        }
        StagedResourceResidency {
            snapshot: None,
            updates,
            deltas,
        }
    }

    pub(crate) fn references_after(
        &self,
        staged: &StagedResourceResidency,
        root: ResourceRoot,
    ) -> usize {
        if let Some(snapshot) = &staged.snapshot {
            return snapshot.references.get(&root).copied().unwrap_or_default();
        }
        let count = self.references.get(&root).copied().unwrap_or_default() as isize
            + staged.deltas.get(&root).copied().unwrap_or_default();
        debug_assert!(count >= 0);
        count as usize
    }

    pub(crate) fn retirements(
        &self,
        staged: &StagedResourceResidency,
    ) -> RetainedResourceRetirements {
        let mut retired = RetainedResourceRetirements::default();
        // Snapshot replacement may touch every old root. Incrementals inspect
        // only roots whose reference count changed, regardless of scene size.
        let mut visit = |root| {
            if !self.references.contains_key(&root) || self.references_after(staged, root) > 0 {
                return;
            }
            match root {
                ResourceRoot::Image(image) => retired.images.push(image),
                ResourceRoot::Text(text) => retired.texts.push(text),
            }
        };
        if staged.snapshot.is_some() {
            for &root in self.references.keys() {
                visit(root);
            }
        } else {
            for &root in staged.deltas.keys() {
                visit(root);
            }
        }
        retired
    }

    pub(crate) fn commit(&mut self, staged: StagedResourceResidency) {
        if let Some(snapshot) = staged.snapshot {
            *self = snapshot;
            return;
        }
        for (slot, next) in staged.updates {
            match next {
                Some(root) => {
                    self.slots.insert(slot, root);
                }
                None => {
                    self.slots.remove(&slot);
                }
            }
        }
        for (root, delta) in staged.deltas {
            let count = self.references.get(&root).copied().unwrap_or_default() as isize + delta;
            debug_assert!(count >= 0);
            if count == 0 {
                self.references.remove(&root);
            } else {
                self.references.insert(root, count as usize);
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.slots.is_empty() && self.references.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_retired_root_in_a_hundred_thousand_stages_only_one_slot_and_reference() {
        let mut residency = ResourceResidency::default();
        for slot in 0..100_000 {
            let id = TransportSlotId {
                slot,
                generation: 0,
            };
            let root = ResourceRoot::Text(TransportTextResourceHandle {
                arena: 1,
                id: u64::from(slot),
                version: 1,
            });
            residency.slots.insert(id, root);
            residency.references.insert(root, 1);
        }
        let slot = TransportSlotId {
            slot: 17,
            generation: 0,
        };
        let delta = RetainedExecutionDeltaEnvelope {
            channel: crate::RETAINED_EXECUTION_TRANSPORT_CHANNEL.into(),
            protocol_version: crate::RETAINED_EXECUTION_TRANSPORT_VERSION,
            session: 1,
            sequence: 1,
            snapshot: false,
            time: 0.0,
            camera: noon_core::Camera2DState::default(),
            inset_2d_views: vec![],
            objects: vec![],
            removed_slots: vec![slot],
            painter_order: None,
        };
        let staged = residency.stage(&delta);
        assert!(staged.snapshot.is_none());
        assert_eq!(staged.updates.len(), 1);
        assert_eq!(staged.deltas.len(), 1);
        let retirements = residency.retirements(&staged);
        assert_eq!(
            retirements.texts,
            [TransportTextResourceHandle {
                arena: 1,
                id: 17,
                version: 1,
            }]
        );
        assert_eq!(residency.slots.len(), 100_000, "staging does not publish");
        residency.commit(staged);
        assert_eq!(residency.slots.len(), 99_999);
        assert_eq!(residency.references.len(), 99_999);
    }
}
