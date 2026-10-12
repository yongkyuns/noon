use super::{RetainedExecutionFrameMirror, RetainedExecutionTransportError};
use crate::retained_resource_transport::RenderGeometrySlot;

/// Rollback touches only the arena slots changed by this candidate envelope.
pub(crate) struct RetainedRenderGeometryRollback {
    previous_session: Option<u32>,
    previous_len: usize,
    previous_slots: Vec<(usize, RenderGeometrySlot)>,
}

impl RetainedExecutionFrameMirror {
    pub(crate) fn stage_installed_render_geometries(
        &mut self,
        session: u32,
        updates: &[(u32, RenderGeometrySlot)],
    ) -> Result<RetainedRenderGeometryRollback, RetainedExecutionTransportError> {
        if self
            .resource_session
            .is_some_and(|installed| installed != session)
        {
            return Err(
                RetainedExecutionTransportError::InvalidRenderGeometryResource(
                    updates.first().map_or(0, |(slot, _)| u64::from(*slot)),
                ),
            );
        }
        let mut expected_len = self.render_geometries.len();
        for (slot, update) in updates {
            let index = *slot as usize;
            let valid = if index == expected_len {
                expected_len += 1;
                update.generation == 0 && update.geometry.is_some()
            } else if let Some(previous) = self.render_geometries.get(index) {
                if previous.geometry.is_some() {
                    previous.generation.checked_add(1) == Some(update.generation)
                } else {
                    previous.generation == update.generation && update.geometry.is_some()
                }
            } else {
                false
            };
            if !valid {
                return Err(
                    RetainedExecutionTransportError::InvalidRenderGeometryResource(
                        crate::retained_resource_transport::render_geometry_id(
                            *slot,
                            update.generation,
                        ),
                    ),
                );
            }
        }
        let mut rollback = RetainedRenderGeometryRollback {
            previous_session: self.resource_session,
            previous_len: self.render_geometries.len(),
            previous_slots: Vec::new(),
        };
        self.resource_session = Some(session);
        for (slot, update) in updates {
            let index = *slot as usize;
            if index == self.render_geometries.len() {
                self.render_geometries.push(update.clone());
            } else {
                rollback.previous_slots.push((
                    index,
                    std::mem::replace(&mut self.render_geometries[index], update.clone()),
                ));
            }
        }
        Ok(rollback)
    }

    pub(crate) fn rollback_installed_render_geometries(
        &mut self,
        rollback: RetainedRenderGeometryRollback,
    ) {
        self.resource_session = rollback.previous_session;
        self.render_geometries.truncate(rollback.previous_len);
        for (index, previous) in rollback.previous_slots {
            self.render_geometries[index] = previous;
        }
    }
}
