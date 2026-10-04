//! Camera-composition domains authored through the canonical semantic transaction.

use crate::{
    AuthoringError, LiveSession, LiveSessionError, MobjectTarget, Scene, SceneMembershipRequest,
};
use noon_core::{
    SemanticMutationTransaction, SemanticMutationTransactionResult, SemanticObjectRole,
    SemanticSpatialCompositionDomain, SemanticStore,
};
use std::{cell::RefCell, collections::HashSet, rc::Rc};

/// Failure while assigning a camera-composition domain to ordinary scene objects.
#[derive(Debug)]
pub enum SpatialCompositionError {
    Authoring(AuthoringError),
    Live(LiveSessionError),
    FixedFrameRequiresPlanarOrientation,
    CameraOrLightMustRemainWorld,
    MeshMustRemainWorld,
}

impl std::fmt::Display for SpatialCompositionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Authoring(error) => error.fmt(formatter),
            Self::Live(error) => error.fmt(formatter),
            Self::FixedFrameRequiresPlanarOrientation => formatter.write_str(
                "FixedFrame requires a planar scalar orientation and cannot flatten a 3D quaternion",
            ),
            Self::MeshMustRemainWorld => formatter.write_str("indexed meshes must remain in the World composition domain"),
            Self::CameraOrLightMustRemainWorld => {
                formatter.write_str("cameras and point lights must remain in the World composition domain")
            }
        }
    }
}

impl std::error::Error for SpatialCompositionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Authoring(error) => Some(error),
            Self::Live(error) => Some(error),
            Self::FixedFrameRequiresPlanarOrientation
            | Self::CameraOrLightMustRemainWorld
            | Self::MeshMustRemainWorld => None,
        }
    }
}

impl From<AuthoringError> for SpatialCompositionError {
    fn from(error: AuthoringError) -> Self {
        Self::Authoring(error)
    }
}

impl From<LiveSessionError> for SpatialCompositionError {
    fn from(error: LiveSessionError) -> Self {
        Self::Live(error)
    }
}

fn append_domain_edits(
    store_rc: &Rc<RefCell<SemanticStore>>,
    target: MobjectTarget<'_>,
    domain: SemanticSpatialCompositionDomain,
    transaction: &mut SemanticMutationTransaction,
    preserve_registered: bool,
) -> Result<(), SpatialCompositionError> {
    let root = target.require_store(store_rc)?;

    let store = store_rc.borrow();
    let leaves = store
        .ordered_leaf_nodes(root)
        .map_err(AuthoringError::from)?;
    let mut seen = HashSet::with_capacity(leaves.len());
    for node in leaves {
        if !seen.insert(node) {
            continue;
        }
        let state = store
            .semantic_object_state_checked(node)
            .map_err(AuthoringError::from)?;
        if preserve_registered
            && state.spatial_composition_domain() != SemanticSpatialCompositionDomain::World
        {
            continue;
        }
        if !state.spatial_declaration_is_valid() {
            return Err(AuthoringError::NonFiniteObjectState.into());
        }
        if matches!(
            state.role(),
            SemanticObjectRole::Camera3D | SemanticObjectRole::PointLight3D
        ) && domain != SemanticSpatialCompositionDomain::World
        {
            return Err(SpatialCompositionError::CameraOrLightMustRemainWorld);
        }
        if domain != SemanticSpatialCompositionDomain::World
            && matches!(state.content.geometry(), Some(noon_core::StoredGeometry::Resource(handle))
                if matches!(store.geometry_resources().get(handle), Some(noon_core::GeometryResource::Mesh(_))))
        {
            return Err(SpatialCompositionError::MeshMustRemainWorld);
        }
        if domain == SemanticSpatialCompositionDomain::FixedFrame
            && state.transform.as_planar().is_none()
        {
            return Err(SpatialCompositionError::FixedFrameRequiresPlanarOrientation);
        }

        // Explicit World membership opts planar vector content into spatial
        // lowering without changing its authored pose. Preserve the exact
        // planar transform's evaluated world pose in the canonical quaternion.
        if domain == SemanticSpatialCompositionDomain::World
            && state.transform.planar_rotation().is_some()
        {
            let world = state
                .transform
                .world_transform()
                .ok_or(AuthoringError::NonFiniteObjectState)?;
            transaction.set_object_transform(node, world.into());
        }
        let anchor = (domain == SemanticSpatialCompositionDomain::FixedOrientation).then_some(root);
        transaction.set_spatial_composition_domain_with_anchor(node, domain, anchor);
    }
    Ok(())
}

fn composition_transaction(
    store: &Rc<RefCell<SemanticStore>>,
    target: MobjectTarget<'_>,
    domain: SemanticSpatialCompositionDomain,
) -> Result<SemanticMutationTransaction, SpatialCompositionError> {
    let mut transaction = SemanticMutationTransaction::new();
    append_domain_edits(store, target, domain, &mut transaction, false)?;
    Ok(transaction)
}

pub(crate) fn add_transaction(
    store: &Rc<RefCell<SemanticStore>>,
    root: noon_core::SemanticNodeId,
    targets: &[MobjectTarget<'_>],
    domain: SemanticSpatialCompositionDomain,
    preserve_registered: bool,
) -> Result<SemanticMutationTransaction, SpatialCompositionError> {
    let mut transaction = crate::scene_membership::prepare_scene_membership(
        store,
        root,
        SceneMembershipRequest::Add(targets),
    )?;
    for target in targets {
        append_domain_edits(
            store,
            *target,
            domain,
            &mut transaction,
            preserve_registered,
        )?;
    }
    Ok(transaction)
}

impl Scene {
    /// Ordinary spatial-scene add: lift World content while preserving registered labels.
    pub fn add_all_world_mobjects(
        &mut self,
        targets: &[MobjectTarget<'_>],
    ) -> Result<SemanticMutationTransactionResult, SpatialCompositionError> {
        let transaction = add_transaction(
            self.integration_store(),
            self.root(),
            targets,
            SemanticSpatialCompositionDomain::World,
            true,
        )?;
        self.apply_semantic_transaction(transaction)
            .map_err(Into::into)
    }

    /// Set the camera-composition domain on an object or all unique family leaves.
    /// One transaction owns both World-pose lifting and domain publication.
    pub fn set_spatial_composition_domain(
        &mut self,
        target: MobjectTarget<'_>,
        domain: SemanticSpatialCompositionDomain,
    ) -> Result<SemanticMutationTransactionResult, SpatialCompositionError> {
        let transaction = composition_transaction(self.integration_store(), target, domain)?;
        self.apply_semantic_transaction(transaction)
            .map_err(Into::into)
    }

    /// Attach an object or family and assign its composition domain atomically.
    pub fn add_in_spatial_composition_domain(
        &mut self,
        target: MobjectTarget<'_>,
        domain: SemanticSpatialCompositionDomain,
    ) -> Result<SemanticMutationTransactionResult, SpatialCompositionError> {
        self.add_all_in_spatial_composition_domain(&[target], domain)
    }

    /// Attach a batch and its camera-composition metadata in one transaction.
    pub fn add_all_in_spatial_composition_domain(
        &mut self,
        targets: &[MobjectTarget<'_>],
        domain: SemanticSpatialCompositionDomain,
    ) -> Result<SemanticMutationTransactionResult, SpatialCompositionError> {
        let transaction = add_transaction(
            self.integration_store(),
            self.root(),
            targets,
            domain,
            false,
        )?;
        self.apply_semantic_transaction(transaction)
            .map_err(Into::into)
    }
}

impl LiveSession<'_> {
    /// Set the camera-composition domain through this session's root publication.
    pub fn set_spatial_composition_domain(
        &mut self,
        target: MobjectTarget<'_>,
        domain: SemanticSpatialCompositionDomain,
    ) -> Result<SemanticMutationTransactionResult, SpatialCompositionError> {
        target.require_store(self.integration_store())?;
        let transaction = composition_transaction(self.integration_store(), target, domain)?;
        self.apply(transaction).map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use noon_core::{SemanticOrientation, SemanticSpatialCompositionDomain as Domain};

    #[test]
    fn world_domain_lifts_planar_pose_and_adds_in_one_transaction() {
        let mut scene = Scene::new();
        let mut object = scene.rectangle(2.0, 1.0).unwrap();
        object.move_to(2.0, -1.0).unwrap();
        object.rotate(4.0 * std::f64::consts::PI + 0.25).unwrap();
        let before_revision = scene.integration_store().borrow().scene_revision();

        scene
            .add_in_spatial_composition_domain(MobjectTarget::Object(&object), Domain::World)
            .unwrap();

        let state = object.state().unwrap();
        assert!(matches!(
            state.transform.orientation,
            SemanticOrientation::Spatial(_)
        ));
        assert_eq!(state.spatial_composition_domain(), Domain::World);
        assert_eq!(state.transform.translation.x, 2.0);
        assert_eq!(state.transform.translation.y, -1.0);
        assert_eq!(
            scene.integration_store().borrow().scene_revision(),
            before_revision.checked_next().unwrap(),
            "root membership and spatial declaration publish as one transaction"
        );
        assert!(scene
            .integration_store()
            .borrow()
            .node(scene.root())
            .unwrap()
            .members()
            .contains(&object.node_id()));
    }

    #[test]
    fn fixed_frame_keeps_the_exact_planar_angle() {
        let mut scene = Scene::new();
        let mut object = scene.square(1.0).unwrap();
        let angle = 10.0 * std::f64::consts::PI + 0.125;
        object.rotate(angle).unwrap();
        let original_transform = object.state().unwrap().transform;

        scene
            .set_spatial_composition_domain(MobjectTarget::Object(&object), Domain::FixedFrame)
            .unwrap();

        let state = object.state().unwrap();
        assert_eq!(state.transform, original_transform);
        assert_eq!(state.transform.planar_rotation(), Some(angle));
        assert_eq!(state.spatial_composition_domain(), Domain::FixedFrame);
    }

    #[test]
    fn ordinary_spatial_add_preserves_label_registration_and_lifts_new_content() {
        let mut scene = Scene::new();
        let fixed = scene.square(1.0).unwrap();
        let oriented = scene.circle(0.5).unwrap();
        let world = scene.rectangle(2.0, 1.0).unwrap();
        scene
            .set_spatial_composition_domain((&fixed).into(), Domain::FixedFrame)
            .unwrap();
        scene
            .set_spatial_composition_domain((&oriented).into(), Domain::FixedOrientation)
            .unwrap();
        let fixed_before = fixed.state().unwrap();
        let oriented_before = oriented.state().unwrap();
        let revision = scene.integration_store().borrow().scene_revision();
        scene
            .add_all_world_mobjects(&[(&fixed).into(), (&oriented).into(), (&world).into()])
            .unwrap();
        assert_eq!(fixed.state().unwrap(), fixed_before);
        assert_eq!(oriented.state().unwrap(), oriented_before);
        assert!(matches!(
            world.state().unwrap().transform.orientation,
            SemanticOrientation::Spatial(_)
        ));
        assert_eq!(
            scene.integration_store().borrow().scene_revision(),
            revision.checked_next().unwrap()
        );
    }

    #[test]
    fn nonplanar_fixed_frame_fails_atomically_and_family_orientation_shares_root() {
        let mut scene = Scene::new();
        let mesh = scene
            .mesh(crate::MeshOptions::new(crate::cube_mesh(1.0).unwrap()))
            .unwrap();
        let before = mesh.state().unwrap();
        assert!(matches!(
            scene.set_spatial_composition_domain(MobjectTarget::Object(&mesh), Domain::FixedFrame),
            Err(SpatialCompositionError::MeshMustRemainWorld)
        ));
        assert_eq!(mesh.state().unwrap(), before);

        let planar = scene.circle(0.5).unwrap();
        let family = scene.family(&[MobjectTarget::Object(&planar)]).unwrap();
        scene
            .set_spatial_composition_domain(
                MobjectTarget::Family(&family),
                Domain::FixedOrientation,
            )
            .unwrap();
        let state = planar.state().unwrap();
        assert_eq!(state.spatial_composition_domain(), Domain::FixedOrientation);
        assert_eq!(state.spatial_anchor_family(), Some(family.node_id()));
    }

    #[test]
    fn camera_and_light_cannot_leave_world_domain() {
        let mut scene = Scene::new();
        let camera = scene
            .camera_3d(
                noon_core::SemanticCamera3D::new(
                    noon_core::SemanticVec3::new(0.0, 0.0, 5.0),
                    noon_core::SemanticRotation3D::IDENTITY,
                    noon_core::SemanticProjection3D::Perspective {
                        vertical_fov_radians: 1.0,
                        near: 0.1,
                        far: 30.0,
                    },
                )
                .unwrap(),
            )
            .unwrap();
        assert!(matches!(
            scene
                .set_spatial_composition_domain(MobjectTarget::Object(&camera), Domain::FixedFrame),
            Err(SpatialCompositionError::CameraOrLightMustRemainWorld)
        ));
        assert_eq!(
            camera.state().unwrap().spatial_composition_domain(),
            Domain::World
        );
    }

    #[test]
    fn live_session_uses_the_same_transactional_domain_operation() {
        let mut scene = Scene::new();
        let object = scene.square(1.0).unwrap();
        scene.add(&object).unwrap();
        let mut execution = scene.execution_session().unwrap();
        {
            let mut live = scene.live(&mut execution);
            live.set_spatial_composition_domain(MobjectTarget::Object(&object), Domain::FixedFrame)
                .unwrap();
        }
        assert_eq!(
            object.state().unwrap().spatial_composition_domain(),
            Domain::FixedFrame
        );
        assert_eq!(execution.frame().objects.len(), 1);
    }
}
