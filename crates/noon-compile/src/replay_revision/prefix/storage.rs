//! Explicit storage accounting and maintenance for historical execution.
use super::{CompiledReplayRevision, CompiledScene, CompiledTrack, SavedRow};
use crate::{CompiledChannelKey, CompiledObject};
use noon_core::TrackId;

/// The three Vec backing stores only. Inline headers are charged by the outer
/// revision index; indirect/shared allocations require separate accounting.
fn revision_payload_bytes(rows: usize, channels: usize, tracks: usize) -> Option<usize> {
    rows.checked_mul(std::mem::size_of::<SavedRow>())?
        .checked_add(channels.checked_mul(std::mem::size_of::<CompiledChannelKey>())?)?
        .checked_add(tracks.checked_mul(std::mem::size_of::<(TrackId, Option<CompiledTrack>)>())?)
}

impl CompiledReplayRevision {
    /// Required row/channel/track backing bytes for a clone or forward revision.
    /// These operations preserve occupied lengths, not source spare capacity.
    /// This is a pre-allocation request, not a bound on allocator over-allocation.
    pub fn prefix_clone_payload_bytes(&self) -> Option<usize> {
        revision_payload_bytes(self.rows.len(), self.channels.len(), self.tracks.len())
    }

    /// Reported row/channel/track Vec capacity times the target's inline sizes.
    /// Revision exchange only swaps elements; it cannot resize these buffers.
    /// Excludes the outer index, object/track storage in the active projection,
    /// indirect Box/Arc payloads, allocator overhead and temporary allocations.
    pub fn prefix_payload_bytes(&self) -> Option<usize> {
        revision_payload_bytes(
            self.rows.capacity(),
            self.channels.capacity(),
            self.tracks.capacity(),
        )
    }
}

impl CompiledScene {
    /// Requested backing bytes of the five object/order vectors in a clone.
    /// `minimum` reserves room for historical rows even when currently inactive.
    /// Source spare capacity is not copied. Indirect payloads/maps are excluded.
    pub fn prefix_object_clone_bytes(&self, minimum: usize) -> Option<usize> {
        self.prefix_object_vector_bytes(minimum, false)
    }

    /// Reported object/order capacities, or their requested minimum after growth.
    /// Fixed-size arithmetic only; this is not allocator/whole-scene accounting.
    pub fn prefix_object_reserved_bytes(&self, minimum: usize) -> Option<usize> {
        self.prefix_object_vector_bytes(minimum, true)
    }

    fn prefix_object_vector_bytes(&self, minimum: usize, spare: bool) -> Option<usize> {
        fn bytes<T>(values: &Vec<T>, minimum: usize, spare: bool) -> Option<usize> {
            let slots = if spare {
                values.capacity()
            } else {
                values.len()
            };
            slots.max(minimum).checked_mul(std::mem::size_of::<T>())
        }
        bytes(&self.objects, minimum, spare)?
            .checked_add(bytes(&self.family_ranks, minimum, spare)?)?
            .checked_add(bytes(&self.painter_ranks, minimum, spare)?)?
            .checked_add(bytes(&self.family_order, minimum, spare)?)?
            .checked_add(bytes(&self.painter_order, minimum, spare)?)
    }

    /// Named O(existing storage) capacity barrier. Never called implicitly from
    /// prefix append or ordinary local mutation. No logical state is changed.
    pub fn reserve_prefix_object_capacity(
        &mut self,
        total: usize,
    ) -> Result<(), std::collections::TryReserveError> {
        self.objects
            .try_reserve_exact(total.saturating_sub(self.objects.len()))?;
        self.family_ranks
            .try_reserve_exact(total.saturating_sub(self.family_ranks.len()))?;
        self.painter_ranks
            .try_reserve_exact(total.saturating_sub(self.painter_ranks.len()))?;
        self.family_order
            .try_reserve_exact(total.saturating_sub(self.family_order.len()))?;
        self.painter_order
            .try_reserve_exact(total.saturating_sub(self.painter_order.len()))?;
        Ok(())
    }
    /// Install source-derived tombstones, not visible membership. All identity,
    /// stream, dependency and capacity checks precede this infallible commit.
    pub fn install_prefix_tombstones(&mut self, objects: Vec<CompiledObject>) {
        for mut object in objects {
            let index = self.objects.len() as u32;
            object.live = false;
            object.dynamic = Default::default();
            self.retired_object_indices.insert(object.id, index);
            self.objects.push(object);
            self.family_ranks.push(None);
            self.painter_ranks.push(None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use noon_core::Property;

    #[test]
    fn revision_payload_accounting_distinguishes_clone_lengths_from_capacity() {
        let mut revision = CompiledReplayRevision {
            rows: Vec::with_capacity(3),
            inset_2d_view: None,
            channels: Vec::with_capacity(4),
            tracks: Vec::with_capacity(5),
            camera_motion_payloads: 0,
            path_motion_commands: 0,
            sampled_motion_keys: 0,
            reveal_phase_keys: 0,
        };
        assert_eq!(revision.prefix_clone_payload_bytes(), Some(0));
        let reserved = revision_payload_bytes(
            revision.rows.capacity(),
            revision.channels.capacity(),
            revision.tracks.capacity(),
        )
        .unwrap();
        assert!(reserved > 0);
        assert_eq!(revision.prefix_payload_bytes(), Some(reserved));
        revision.rows.push(SavedRow {
            index: 0,
            object: None,
            order: Some(None),
        });
        revision
            .channels
            .push(CompiledChannelKey::new(0, Property::Opacity));
        assert_eq!(
            revision.prefix_clone_payload_bytes(),
            revision_payload_bytes(1, 1, 0)
        );
        assert_eq!(revision.prefix_payload_bytes(), Some(reserved));
        // An absent saved object still occupies its entire inline row slot.
        assert!(revision.prefix_clone_payload_bytes().unwrap() > 0);
    }

    #[test]
    fn revision_payload_accounting_rejects_multiplication_and_addition_overflow() {
        assert_eq!(revision_payload_bytes(0, 0, 0), Some(0));
        assert_eq!(revision_payload_bytes(usize::MAX, 0, 0), None);
        assert_eq!(revision_payload_bytes(0, usize::MAX, 0), None);
        assert_eq!(revision_payload_bytes(0, 0, usize::MAX), None);
        assert_eq!(
            revision_payload_bytes(
                usize::MAX / std::mem::size_of::<SavedRow>(),
                usize::MAX / std::mem::size_of::<CompiledChannelKey>(),
                0,
            ),
            None
        );
    }

    #[test]
    fn object_vector_accounting_includes_all_five_vectors_and_spare_capacity() {
        use noon_core::{GeometryRef, ObjectId, Style, Transform2D};
        let object = CompiledObject::new(
            ObjectId::new(0),
            GeometryRef::circle(0.5),
            Transform2D::IDENTITY,
            Style::default(),
        );
        let mut scene = CompiledScene::compile_objects(vec![object], &[]).unwrap();
        let cloned = scene.prefix_object_clone_bytes(0);
        scene.reserve_prefix_object_capacity(8).unwrap();
        fn bytes<T>(values: &Vec<T>) -> usize {
            values.capacity() * std::mem::size_of::<T>()
        }
        let expected = bytes(&scene.objects)
            + bytes(&scene.family_ranks)
            + bytes(&scene.painter_ranks)
            + bytes(&scene.family_order)
            + bytes(&scene.painter_order);
        assert_eq!(scene.prefix_object_reserved_bytes(0), Some(expected));
        assert_eq!(scene.prefix_object_clone_bytes(0), cloned);
        assert!(expected > cloned.unwrap());
        assert_eq!(scene.prefix_object_reserved_bytes(8), Some(expected));
    }

    #[test]
    fn object_vector_request_overflow_is_rejected_without_reserving() {
        let scene = CompiledScene::compile_objects(Vec::new(), &[]).unwrap();
        let before = scene.prefix_object_reserved_bytes(0);
        assert_eq!(scene.prefix_object_clone_bytes(0), Some(0));
        assert_eq!(scene.prefix_object_clone_bytes(usize::MAX), None);
        assert_eq!(scene.prefix_object_reserved_bytes(usize::MAX), None);
        assert_eq!(scene.prefix_object_reserved_bytes(0), before);
    }

    #[test]
    fn ordered_effect_rows_remain_outside_prefix_suffix_admission() {
        use crate::{CompiledEffect, CompiledEffectStack, CompiledObject, ExecutionPatch};
        use noon_core::{
            Color, EffectDefinition, GeometryRef, Glow, ObjectId, SemanticNodeId, Style,
            Transform2D,
        };
        let id = ObjectId::new(0);
        let effects = std::sync::Arc::new(
            CompiledEffectStack::new([
                CompiledEffect {
                    attachment: SemanticNodeId::new(7, 1),
                    definition: EffectDefinition::Glow(Glow::default()),
                },
                CompiledEffect {
                    attachment: SemanticNodeId::new(8, 2),
                    definition: EffectDefinition::Glow(Glow::default()),
                },
            ])
            .unwrap(),
        );
        let mut row = CompiledObject::new(
            id,
            GeometryRef::circle(0.5),
            Transform2D::IDENTITY,
            Style::default(),
        );
        row.effects = Some(effects.clone());
        let mut scene = CompiledScene::compile_objects(vec![row], &[]).unwrap();
        let patch = ExecutionPatch::SetStyle {
            object: id,
            style: Style {
                fill: Some(Color::RED),
                ..Style::default()
            },
            fill_paint: None,
        };
        let inverse = scene.prepare_replay_revision(&patch).unwrap();
        scene.apply_execution_patch(&patch).unwrap();
        let rows = scene.objects().to_vec();
        for _ in 0..2 {
            assert!(
                scene.forward_prefix_revisions(&[&inverse]).is_none(),
                "effect stacks must not enter unqualified prefix suffixes"
            );
            assert_eq!(scene.objects(), rows.as_slice());
            assert!(std::sync::Arc::ptr_eq(
                scene.objects()[0].effects.as_ref().unwrap(),
                &effects
            ));
        }
    }
}
