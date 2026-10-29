//! Derived transport residency, shared by the producer and installed receiver.
//! Incrementals visit touched slots/roots only; full snapshots replace the index.
use std::collections::HashMap;

use crate::{
    RetainedExecutionDeltaEnvelope, RetainedResourceRetirements, RetainedTransportObjectState,
    TransportGeometryResourceHandle, TransportImageResourceHandle, TransportObjectContent,
    TransportSlotId, TransportTextResourceHandle,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum ResourceRoot {
    Composed(crate::TransportComposedSourceHandle),
    Shader(crate::TransportShaderProgramHandle),
    Image(TransportImageResourceHandle),
    Text(TransportTextResourceHandle),
    Geometry(TransportGeometryResourceHandle),
    RenderGeometry(u64),
}

impl ResourceRoot {
    fn from_object(object: &RetainedTransportObjectState) -> Option<Self> {
        match &object.content {
            TransportObjectContent::Composed { source } => Some(Self::Composed(*source)),
            TransportObjectContent::Image { image, .. } => Some(Self::Image(*image)),
            TransportObjectContent::Text { text } => Some(Self::Text(*text)),
            TransportObjectContent::Geometry { resource, .. } => resource
                .map(Self::Geometry)
                .or_else(|| object.render_geometry_resource.map(Self::RenderGeometry)),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct ObjectRoots {
    content: [Option<ResourceRoot>; 2],
    // Added only for shader rows; changing parameter values reuses this Arc.
    shaders: Option<std::sync::Arc<Vec<crate::TransportShaderProgramHandle>>>,
}
impl From<[Option<ResourceRoot>; 2]> for ObjectRoots {
    fn from(content: [Option<ResourceRoot>; 2]) -> Self {
        Self {
            content,
            shaders: None,
        }
    }
}
impl ObjectRoots {
    fn iter(&self) -> impl Iterator<Item = ResourceRoot> + '_ {
        self.content.iter().copied().flatten().chain(
            self.shaders
                .iter()
                .flat_map(|v| v.iter().copied())
                .map(ResourceRoot::Shader),
        )
    }
    fn is_empty(&self) -> bool {
        self.iter().next().is_none()
    }
}
fn object_roots(
    object: &RetainedTransportObjectState,
    previous: Option<&ObjectRoots>,
) -> ObjectRoots {
    let programs = || {
        object
            .effects
            .iter()
            .filter_map(|effect| match effect.definition {
                crate::TransportEffectDefinition::Shader { program, .. } => Some(program),
                _ => None,
            })
    };
    let shaders = if programs().next().is_none() {
        None
    } else if let Some(old) = previous
        .and_then(|old| old.shaders.as_ref())
        .filter(|old| old.iter().copied().eq(programs()))
    {
        Some(std::sync::Arc::clone(old))
    } else {
        Some(std::sync::Arc::new(programs().collect()))
    };
    ObjectRoots {
        content: [
            ResourceRoot::from_object(object),
            object
                .fill_paint
                .as_deref()
                .and_then(noon_core::SemanticPaint::as_image)
                .map(|image| ResourceRoot::Image(image.resource().into())),
        ],
        shaders,
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ResourceResidency {
    slots: HashMap<TransportSlotId, ObjectRoots>,
    references: HashMap<ResourceRoot, usize>,
}

pub(crate) struct StagedResourceResidency {
    snapshot: Option<ResourceResidency>,
    updates: HashMap<TransportSlotId, ObjectRoots>,
    deltas: HashMap<ResourceRoot, isize>,
}

impl ResourceResidency {
    pub(crate) fn stage(&self, delta: &RetainedExecutionDeltaEnvelope) -> StagedResourceResidency {
        if delta.snapshot {
            let mut snapshot = Self::default();
            for object in &delta.objects {
                let roots = object_roots(object, None);
                if !roots.is_empty() {
                    snapshot.slots.insert(object.slot, roots.clone());
                }
                for root in roots.iter() {
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
            updates.insert(*slot, ObjectRoots::default());
        }
        for object in &delta.objects {
            let mut next = object_roots(object, self.slots.get(&object.slot));
            next.content[0] = next.content[0].or_else(|| {
                // The retained wire can omit unchanged inline geometry. Preserve
                // its installed root until a new geometry or transform clears it.
                (matches!(
                    &object.content,
                    TransportObjectContent::Geometry { resource: None, .. }
                ) && object.render_transform.is_some()
                    && object.render_geometry.is_none())
                .then(|| {
                    self.slots
                        .get(&object.slot)
                        .and_then(|roots| roots.content[0])
                })
                .flatten()
                .filter(|root| matches!(root, ResourceRoot::RenderGeometry(_)))
            });
            updates.insert(object.slot, next);
        }
        let mut deltas = HashMap::new();
        for (&slot, next) in &updates {
            let old = self.slots.get(&slot).cloned().unwrap_or_default();
            if &old == next {
                continue;
            }
            for root in old.iter() {
                *deltas.entry(root).or_default() -= 1;
            }
            for root in next.iter() {
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
                ResourceRoot::Composed(source) => retired.composed_sources.push(source),
                ResourceRoot::Shader(program) => retired.shader_programs.push(program),
                ResourceRoot::Image(image) => retired.images.push(image),
                ResourceRoot::Text(text) => retired.texts.push(text),
                ResourceRoot::Geometry(geometry) => retired.geometries.push(geometry),
                ResourceRoot::RenderGeometry(_) => {}
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
            if !next.is_empty() {
                self.slots.insert(slot, next);
            } else {
                self.slots.remove(&slot);
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
            residency.slots.insert(id, [Some(root), None].into());
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
            publication_context: noon_core::PublicationContext::default(),
            snapshot: false,
            time: 0.0,
            camera: noon_core::Camera2DState::default(),
            inset_2d_views: vec![],
            objects: vec![],
            object_patches: Vec::new(),
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
