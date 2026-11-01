//! Explicit capacity maintenance; append itself never relocates scene storage.
use super::object_budget;
use super::{PrefixError, PrefixReceiver, ReplayError, Revision};
use crate::{FrameObjectState, SceneInstance};

/// Constant-time admission for the index allocation only, including spare slots.
/// Kept separate from payload/resource accounting: Revision contains Vec headers,
/// not the allocations owned by those vectors. No history traversal or allocation.
pub(super) fn preflight_history_index_capacity(capacity: usize) -> Result<usize, PrefixError> {
    let bytes_per_revision = std::mem::size_of::<Revision>();
    let limit_bytes = SceneInstance::MAX_PREFIX_HISTORY_INDEX_BYTES;
    capacity
        .checked_mul(bytes_per_revision)
        .filter(|&bytes| bytes <= limit_bytes)
        .ok_or(PrefixError::HistoryIndexBudgetExceeded {
            revision_capacity: capacity,
            bytes_per_revision,
            limit_bytes,
        })
}

/// One storage-growth implementation for bootstrap and later maintenance.
/// Requested over-budget storage fails before any reservation. Allocation failure
/// or allocator over-reservation may leave larger capacities, but cannot change
/// rows, history, publication or the receiver's advertised object capacity.
pub(super) fn reserve_object_vectors(
    runtime: &mut SceneInstance,
    total: usize,
    limit_bytes: usize,
) -> Result<(), PrefixError> {
    object_budget::preflight_reserved(runtime, total, limit_bytes)?;
    runtime
        .compiled
        .reserve_prefix_object_capacity(total)
        .map_err(|_| PrefixError::Allocation)?;
    macro_rules! reserve {
        ($values:expr) => {
            $values
                .try_reserve_exact(total.saturating_sub($values.len()))
                .map_err(|_| PrefixError::Allocation)?;
        };
    }
    reserve!(runtime.frame.objects);
    reserve!(runtime.frame.presences);
    reserve!(runtime.frame.reveals);
    reserve!(runtime.frame.morphs);
    reserve!(runtime.frame.render_geometries);
    reserve!(runtime.frame.render_transforms);
    reserve!(runtime.frame.family_animations);
    reserve!(runtime.frame.family_animation_plan_indices);
    reserve!(runtime.painter_ranks);
    reserve!(runtime.painter_order);
    object_budget::preflight_reserved(runtime, 0, limit_bytes)?;
    Ok(())
}

impl PrefixReceiver {
    /// Named maintenance barrier for all storage required by this admitted batch.
    /// Failed validation never allocates; failed allocation changes capacity only.
    pub fn reserve_append_capacity(
        &mut self,
        runtime: &mut SceneInstance,
        batch: &super::PrefixBatch,
    ) -> Result<(), PrefixError> {
        match self.preflight_append(runtime, batch) {
            Ok(_)
            | Err(PrefixError::CapacityRequired { .. })
            | Err(PrefixError::FamilyAnimationCapacityRequired { .. }) => {}
            Err(error) => return Err(error),
        }
        self.reserve_object_capacity(runtime, batch.required_object_capacity())?;
        let total = batch.required_family_animation_capacity();
        runtime
            .compiled
            .reserve_prefix_family_capacity(total)
            .map_err(|_| PrefixError::Allocation)?;
        runtime
            .timeline_scheduler
            .reserve_prefix_groups(batch.family_animations.len())
            .map_err(|_| PrefixError::Allocation)?;
        self.family_capacity = self.family_capacity.max(total);
        Ok(())
    }

    /// Named O(existing storage) maintenance barrier, separate from ordinary
    /// prefix delivery. Reserve ahead using a bounded workload capacity or the
    /// next batch's required_object_capacity(). Returns requested total slots.
    /// Allocation failure changes capacity only, never frame/history/publication.
    /// The separate object-vector byte cap includes existing spare capacity.
    /// No-op requests also check it; previously published history stays usable.
    /// It excludes maps, indirect payloads and allocator/whole-process peaks.
    pub fn reserve_object_capacity(
        &mut self,
        runtime: &mut SceneInstance,
        total: usize,
    ) -> Result<usize, PrefixError> {
        self.validate_runtime(runtime)?;
        if total <= self.object_capacity {
            // Failed growth may leave excess spare capacity without committing
            // object_capacity. A no-op must not report that storage as admitted.
            object_budget::preflight_reserved(runtime, total, self.object_vector_budget_bytes)?;
            return Ok(self.object_capacity);
        }
        if total > u32::MAX as usize
            || total.saturating_sub(self.initial_object_slots) > self.limits.payloads
        {
            return Err(ReplayError::RetentionLimit.into());
        }
        reserve_object_vectors(runtime, total, self.object_vector_budget_bytes)?;
        self.object_capacity = total;
        Ok(total)
    }
}
impl SceneInstance {
    /// Materialize only previously reserved tombstone rows, upon a requested
    /// seek or forward advance, never during append. Actual membership/content is
    /// selected by historical revision exchange before any renderer publication.
    pub(super) fn extend_prefix_frame_storage(&mut self) {
        while self.frame.objects.len() < self.compiled.objects().len() {
            let object = &self.compiled.objects()[self.frame.objects.len()];
            debug_assert!(!object.live);
            self.frame.objects.push(FrameObjectState {
                fill_paint: object.fill_paint.clone(),
                draw_enabled: object.draw_enabled,
                effects: None,
                id: object.id,
                content: object.content.clone(),
                text_bounds: object.text_bounds,
                transform: object.base_transform,
                spatial: None,
                style: object.base_style,
                z_index: object.base_z_index,
                appearance: 1.0,
            });
            self.frame.presences.push(false);
            self.frame.reveals.push(1.0);
            self.frame.morphs.push(0.0);
            self.frame.render_geometries.push(None);
            self.frame.render_transforms.push(None);
            self.frame.family_animations.push(None);
            self.frame.family_animation_plan_indices.push(None);
            self.painter_ranks.push(None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PrefixBatch, ReplayLimits};
    use noon_compile::{CompiledObject, CompiledScene, ExecutionPatch};
    use noon_core::{
        Color, GeometryRef, GradientStop, LinearGradient, ObjectId, SemanticPaint, Style,
        Transform2D,
    };
    use std::sync::Arc;

    fn gradient(start: Color, end: Color) -> Arc<SemanticPaint> {
        Arc::new(SemanticPaint::LinearGradient(Arc::new(
            LinearGradient::new(
                [0.0, 0.5],
                [1.0, 0.5],
                [
                    GradientStop::new(0.0, start).unwrap(),
                    GradientStop::new(1.0, end).unwrap(),
                ],
            )
            .unwrap(),
        )))
    }

    fn object(id: u64, paint: Option<Arc<SemanticPaint>>) -> CompiledObject {
        let mut object = CompiledObject::new(
            ObjectId::new(id),
            GeometryRef::rectangle(2.0, 1.0),
            Transform2D::IDENTITY,
            Style::default(),
        );
        object.fill_paint = paint;
        object
    }

    fn source() -> SceneInstance {
        let compiled = CompiledScene::compile_objects(vec![object(0, None)], &[]).unwrap();
        let mut source = SceneInstance::new(compiled);
        source
            .begin_replay_retention(ReplayLimits::default())
            .unwrap();
        source.advance_to(1.0).unwrap();
        source
    }

    fn append(receiver: &mut PrefixReceiver, runtime: &mut SceneInstance, batch: PrefixBatch) {
        receiver
            .reserve_object_capacity(runtime, batch.required_object_capacity())
            .unwrap();
        receiver.append(runtime, batch).unwrap();
    }

    #[test]
    fn prefix_storage_materializes_gradient_and_solid_rows_without_publishing_the_future() {
        let mut source = source();
        let (mut publisher, mut runtime, mut receiver) = source.create_prefix_projection().unwrap();
        receiver.prepare_seek(&mut runtime, 0.5).unwrap();
        runtime.seek(0.5).unwrap();
        let before = runtime.frame().clone();
        let publication = runtime.publication_context();
        let paint = gradient(Color::RED, Color::BLUE);
        for object in [object(1, Some(paint.clone())), object(2, None)] {
            source
                .apply_execution_patch(&ExecutionPatch::CreateObject(object))
                .unwrap();
        }
        source.advance_to(2.0).unwrap();
        let batch = source.prepare_prefix_update(&mut publisher).unwrap();
        append(&mut receiver, &mut runtime, batch);
        assert_eq!(
            runtime.frame(),
            &before,
            "append must not materialize future rows"
        );
        assert_eq!(runtime.publication_context(), publication);

        // Admission grows only dormant frame storage. Historical evaluation,
        // not delivery or reservation, makes a future object visible.
        receiver.prepare_seek(&mut runtime, 0.5).unwrap();
        assert_eq!(runtime.frame().objects.len(), 3);
        assert_eq!(runtime.frame().objects[0], before.objects[0]);
        assert!(!runtime.frame().presences[1]);
        assert!(!runtime.frame().presences[2]);
        assert!(Arc::ptr_eq(
            runtime.frame().objects[1].fill_paint.as_ref().unwrap(),
            &paint,
        ));
        assert!(runtime.frame().objects[2].fill_paint.is_none());
        assert_eq!(runtime.frame().time, before.time);
        assert_eq!(runtime.publication_context(), publication);
        receiver.prepare_seek(&mut runtime, 0.5).unwrap();
        assert_eq!(
            runtime.frame().objects.len(),
            3,
            "repeated admission must not add rows"
        );
    }

    #[test]
    fn prefix_storage_restores_gradient_revisions_after_source_drop_and_out_of_order_seek() {
        let mut source = source();
        let (mut publisher, mut runtime, mut receiver) = source.create_prefix_projection().unwrap();
        receiver.prepare_seek(&mut runtime, 0.5).unwrap();
        runtime.seek(0.5).unwrap();
        let original = runtime.frame().clone();
        let first_paint = gradient(Color::RED, Color::BLUE);
        let second_paint = gradient(Color::BLUE, Color::RED);
        source
            .apply_execution_patch(&ExecutionPatch::CreateObject(object(1, Some(first_paint))))
            .unwrap();
        source.advance_to(1.5).unwrap();
        let first = source.frame().objects[1].clone();
        source.advance_to(2.0).unwrap();
        let batch = source.prepare_prefix_update(&mut publisher).unwrap();
        append(&mut receiver, &mut runtime, batch);
        assert_eq!(runtime.frame(), &original);
        source
            .apply_execution_patch(&ExecutionPatch::SetStyle {
                object: ObjectId::new(1),
                style: Style::default(),
                fill_paint: Some(second_paint),
            })
            .unwrap();
        source.advance_to(2.5).unwrap();
        let second = source.frame().objects[1].clone();
        assert_ne!(first.fill_paint, second.fill_paint);
        source.advance_to(3.0).unwrap();
        source
            .apply_execution_patch(&ExecutionPatch::SetStyle {
                object: ObjectId::new(1),
                style: Style::default(),
                fill_paint: None,
            })
            .unwrap();
        source.advance_to(3.5).unwrap();
        let solid = source.frame().objects[1].clone();
        assert!(solid.fill_paint.is_none());
        source.advance_to(4.0).unwrap();
        source.seal_replay().unwrap();
        let terminal = source.prepare_prefix_finish(&mut publisher).unwrap();
        append(&mut receiver, &mut runtime, terminal);
        assert_eq!(
            runtime.frame(),
            &original,
            "terminal delivery must not move the reader"
        );
        assert_eq!(receiver.end_time(), Some(4.0));
        drop(source);
        drop(publisher);

        for (time, expected) in [
            (3.5, &solid),
            (1.5, &first),
            (2.5, &second),
            (1.5, &first),
            (4.0, &solid),
        ] {
            receiver.prepare_seek(&mut runtime, time).unwrap();
            runtime.seek(time).unwrap();
            assert!(runtime.frame().presences[1]);
            assert_eq!(&runtime.frame().objects[1], expected, "paint at {time}");
            receiver.prepare_seek(&mut runtime, 0.5).unwrap();
            runtime.seek(0.5).unwrap();
            assert!(!runtime.frame().presences[1]);
            assert_eq!(runtime.frame().objects[0], original.objects[0]);
        }
    }
}
