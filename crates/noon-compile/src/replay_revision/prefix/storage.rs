//! Explicit storage accounting and maintenance for historical execution.
use super::{CompiledReplayRevision, CompiledScene, CompiledTrack, SavedRow};
use crate::{CompiledChannelKey, CompiledObject};
use noon_core::{GeometryRef, TrackId, TrackValues};

/// The three Vec backing stores only. Inline headers are charged by the outer
/// revision index; indirect/shared allocations require separate accounting.
fn revision_payload_bytes(rows: usize, channels: usize, tracks: usize) -> Option<usize> {
    rows.checked_mul(std::mem::size_of::<SavedRow>())?
        .checked_add(channels.checked_mul(std::mem::size_of::<CompiledChannelKey>())?)?
        .checked_add(tracks.checked_mul(std::mem::size_of::<(TrackId, Option<CompiledTrack>)>())?)
}

// Count references conservatively rather than introducing a shared-payload registry.
// Values and prepared plans can share paths; the charge intentionally counts both.
fn geometry_bytes(geometry: &GeometryRef) -> Option<usize> {
    match geometry {
        GeometryRef::VectorPath(path) => {
            let bytes = path.retained_bytes();
            (bytes != usize::MAX).then_some(bytes)
        }
        _ => Some(0),
    }
}

fn row_geometry_bytes(row: &CompiledObject) -> Option<usize> {
    row.content.geometry().map_or(Some(0), geometry_bytes)
}

/// Logical inline bytes for one immutable supported effect-stack payload. Arc
/// aliases are charged at each row reference; allocator metadata, renderer
/// working storage, and unsupported effect programs/resources are excluded.
fn effect_stack_bytes(row: &CompiledObject) -> Option<usize> {
    let Some(stack) = row.effects.as_deref() else {
        return Some(0);
    };
    if !super::local_effect_stack(stack) {
        return Some(0);
    }
    std::mem::size_of::<crate::CompiledEffectStack>()
        .checked_add(
            stack
                .attachments()
                .len()
                .checked_mul(std::mem::size_of::<noon_core::SemanticNodeId>())?,
        )?
        .checked_add(
            stack
                .definitions()
                .len()
                .checked_mul(std::mem::size_of::<noon_core::EffectDefinition>())?,
        )
}

/// Logical bytes for one immutable composed membership plan. Arc aliases are
/// charged at each retained row reference, including the inline owner header.
fn composed_source_bytes(row: &CompiledObject) -> Option<usize> {
    let Some(source) = row.content.composed_source() else {
        return Some(0);
    };
    std::mem::size_of::<noon_core::ComposedSource>().checked_add(
        source
            .members()
            .len()
            .checked_mul(std::mem::size_of::<noon_core::ObjectId>())?,
    )
}

fn track_geometry_bytes(track: &CompiledTrack) -> Option<usize> {
    let values = match &track.values {
        TrackValues::PreparedMorph { geometry, .. } => geometry_bytes(geometry)?,
        TrackValues::Object { from, to } => {
            geometry_bytes(&from.geometry)?.checked_add(geometry_bytes(&to.geometry)?)?
        }
        _ => 0,
    };
    let plan = match &track.transform_geometry_plan {
        Some(crate::TransformGeometryPlan::PathPair {
            geometry,
            prestart_geometry,
            ..
        }) => geometry_bytes(geometry)?.checked_add(
            prestart_geometry
                .as_ref()
                .map_or(Some(0), |geometry| geometry_bytes(geometry))?,
        )?,
        _ => 0,
    };
    values.checked_add(plan)
}

impl CompiledScene {
    /// Finalize the already-committed patch's retained storage charge. Count both
    /// saved and current sides so later replay exchanges cannot increase it.
    /// Visits only affected rows/tracks; never scans the scene or copies paths.
    #[doc(hidden)]
    pub fn charge_prefix_revision_geometry(
        &self,
        revision: &mut CompiledReplayRevision,
    ) -> Option<()> {
        let mut bytes = 0usize;
        for saved in &revision.rows {
            for row in saved.object.iter().chain(
                self.objects
                    .get(saved.index as usize)
                    .filter(|row| row.live),
            ) {
                bytes = bytes.checked_add(row_geometry_bytes(row)?)?;
                revision.effect_payload_bytes = revision
                    .effect_payload_bytes
                    .checked_add(effect_stack_bytes(row)?)?;
                revision.composed_payload_bytes = revision
                    .composed_payload_bytes
                    .checked_add(composed_source_bytes(row)?)?;
            }
        }
        for (id, saved) in &revision.tracks {
            for track in saved.iter().chain(self.track(*id)) {
                bytes = bytes.checked_add(track_geometry_bytes(track)?)?;
            }
        }
        revision.geometry_payload_bytes = bytes;
        Some(())
    }

    /// Logical closure bytes for supported immutable stacks in the current
    /// projection. One charge per row reference; shared `Arc`s are intentionally
    /// not deduplicated because this is a conservative retained-history charge.
    pub fn prefix_effect_payload_bytes(&self) -> Option<usize> {
        self.objects.iter().try_fold(0usize, |total, row| {
            total.checked_add(effect_stack_bytes(row)?)
        })
    }

    /// Logical composed-source membership bytes in the current projection,
    /// charged once per row reference with checked arithmetic.
    pub fn prefix_composed_payload_bytes(&self) -> Option<usize> {
        self.objects.iter().try_fold(0usize, |total, row| {
            total.checked_add(composed_source_bytes(row)?)
        })
    }
}

impl CompiledReplayRevision {
    /// Requested row/channel/track backing plus both-side path/effect/composed payloads.
    /// These operations preserve occupied lengths, not source spare capacity.
    /// This is a pre-allocation request, not a bound on allocator over-allocation.
    pub fn prefix_clone_payload_bytes(&self) -> Option<usize> {
        revision_payload_bytes(self.rows.len(), self.channels.len(), self.tracks.len())?
            .checked_add(self.geometry_payload_bytes)?
            .checked_add(self.effect_payload_bytes)?
            .checked_add(self.composed_payload_bytes)
    }

    /// Reported Vec backing plus stable both-side logical payload charges.
    /// Shared paths are conservatively charged per saved/current reference.
    /// Revision exchange only swaps elements; it cannot resize these buffers.
    /// Excludes the outer index, object/track storage in the active projection,
    /// other indirect Box/Arc payloads, path spare capacity, allocator overhead and temporaries.
    pub fn prefix_payload_bytes(&self) -> Option<usize> {
        revision_payload_bytes(
            self.rows.capacity(),
            self.channels.capacity(),
            self.tracks.capacity(),
        )?
        .checked_add(self.geometry_payload_bytes)?
        .checked_add(self.effect_payload_bytes)?
        .checked_add(self.composed_payload_bytes)
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
            geometry_payload_bytes: 0,
            effect_payload_bytes: 0,
            composed_payload_bytes: 0,
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
    fn composed_membership_is_charged_on_both_saved_and_current_rows() {
        use noon_core::{
            ComposedSource, GeometryRef, ObjectContentRef, ObjectId, Style, Transform2D,
        };
        use std::sync::Arc;

        let members = [ObjectId::new(1), ObjectId::new(2)];
        let mut owner = CompiledObject::new(
            ObjectId::new(0),
            GeometryRef::circle(0.5),
            Transform2D::IDENTITY,
            Style::default(),
        );
        owner.content =
            ObjectContentRef::Composed(Arc::new(ComposedSource::new(members, None).unwrap()));
        owner.draw_enabled = false;
        owner.base_style = Style {
            fill: None,
            stroke: None,
            ..Style::default()
        };
        let member = |id| {
            CompiledObject::new(
                ObjectId::new(id),
                GeometryRef::circle(0.25),
                Transform2D::IDENTITY,
                Style::default(),
            )
        };
        let scene =
            CompiledScene::compile_objects(vec![owner.clone(), member(1), member(2)], &[]).unwrap();
        let one_reference = std::mem::size_of::<ComposedSource>()
            + members
                .len()
                .checked_mul(std::mem::size_of::<ObjectId>())
                .unwrap();
        let mut revision = CompiledReplayRevision {
            rows: vec![SavedRow {
                index: 0,
                object: Some(owner),
                order: None,
            }],
            inset_2d_view: None,
            channels: Vec::new(),
            tracks: Vec::new(),
            camera_motion_payloads: 0,
            path_motion_commands: 0,
            geometry_payload_bytes: 0,
            effect_payload_bytes: 0,
            composed_payload_bytes: 0,
        };

        scene
            .charge_prefix_revision_geometry(&mut revision)
            .unwrap();
        assert_eq!(revision.composed_payload_bytes, one_reference * 2);
        assert_eq!(scene.prefix_composed_payload_bytes(), Some(one_reference));

        revision.composed_payload_bytes = usize::MAX;
        assert!(scene
            .charge_prefix_revision_geometry(&mut revision)
            .is_none());
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
    fn unqualified_effect_rows_remain_outside_prefix_suffix_admission() {
        use crate::{CompiledEffect, CompiledEffectStack, CompiledObject, ExecutionPatch};
        use noon_core::{
            Blur, Color, EffectDefinition, GeometryRef, Glow, ObjectId, SemanticNodeId, Style,
            Transform2D,
        };
        for definitions in [
            vec![EffectDefinition::Glow(Glow::default()); 2],
            vec![EffectDefinition::Blur(Blur::default()); 2],
        ] {
            let id = ObjectId::new(0);
            let effects = std::sync::Arc::new(
                CompiledEffectStack::new(definitions.into_iter().enumerate().map(
                    |(i, definition)| CompiledEffect {
                        attachment: SemanticNodeId::new(i as u32 + 7, 1),
                        definition,
                    },
                ))
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

    #[test]
    fn ordered_blur_glow_visibility_stack_is_admitted_as_one_bounded_profile() {
        use crate::{CompiledEffect, CompiledEffectStack, CompiledObject, ExecutionPatch};
        use noon_core::{
            Blur, Color, EffectDefinition, GeometryRef, Glow, NoiseDissolve, ObjectId,
            SemanticNodeId, Style, Transform2D,
        };
        use std::sync::Arc;

        let object_id = ObjectId::new(0);
        let visibility = SemanticNodeId::new(9, 1);
        let blur = SemanticNodeId::new(7, 1);
        let glow = SemanticNodeId::new(8, 1);
        let effects = Arc::new(
            CompiledEffectStack::new_with_visibility(
                [
                    CompiledEffect {
                        attachment: blur,
                        definition: EffectDefinition::Blur(Blur::default()),
                    },
                    CompiledEffect {
                        attachment: glow,
                        definition: EffectDefinition::Glow(Glow::default()),
                    },
                    CompiledEffect {
                        attachment: visibility,
                        definition: EffectDefinition::NoiseDissolve(NoiseDissolve::default()),
                    },
                ],
                Some(visibility),
            )
            .unwrap(),
        );
        let mut row = CompiledObject::new(
            object_id,
            GeometryRef::circle(0.5),
            Transform2D::IDENTITY,
            Style::default(),
        );
        row.effects = Some(Arc::clone(&effects));
        let mut scene = CompiledScene::compile_objects(vec![row], &[]).unwrap();
        let patch = ExecutionPatch::SetStyle {
            object: object_id,
            style: Style {
                fill: Some(Color::RED),
                ..Style::default()
            },
            fill_paint: None,
        };
        let inverse = scene.prepare_replay_revision(&patch).unwrap();
        scene.apply_execution_patch(&patch).unwrap();

        assert!(scene.supports_prefix_revision(&inverse));
        assert!(scene.forward_prefix_revisions(&[&inverse]).is_some());

        let blur_and_visibility = Arc::new(
            CompiledEffectStack::new_with_visibility(
                [
                    CompiledEffect {
                        attachment: blur,
                        definition: EffectDefinition::Blur(Blur::default()),
                    },
                    CompiledEffect {
                        attachment: visibility,
                        definition: EffectDefinition::NoiseDissolve(NoiseDissolve::default()),
                    },
                ],
                Some(visibility),
            )
            .unwrap(),
        );
        let visibility_only = Arc::new(
            CompiledEffectStack::new_with_visibility(
                [CompiledEffect {
                    attachment: visibility,
                    definition: EffectDefinition::NoiseDissolve(NoiseDissolve::default()),
                }],
                Some(visibility),
            )
            .unwrap(),
        );
        for (expected, next) in [
            (
                vec![blur, glow, visibility],
                Some(Arc::clone(&blur_and_visibility)),
            ),
            (vec![blur, visibility], Some(Arc::clone(&visibility_only))),
            (vec![visibility], None),
            (vec![], Some(Arc::clone(&effects))),
        ] {
            let patch = ExecutionPatch::SetEffectStack {
                object: object_id,
                expected: expected.into_boxed_slice(),
                effects: next,
            };
            let inverse = scene.prepare_replay_revision(&patch).unwrap();
            scene.apply_execution_patch(&patch).unwrap();
            assert!(scene.supports_prefix_revision(&inverse));
            assert!(scene.forward_prefix_revisions(&[&inverse]).is_some());
        }

        let wrong_order = Arc::new(
            CompiledEffectStack::new([
                CompiledEffect {
                    attachment: SemanticNodeId::new(8, 1),
                    definition: EffectDefinition::Glow(Glow::default()),
                },
                CompiledEffect {
                    attachment: SemanticNodeId::new(7, 1),
                    definition: EffectDefinition::Blur(Blur::default()),
                },
            ])
            .unwrap(),
        );
        let mut unsupported = CompiledObject::new(
            object_id,
            GeometryRef::circle(0.5),
            Transform2D::IDENTITY,
            Style::default(),
        );
        unsupported.effects = Some(wrong_order);
        let mut rejected = CompiledScene::compile_objects(vec![unsupported], &[]).unwrap();
        let inverse = rejected.prepare_replay_revision(&patch).unwrap();
        rejected.apply_execution_patch(&patch).unwrap();
        assert!(!rejected.supports_prefix_revision(&inverse));
        assert!(rejected.forward_prefix_revisions(&[&inverse]).is_none());
    }
}
