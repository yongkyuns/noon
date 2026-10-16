//! Persistent placement using an exact settled callback publication.
use super::*;

impl ExecutionSession {
    /// A difference from authored state is not, by itself, an active animation.
    /// Accept only affine domains owned by a callback at this exact settled
    /// publication. Timeline, reactive, failed and unpublished work stays closed.
    pub(crate) fn placement_uses_settled_callback_transform(
        &self,
        store: &noon_core::SemanticStore,
        target: SemanticNodeId,
        authored: Transform2D,
        effective: Transform2D,
    ) -> bool {
        if self.pending_callback.is_some()
            || self.pending_segment_completion.is_some()
            || self.callback_termination.is_some()
        {
            return false;
        }
        let Some(domains) = self.last_callback_receipt.as_ref().and_then(|receipt| {
            receipt.domains_at(target, self.frame().time, self.publication_context())
        }) else {
            return false;
        };
        let Ok(state) = store.semantic_object_state_checked(target) else {
            return false;
        };
        if !matches!(
            state.role(),
            SemanticObjectRole::Ordinary | SemanticObjectRole::Camera2D
        ) || !matches!(state.transform.orientation, SemanticOrientation::Planar(_))
            || state.signal_bindings().iter().any(|binding| {
                matches!(
                    binding.property(),
                    SemanticObjectProperty::Translation
                        | SemanticObjectProperty::RotationZ
                        | SemanticObjectProperty::Scale
                )
            })
        {
            return false;
        }
        let mut changed = 0;
        if authored.translation != effective.translation {
            changed |= CALLBACK_TRANSLATION;
        }
        if authored.rotation != effective.rotation {
            changed |= CALLBACK_ROTATION;
        }
        if authored.scale != effective.scale {
            changed |= CALLBACK_SCALE;
        }
        changed != 0 && changed & !domains == 0
    }

    /// Publish a placement of one settled callback-owned affine row through the
    /// existing semantic/runtime transaction. An explicit effective carry keeps
    /// callback rotation/scale and handles a reset to the unchanged authored
    /// translation: an authored no-op must not leave the old callback position.
    pub(crate) fn publish_settled_callback_translation(
        &mut self,
        store: &mut noon_core::SemanticStore,
        root: SemanticNodeId,
        target: SemanticNodeId,
        translation: SemanticVec3,
        mut transform: Transform2D,
    ) -> Result<
        noon_core::SemanticMutationTransactionResult,
        super::super::ExecutionSessionPublicationError,
    > {
        use super::super::publication::SemanticPublicationPurpose;
        use super::super::ExecutionSessionPublicationError as PublicationError;

        self.require_resource_creation_at_root(store, root)?;
        let object = self
            .execution_index
            .execution_object_id(target)
            .ok_or(PublicationError::UnknownObject(target))?;
        // All fallible lowering and effective writes are prepared before the
        // single semantic commit. No callback invocation or clock change occurs.
        let mut transaction = noon_core::SemanticMutationTransaction::new();
        transaction.set_property(target, SemanticObjectProperty::Translation, translation);
        let prepared = transaction
            .prepare(store)
            .map_err(PublicationError::Semantic)?;
        transform.translation = noon_core::Vec2::new(translation.x as f32, translation.y as f32);
        let effective = self
            .runtime
            .prepare_effective_property_batch(&[RuntimeEffectivePropertyWrite::Transform {
                object,
                transform,
            }])
            .map_err(|error| PublicationError::Runtime(error.into()))?;
        let before = self.publication_context();
        let (result, _) = self.apply_prepared_semantic_transaction_with_execution_contract(
            prepared,
            Vec::new(),
            Some(effective).into(),
            SemanticPublicationPurpose::AuthoredMutation,
            None,
            Some(root),
        )?;
        // A value-only placement does not change any callback registrations.
        // Rebase the exact existing receipt without scanning other callback rows.
        let publication = self.publication_context();
        let time = self.frame().time;
        if let Some(receipt) = self
            .last_callback_receipt
            .as_mut()
            .filter(|receipt| receipt.time == time && receipt.publication == before)
        {
            receipt.publication = publication;
        } else {
            self.last_callback_receipt = None;
        }
        self.callback_schedule
            .carry_completed_publication(time, publication);
        Ok(result)
    }

    /// An ordinary layout translation changes neither callback registration nor
    /// authored time. Preserve only a phase completed at the exact old publication;
    /// a pending, invalidated, or newly registered callback must still run.
    pub(crate) fn carry_settled_callback_through_layout_translation(
        &mut self,
        time: f64,
        before: PublicationContext,
    ) {
        if self.frame().time != time
            || self.pending_callback.is_some()
            || self.callback_termination.is_some()
            || self.callback_schedule.completed_time != Some(time)
            || self.callback_schedule.completed_publication != Some(before)
        {
            return;
        }
        let publication = self.publication_context();
        self.callback_schedule
            .carry_completed_publication(time, publication);
        if let Some(receipt) = self
            .last_callback_receipt
            .as_mut()
            .filter(|receipt| receipt.time == time && receipt.publication == before)
        {
            receipt.publication = publication;
        }
    }
}
