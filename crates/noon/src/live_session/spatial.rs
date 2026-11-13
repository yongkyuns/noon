use super::*;
use crate::AuthoringError;

impl LiveSession<'_> {
    /// Atomically recolor one sampled surface through this session's publication owner.
    pub fn set_surface_checkerboard(
        &mut self,
        surface: &crate::SurfaceFamily,
        colors: [crate::Color; 2],
        opacity: f64,
    ) -> Result<(), LiveSessionError> {
        self.require_family(surface.family())?;
        let transaction = surface.prepare_checkerboard_transaction(colors, opacity)?;
        self.apply(transaction).map(|_| ())
    }

    /// Regenerate only this mesh's resource after a dependency changes. Old
    /// immutable versions remain available to snapshots and submitted draw work.
    pub fn replace_mesh_geometry(
        &mut self,
        object: &Mobject,
        mesh: noon_core::MeshResource,
    ) -> Result<(), LiveSessionError> {
        self.require_mobject(object)?;
        let old = object
            .state()?
            .content
            .geometry()
            .and_then(|geometry| geometry.resource_handle())
            .ok_or(AuthoringError::NonFiniteGeometry)?;
        {
            let store = self.store.borrow();
            if !matches!(
                store.geometry_resources().get(old),
                Some(noon_core::GeometryResource::Mesh(_))
            ) {
                return Err(AuthoringError::NonFiniteGeometry.into());
            }
        }
        self.with_semantic_publication(|store, publish| {
            store.with_geometry_mesh(mesh, |store, handle| {
                let mut transaction = SemanticMutationTransaction::new();
                transaction.replace_content(
                    object.node_id(),
                    noon_core::StoredGeometry::Resource(handle),
                );
                publish(store, transaction).map(|_| ())
            })
        })
    }
}

#[cfg(test)]
mod tests;
