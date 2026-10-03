//! Spatial objects use ordinary semantic identities, publication, and handles.

use crate::{AuthoringError, DeclaredAnimation, Mobject, MobjectFamily, Scene};
use noon_core::{
    Color, GeometryResource, MeshResource, SemanticCamera3D, SemanticMutationTransaction,
    SemanticNodeCreation, SemanticObjectRole, SemanticObjectState, SemanticPaint,
    SemanticSpatialMaterial, SemanticStyle, SemanticVec3, SemanticWorldTransform3D, StoredGeometry,
};
use std::{rc::Rc, sync::Arc};

/// Inert mesh constructor input. Generation/sampling happens once before admission;
/// instance motion subsequently changes the ordinary effective world transform.
#[derive(Clone, Debug)]
pub struct MeshOptions {
    pub geometry: MeshResource,
    pub transform: SemanticWorldTransform3D,
    pub style: SemanticStyle,
    pub material: SemanticSpatialMaterial,
}

impl MeshOptions {
    /// Opaque unlit fill with no stroke is supported by the retained mesh lane.
    pub fn new(geometry: MeshResource) -> Self {
        Self {
            geometry,
            transform: SemanticWorldTransform3D::IDENTITY,
            style: SemanticStyle {
                fill: Some(SemanticPaint::Solid(Color::BLUE)),
                stroke: None,
                stroke_width: 0.0,
                ..SemanticStyle::default()
            },
            material: SemanticSpatialMaterial::Unlit,
        }
    }

    pub fn with_transform(mut self, transform: SemanticWorldTransform3D) -> Self {
        self.transform = transform;
        self
    }

    pub fn with_style(mut self, style: SemanticStyle) -> Self {
        self.style = style;
        self
    }

    pub fn with_material(mut self, material: SemanticSpatialMaterial) -> Self {
        self.material = material;
        self
    }

    fn into_resource(
        self,
    ) -> (
        GeometryResource,
        impl FnOnce(noon_core::GeometryResourceHandle) -> SemanticObjectState,
    ) {
        let Self {
            geometry,
            transform,
            style,
            material,
        } = self;
        let state = move |handle| {
            let mut state = SemanticObjectState::new(StoredGeometry::Resource(handle));
            state.transform = transform.into();
            state.style = style;
            state.set_spatial_material(material);
            state
        };
        (GeometryResource::Mesh(Arc::new(geometry)), state)
    }
}

pub(crate) fn publish_mesh_creation(
    store: &mut noon_core::SemanticStore,
    options: MeshOptions,
    publish: impl FnOnce(
        &mut noon_core::SemanticStore,
        SemanticMutationTransaction,
    ) -> Result<noon_core::SemanticMutationTransactionResult, AuthoringError>,
) -> Result<noon_core::SemanticNodeId, AuthoringError> {
    let (resource, make_state) = options.into_resource();
    store.with_geometry_resources([resource], |store, handles| {
        let mut transaction = SemanticMutationTransaction::new();
        let object = transaction.create_node(SemanticNodeCreation::object(make_state(handles[0])));
        publish(store, transaction)?
            .resolve(object)
            .ok_or(AuthoringError::UnresolvedCreatedNode(object))
    })
}

pub(crate) fn publish_mesh_family(
    store: &mut noon_core::SemanticStore,
    options: Vec<MeshOptions>,
    publish: impl FnOnce(
        &mut noon_core::SemanticStore,
        SemanticMutationTransaction,
    ) -> Result<noon_core::SemanticMutationTransactionResult, AuthoringError>,
) -> Result<noon_core::SemanticNodeId, AuthoringError> {
    let (resources, constructors): (Vec<_>, Vec<_>) =
        options.into_iter().map(MeshOptions::into_resource).unzip();
    store.with_geometry_resources(resources, |store, handles| {
        let mut transaction = SemanticMutationTransaction::new();
        let family = transaction.create_node(SemanticNodeCreation::family());
        for (make_state, handle) in constructors.into_iter().zip(handles) {
            let face = transaction.create_node(SemanticNodeCreation::object(make_state(*handle)));
            transaction.add_member(family, face);
        }
        publish(store, transaction)?
            .resolve(family)
            .ok_or(AuthoringError::UnresolvedCreatedNode(family))
    })
}

impl Scene {
    /// Apply world motion from this owner's coherent current state while running.
    pub fn world_affine(
        &mut self,
        target: crate::MobjectTarget<'_>,
        edit: crate::WorldAffineEdit,
    ) -> Result<(), AuthoringError> {
        let transaction = if let Some(session) = self.running_execution() {
            crate::world_affine::prepare_world_affine_with(
                self.integration_store(),
                target,
                edit,
                |store, node| effective_world(session, store, node),
            )?
        } else {
            crate::world_affine::prepare_world_affine(self.integration_store(), target, edit)?
        };
        self.apply_semantic_transaction(transaction).map(|_| ())
    }

    /// Read current published pose without substituting authored base state.
    pub fn effective_world_transform(
        &self,
        object: &Mobject,
    ) -> Result<SemanticWorldTransform3D, AuthoringError> {
        self.require_object(object)?;
        let session = self.running_execution().ok_or(AuthoringError::Unsupported(
            crate::UnsupportedAuthoringOperation::EffectiveStateUnavailable,
        ))?;
        effective_world(
            session,
            &self.integration_store().borrow(),
            object.node_id(),
        )
    }

    /// Admit one detached mesh through the Scene-owned atomic resource publication.
    pub fn mesh(&mut self, options: MeshOptions) -> Result<Mobject, AuthoringError> {
        let node = self.with_semantic_publication(|store, publish| {
            publish_mesh_creation(store, options, publish)
        })?;
        Mobject::from_node(Rc::clone(self.integration_store()), node)
    }

    /// Admit a detached family of independently addressable mesh faces/caps.
    /// Family and resources share one rollback boundary and semantic identity space.
    pub fn mesh_family(
        &mut self,
        options: Vec<MeshOptions>,
    ) -> Result<MobjectFamily, AuthoringError> {
        let node = self.with_semantic_publication(|store, publish| {
            publish_mesh_family(store, options, publish)
        })?;
        MobjectFamily::from_node(Rc::clone(self.integration_store()), node)
    }

    /// Initialize this Scene's one 3D camera before root content is attached.
    /// It remains an ordinary semantic Mobject whose world tracks drive Runtime.
    pub fn camera_3d(&mut self, camera: SemanticCamera3D) -> Result<Mobject, AuthoringError> {
        if !self
            .integration_store()
            .borrow()
            .node(self.root())
            .ok_or(AuthoringError::NonFiniteObjectState)?
            .members()
            .is_empty()
        {
            return Err(AuthoringError::CameraRequiresEmptyScene(self.root()));
        }
        let mut state = SemanticObjectState::new(StoredGeometry::Circle { radius: 0.0 });
        state.set_role(SemanticObjectRole::Camera3D);
        state
            .set_camera_projection(Some(camera.projection))
            .map_err(|_| AuthoringError::NonFiniteObjectState)?;
        state.transform = SemanticWorldTransform3D::new(
            camera.position,
            camera.orientation,
            SemanticVec3::new(1.0, 1.0, 1.0),
        )
        .ok_or(AuthoringError::NonFiniteObjectState)?
        .into();
        self.create_spatial_role(state, true)
    }

    /// Create a detached point light. Color and intensity use ordinary effective
    /// fill/opacity properties; motion uses the same world transform as meshes.
    pub fn point_light_3d(
        &mut self,
        position: SemanticVec3,
        color: Color,
        intensity: f64,
    ) -> Result<Mobject, AuthoringError> {
        if !(0.0..=1.0).contains(&intensity) {
            return Err(AuthoringError::InvalidOpacity {
                name: "light intensity".into(),
                value: intensity,
            });
        }
        let mut state = SemanticObjectState::new(StoredGeometry::Circle { radius: 0.0 });
        state.set_role(SemanticObjectRole::PointLight3D);
        state.transform.translation = position;
        state.style = SemanticStyle {
            fill: Some(SemanticPaint::Solid(color)),
            fill_opacity: intensity,
            stroke: None,
            stroke_width: 0.0,
            ..SemanticStyle::default()
        };
        self.create_spatial_role(state, false)
    }

    fn create_spatial_role(
        &mut self,
        state: SemanticObjectState,
        attach: bool,
    ) -> Result<Mobject, AuthoringError> {
        let mut transaction = SemanticMutationTransaction::new();
        let node = transaction.create_node(SemanticNodeCreation::object(state));
        if attach {
            transaction.add_member(self.root(), node);
        }
        let node = self
            .apply_semantic_transaction(transaction)?
            .resolve(node)
            .ok_or(AuthoringError::UnresolvedCreatedNode(node))?;
        Mobject::from_node(Rc::clone(self.integration_store()), node)
    }

    /// Publish a complete authored world transform before or after bootstrap.
    /// Timeline/reactive effective state remains Runtime's responsibility.
    pub fn set_world_transform(
        &mut self,
        object: &Mobject,
        world: SemanticWorldTransform3D,
    ) -> Result<(), AuthoringError> {
        self.require_object(object)?;
        let mut transaction = SemanticMutationTransaction::new();
        transaction.set_object_transform(object.node_id(), world.into());
        self.apply_semantic_transaction(transaction).map(|_| ())
    }

    /// Declare shared world-transform-to intent before execution bootstrap.
    /// Activation captures the then-current effective pose through the ordinary
    /// LiveSession animation path.
    pub fn declare_world_transform(
        &self,
        object: &Mobject,
        target: SemanticWorldTransform3D,
        options: noon_core::AnimationOptions,
    ) -> Result<DeclaredAnimation, String> {
        self.require_object(object)
            .map_err(|error| error.to_string())?;
        if self.running_execution().is_some() {
            return Err(
                "world track declaration requires unstarted execution; use a live composition"
                    .into(),
            );
        }
        if options.lag_ratio.is_some()
            || options.path_arc.is_some()
            || options.reverse_rate_function.is_some()
            || options.remover.is_some()
            || options.introducer.is_some()
        {
            return Err("world tracks support duration and rate function options".into());
        }
        let mut transaction = SemanticMutationTransaction::new();
        let animation =
            transaction.create_world_transform_animation(object.node_id(), target, options);
        let node = transaction
            .apply(&mut self.integration_store().borrow_mut())
            .map_err(|error| error.to_string())?
            .resolve(animation)
            .ok_or_else(|| "world track declaration was not resolved".to_owned())?;
        Ok(DeclaredAnimation::new(
            Rc::clone(self.integration_store()),
            node,
        ))
    }
}

impl MobjectFamily {
    /// Authored world affine edit; Scene/LiveSession publish edits into execution.
    pub fn world_affine(&mut self, edit: crate::WorldAffineEdit) -> Result<(), AuthoringError> {
        let transaction = crate::world_affine::prepare_world_affine(
            self.integration_store(),
            (&*self).into(),
            edit,
        )?;
        transaction
            .apply(&mut self.integration_store().borrow_mut())
            .map(|_| ())
            .map_err(AuthoringError::from)
    }

    /// Cold-authoring mesh family without creating an extra Scene root.
    pub fn from_meshes(
        store: std::rc::Rc<std::cell::RefCell<noon_core::SemanticStore>>,
        options: Vec<MeshOptions>,
    ) -> Result<Self, AuthoringError> {
        let node = publish_mesh_family(&mut store.borrow_mut(), options, |store, transaction| {
            transaction.apply(store).map_err(AuthoringError::from)
        })?;
        Self::from_node(store, node)
    }
}

impl Mobject {
    /// Authored world affine edit; Scene/LiveSession publish edits into execution.
    pub fn world_affine(&mut self, edit: crate::WorldAffineEdit) -> Result<(), AuthoringError> {
        let transaction = crate::world_affine::prepare_world_affine(
            self.integration_store(),
            (&*self).into(),
            edit,
        )?;
        transaction
            .apply(&mut self.integration_store().borrow_mut())
            .map(|_| ())
            .map_err(AuthoringError::from)
    }

    /// Cold-authoring constructor; running scenes use their publication owner.
    pub fn from_mesh(
        store: std::rc::Rc<std::cell::RefCell<noon_core::SemanticStore>>,
        options: MeshOptions,
    ) -> Result<Self, AuthoringError> {
        let node =
            publish_mesh_creation(&mut store.borrow_mut(), options, |store, transaction| {
                transaction.apply(store).map_err(AuthoringError::from)
            })?;
        Self::from_node(store, node)
    }

    /// Authored full-precision pose. Reading effective Runtime state requires Scene.
    pub fn world_transform(&self) -> Result<SemanticWorldTransform3D, AuthoringError> {
        self.state()?
            .transform
            .world_transform()
            .ok_or(AuthoringError::NonFiniteObjectState)
    }

    /// Edit authored state/targets; use Scene::set_world_transform for live publication.
    pub fn set_world_transform(
        &mut self,
        world: SemanticWorldTransform3D,
    ) -> Result<(), AuthoringError> {
        let mut transaction = SemanticMutationTransaction::new();
        transaction.set_object_transform(self.node_id(), world.into());
        transaction
            .apply(&mut self.integration_store().borrow_mut())
            .map(|_| ())
            .map_err(AuthoringError::from)
    }
}

pub(crate) fn effective_world(
    session: &crate::ExecutionSession,
    store: &noon_core::SemanticStore,
    node: noon_core::SemanticNodeId,
) -> Result<SemanticWorldTransform3D, AuthoringError> {
    session
        .effective_semantic_object(store, node)?
        .object
        .world_transform()
        .ok_or(AuthoringError::NonFiniteObjectState)
}

#[cfg(test)]
mod tests;
