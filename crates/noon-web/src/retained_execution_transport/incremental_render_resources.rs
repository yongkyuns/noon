use std::sync::Arc;

use noon_core::GeometryRef;

use super::{RetainedExecutionFrameMirror, RetainedExecutionTransportError};

/// O(1) rollback token for a temporarily staged renderer-resource table.
/// The previous immutable table is retained by Arc while the candidate delta validates.
pub(crate) struct RetainedRenderGeometryRollback {
    previous_session: Option<u32>,
    previous_geometries: Arc<[Arc<GeometryRef>]>,
}

impl RetainedExecutionFrameMirror {
    pub(crate) fn stage_installed_render_geometries(
        &mut self,
        session: u32,
        geometries: Arc<[Arc<GeometryRef>]>,
    ) -> Result<RetainedRenderGeometryRollback, RetainedExecutionTransportError> {
        if self.resource_session.is_some_and(|installed| installed != session) {
            return Err(RetainedExecutionTransportError::InvalidRenderGeometryResource(
                geometries.len().saturating_sub(1) as u32,
            ));
        }
        if geometries.len() < self.render_geometries.len()
            || !self
                .render_geometries
                .iter()
                .zip(geometries.iter())
                .all(|(installed, next)| Arc::ptr_eq(installed, next))
        {
            return Err(RetainedExecutionTransportError::InvalidRenderGeometryResource(
                geometries.len().saturating_sub(1) as u32,
            ));
        }
        let rollback = RetainedRenderGeometryRollback {
            previous_session: self.resource_session,
            previous_geometries: self.render_geometries.clone(),
        };
        self.resource_session = Some(session);
        self.render_geometries = geometries;
        Ok(rollback)
    }

    pub(crate) fn rollback_installed_render_geometries(
        &mut self,
        rollback: RetainedRenderGeometryRollback,
    ) {
        self.resource_session = rollback.previous_session;
        self.render_geometries = rollback.previous_geometries;
    }
}
