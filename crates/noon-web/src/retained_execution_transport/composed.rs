//! Resource keys and sparse checked source dependencies at the worker boundary.
use super::*;
use crate::TransportComposedSourceHandle;
use noon_core::ComposedSource;

mod graph;
pub(super) use graph::ResolvedRow;

#[derive(Clone, Debug)]
pub(super) struct PublishedComposedSource {
    handle: TransportComposedSourceHandle,
    source: Arc<ComposedSource>,
}

pub(super) fn published_source(
    frame: &FrameState,
    index: usize,
    content: &TransportObjectContent,
) -> Option<PublishedComposedSource> {
    let TransportObjectContent::Composed { source: handle } = content else {
        return None;
    };
    let ObjectContentRef::Composed(source) = &frame.objects[index].content else {
        return None;
    };
    Some(PublishedComposedSource {
        handle: *handle,
        source: source.clone(),
    })
}

impl RetainedExecutionDeltaEncoder {
    pub(super) fn composed_source_handle(
        &self,
        frame: &FrameState,
        index: usize,
    ) -> Option<TransportComposedSourceHandle> {
        let row = frame.objects.get(index)?;
        let ObjectContentRef::Composed(source) = &row.content else {
            return None;
        };
        if let Some(previous) = self
            .published_composed_sources
            .get(index)
            .and_then(Option::as_ref)
        {
            if previous.handle.owner == row.id
                && (Arc::ptr_eq(&previous.source, source) || previous.source == *source)
            {
                return Some(previous.handle);
            }
        }
        Some(TransportComposedSourceHandle {
            session: self.session,
            owner: row.id,
            version: self.next_sequence,
        })
    }
    pub(super) fn commit_composed_sources(
        &mut self,
        frame: &FrameState,
        rows: &[RetainedTransportObjectState],
        removed: &HashSet<usize>,
    ) {
        self.published_composed_sources
            .resize(frame.objects.len(), None);
        for row in rows {
            let index = row.slot.slot as usize;
            self.published_composed_sources[index] = published_source(frame, index, &row.content);
        }
        for &index in removed {
            if let Some(source) = self.published_composed_sources.get_mut(index) {
                *source = None;
            }
        }
    }
    pub(crate) fn published_composed_source(
        &self,
        slot: TransportSlotId,
        handle: TransportComposedSourceHandle,
    ) -> Option<&ComposedSource> {
        let published = self
            .published_composed_sources
            .get(slot.slot as usize)?
            .as_ref()?;
        (slot.generation == 0 && published.handle == handle).then_some(published.source.as_ref())
    }
}

pub(super) fn validate_owner_fields(
    row: &RetainedTransportObjectState,
) -> Result<(), RetainedExecutionTransportError> {
    if let TransportObjectContent::Composed { source } = row.content {
        if source.owner != row.object
            || row.draw_enabled
            || row.transform != Transform2D::IDENTITY
            || row.spatial.is_some()
            || row.fill_paint.is_some()
            || row.style.fill.is_some()
            || row.style.stroke.is_some()
            || !row.style.opacity.is_finite()
            || !(0.0..=1.0).contains(&row.style.opacity)
            || row.appearance != 1.0
            || !row.presence
            || row.reveal != 1.0
            || row.morph != 0.0
            || row.text_bounds.is_some()
            || row.render_geometry.is_some()
            || row.render_geometry_resource.is_some()
            || row.render_transform.is_some()
        {
            return Err(RetainedExecutionTransportError::InvalidComposedSourceHandle(row.slot));
        }
    }
    Ok(())
}

impl RetainedExecutionFrameMirror {
    #[cfg(test)]
    pub(crate) fn composed_validation_stats(&self) -> noon_compile::ComposedValidationStats {
        self.last_composed_validation
    }
    pub(crate) fn extend_installed_composed_sources(
        &mut self,
        sources: &HashMap<TransportComposedSourceHandle, Arc<ComposedSource>>,
    ) {
        self.composed_sources.extend(
            sources
                .iter()
                .map(|(&handle, source)| (handle, source.clone())),
        );
    }
    pub(crate) fn remove_installed_composed_sources<'a>(
        &mut self,
        handles: impl IntoIterator<Item = &'a TransportComposedSourceHandle>,
    ) {
        for handle in handles {
            self.composed_sources.remove(handle);
        }
    }
    pub(super) fn validate_composed_handles(
        &self,
        delta: &RetainedExecutionDeltaEnvelope,
    ) -> Result<(), RetainedExecutionTransportError> {
        for row in &delta.objects {
            for effect in &row.effects {
                if let TransportEffectDefinition::Shader { program, .. } = effect.definition {
                    if program.session != delta.session || program.version > delta.sequence {
                        return Err(RetainedExecutionTransportError::InvalidEffectState(
                            row.slot,
                        ));
                    }
                }
            }
            if let TransportObjectContent::Composed { source } = row.content {
                if source.session != delta.session
                    || source.owner != row.object
                    || source.version > delta.sequence
                    || (self.session == Some(delta.session)
                        && self
                            .composed_handles
                            .get(&row.object)
                            .is_some_and(|current| source.version < current.version))
                {
                    return Err(
                        RetainedExecutionTransportError::InvalidComposedSourceHandle(row.slot),
                    );
                }
            }
        }
        Ok(())
    }
    pub(super) fn commit_composed_handles(&mut self, delta: &RetainedExecutionDeltaEnvelope) {
        if delta.snapshot {
            self.composed_handles.clear();
        }
        for slot in &delta.removed_slots {
            if let Some(index) = self.slot_indices.get(slot) {
                self.composed_handles
                    .remove(&self.frame.as_ref().expect("installed frame").objects[*index].id);
            }
        }
        for row in &delta.objects {
            if let TransportObjectContent::Composed { source } = row.content {
                self.composed_handles.insert(row.object, source);
            } else {
                self.composed_handles.remove(&row.object);
            }
        }
    }
}
