use super::*;
use crate::spatial_authoring::{publish_mesh_creation, publish_mesh_family};
use crate::{AuthoringError, MeshOptions};

impl LiveSession<'_> {
    pub fn world_affine(
        &mut self,
        target: MobjectTarget<'_>,
        edit: crate::WorldAffineEdit,
    ) -> Result<(), LiveSessionError> {
        if let MobjectTarget::Object(object) = &target {
            crate::camera_motion_authoring::ensure_camera_motion_closed(object)?;
        }
        let transaction = crate::world_affine::prepare_world_affine_with(
            self.store,
            target,
            edit,
            |store, node| crate::spatial_authoring::effective_world(self.session, store, node),
        )?;
        self.apply(transaction).map(|_| ())
    }

    pub fn effective_world_transform(
        &self,
        object: &Mobject,
    ) -> Result<noon_core::SemanticWorldTransform3D, LiveSessionError> {
        self.require_mobject(object)?;
        crate::spatial_authoring::effective_world(
            self.session,
            &self.store.borrow(),
            object.node_id(),
        )
        .map_err(LiveSessionError::from)
    }

    /// Detached creation uses the same resource/publication boundary as Scene.
    pub fn create_mesh(&mut self, options: MeshOptions) -> Result<Mobject, LiveSessionError> {
        let node = self.with_semantic_publication(|store, publish| {
            publish_mesh_creation(store, options, publish)
        })?;
        Mobject::from_node(Rc::clone(self.store), node).map_err(LiveSessionError::from)
    }

    pub fn create_mesh_family(
        &mut self,
        options: Vec<MeshOptions>,
    ) -> Result<MobjectFamily, LiveSessionError> {
        let node = self.with_semantic_publication(|store, publish| {
            publish_mesh_family(store, options, publish)
        })?;
        MobjectFamily::from_node(Rc::clone(self.store), node).map_err(LiveSessionError::from)
    }

    pub fn set_world_transform(
        &mut self,
        object: &Mobject,
        world: noon_core::SemanticWorldTransform3D,
    ) -> Result<(), LiveSessionError> {
        self.require_mobject(object)?;
        crate::camera_motion_authoring::ensure_camera_motion_closed(object)?;
        let mut transaction = SemanticMutationTransaction::new();
        transaction.set_object_transform(object.node_id(), world.into());
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

impl crate::LiveSession<'_> {
    /// Ordinary spatial-scene add retains any existing label registration.
    pub fn add_all_world_mobjects(
        &mut self,
        targets: &[MobjectTarget<'_>],
    ) -> Result<noon_core::SemanticMutationTransactionResult, crate::SpatialCompositionError> {
        let transaction = crate::spatial_composition::add_transaction(
            self.integration_store(),
            self.root,
            targets,
            noon_core::SemanticSpatialCompositionDomain::World,
            true,
        )?;
        self.apply(transaction).map_err(Into::into)
    }

    /// Attach a batch with its spatial domain through the current publication.
    pub fn add_all_in_spatial_composition_domain(
        &mut self,
        targets: &[MobjectTarget<'_>],
        domain: noon_core::SemanticSpatialCompositionDomain,
    ) -> Result<noon_core::SemanticMutationTransactionResult, crate::SpatialCompositionError> {
        let transaction = crate::spatial_composition::add_transaction(
            self.integration_store(),
            self.root,
            targets,
            domain,
            false,
        )?;
        self.apply(transaction).map_err(Into::into)
    }
}
