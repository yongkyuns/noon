//! Canonical scene spatial creation and live/cold mutation ownership.

use super::{CanonicalAuthoringScene, PlayerOwnership};
use noon::{Color, SemanticCamera3D, WorldAffineEdit};
use noon_core::SemanticVec3;
use std::rc::Rc;

impl CanonicalAuthoringScene {
    pub(crate) fn set_surface_checkerboard(
        &mut self,
        surface: &noon::SurfaceFamily,
        colors: [Color; 2],
        opacity: f64,
    ) -> Result<(), crate::authoring_error::AuthoringFailure> {
        match &mut self.player_ownership {
            PlayerOwnership::Unstarted => self
                .scene
                .set_surface_checkerboard(surface, colors, opacity)
                .map_err(Into::into),
            PlayerOwnership::Active(_) | PlayerOwnership::Returned(_) => self
                .active_live_player()?
                .live_set_surface_checkerboard(surface, colors, opacity),
            PlayerOwnership::Transferred(_) => {
                Err("live execution session is running in the semantic engine".into())
            }
        }
    }

    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn declare_world_transform(
        &self,
        object: &noon::Mobject,
        transform: noon::SemanticWorldTransform3D,
        options: noon_core::AnimationOptions,
    ) -> Result<noon::DeclaredAnimation, String> {
        if !self.player_ownership.is_unstarted() {
            return Err(
                "declare live animations before beginning execution; use a live composition".into(),
            );
        }
        self.scene
            .declare_world_transform(object, transform, options)
    }

    pub(crate) fn create_mesh(
        &mut self,
        options: noon::MeshOptions,
    ) -> Result<noon::Mobject, crate::authoring_error::AuthoringFailure> {
        match &mut self.player_ownership {
            PlayerOwnership::Unstarted => self.scene.mesh(options).map_err(Into::into),
            PlayerOwnership::Active(_) | PlayerOwnership::Returned(_) => {
                self.active_live_player()?.live_create_mesh(options)
            }
            PlayerOwnership::Transferred(_) => {
                Err("live execution session is running in the semantic engine".into())
            }
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn create_mesh_family(
        &mut self,
        options: Vec<noon::MeshOptions>,
        paths: Vec<noon::SpatialPathOptions>,
    ) -> Result<noon::MobjectFamily, crate::authoring_error::AuthoringFailure> {
        match &mut self.player_ownership {
            PlayerOwnership::Unstarted => self
                .scene
                .mesh_family_with_paths(options, paths)
                .map_err(Into::into),
            PlayerOwnership::Active(_) | PlayerOwnership::Returned(_) => self
                .active_live_player()?
                .live_create_mesh_family_with_paths(options, paths),
            PlayerOwnership::Transferred(_) => {
                Err("live execution session is running in the semantic engine".into())
            }
        }
    }

    pub(crate) fn create_camera_3d(
        &mut self,
        id: noon_core::ObjectId,
        camera: SemanticCamera3D,
    ) -> Result<noon::Mobject, crate::authoring_error::AuthoringFailure> {
        if !self.player_ownership.is_unstarted() {
            return Err("3D camera must be declared before execution starts".into());
        }
        if self.bindings.contains_key(&id) {
            return Err(format!("canonical object {} is already bound", id.get()).into());
        }
        let object = self
            .scene
            .camera_3d(camera)
            .map_err(crate::authoring_error::AuthoringFailure::from)?;
        let node = object.node_id();
        self.bindings.insert(id, node);
        self.identities.insert(node, id);
        Ok(object)
    }

    pub(crate) fn create_point_light_3d(
        &mut self,
        position: SemanticVec3,
        color: Color,
        intensity: f64,
    ) -> Result<noon::Mobject, crate::authoring_error::AuthoringFailure> {
        if !self.player_ownership.is_unstarted() {
            return Err("3D point lights must be created before execution starts".into());
        }
        self.scene
            .point_light_3d(position, color, intensity)
            .map_err(Into::into)
    }

    pub(crate) fn effective_world_transform(
        &mut self,
        object: &noon::Mobject,
    ) -> Result<noon::SemanticWorldTransform3D, crate::authoring_error::AuthoringFailure> {
        if !Rc::ptr_eq(self.scene.integration_store(), object.integration_store()) {
            return Err(noon::AuthoringError::ForeignStore.into());
        }
        object
            .validate()
            .map_err(crate::authoring_error::AuthoringFailure::from)?;
        match &mut self.player_ownership {
            PlayerOwnership::Unstarted => object.world_transform().map_err(Into::into),
            PlayerOwnership::Active(_) | PlayerOwnership::Returned(_) => self
                .active_live_player()?
                .live_effective_world_transform(object),
            PlayerOwnership::Transferred(_) => {
                Err("effective world state is owned by the transferred execution session".into())
            }
        }
    }

    pub(crate) fn effective_world_center(
        &mut self,
        object: &noon::Mobject,
    ) -> Result<noon_core::SemanticVec3, crate::authoring_error::AuthoringFailure> {
        if !Rc::ptr_eq(self.scene.integration_store(), object.integration_store()) {
            return Err(noon::AuthoringError::ForeignStore.into());
        }
        object
            .validate()
            .map_err(crate::authoring_error::AuthoringFailure::from)?;
        match &mut self.player_ownership {
            PlayerOwnership::Unstarted => object.world_center().map_err(Into::into),
            PlayerOwnership::Active(_) | PlayerOwnership::Returned(_) => self
                .active_live_player()?
                .live_effective_world_center(object),
            PlayerOwnership::Transferred(_) => {
                Err("effective world state is owned by the transferred execution session".into())
            }
        }
    }

    pub(crate) fn effective_world_family_center(
        &mut self,
        family: &noon::MobjectFamily,
    ) -> Result<noon_core::SemanticVec3, crate::authoring_error::AuthoringFailure> {
        if !Rc::ptr_eq(self.scene.integration_store(), family.integration_store()) {
            return Err(noon::AuthoringError::ForeignStore.into());
        }
        match &mut self.player_ownership {
            PlayerOwnership::Unstarted => family.world_center().map_err(Into::into),
            PlayerOwnership::Active(_) | PlayerOwnership::Returned(_) => self
                .active_live_player()?
                .live_effective_world_family_center(family),
            PlayerOwnership::Transferred(_) => {
                Err("effective world state is owned by the transferred execution session".into())
            }
        }
    }

    pub(crate) fn set_world_transform(
        &mut self,
        object: &noon::Mobject,
        transform: noon::SemanticWorldTransform3D,
    ) -> Result<(), crate::authoring_error::AuthoringFailure> {
        if !Rc::ptr_eq(self.scene.integration_store(), object.integration_store()) {
            return Err(noon::AuthoringError::ForeignStore.into());
        }
        object
            .validate()
            .map_err(crate::authoring_error::AuthoringFailure::from)?;
        match &mut self.player_ownership {
            PlayerOwnership::Unstarted => self
                .scene
                .set_world_transform(object, transform)
                .map_err(Into::into),
            PlayerOwnership::Active(_) | PlayerOwnership::Returned(_) => self
                .active_live_player()?
                .live_set_world_transform(object, transform),
            PlayerOwnership::Transferred(_) => {
                Err("live execution session is running in the semantic engine".into())
            }
        }
    }

    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn world_affine(
        &mut self,
        target: noon::MobjectTarget<'_>,
        edit: WorldAffineEdit,
    ) -> Result<(), crate::authoring_error::AuthoringFailure> {
        let target_store = match &target {
            noon::MobjectTarget::Object(object) => object.integration_store(),
            noon::MobjectTarget::Family(family) => family.integration_store(),
        };
        if !Rc::ptr_eq(self.scene.integration_store(), target_store) {
            return Err(noon::AuthoringError::ForeignStore.into());
        }
        match &target {
            noon::MobjectTarget::Object(object) => object.validate(),
            noon::MobjectTarget::Family(family) => family.validate(),
        }
        .map_err(crate::authoring_error::AuthoringFailure::from)?;
        match &mut self.player_ownership {
            PlayerOwnership::Unstarted => self.scene.world_affine(target, edit).map_err(Into::into),
            PlayerOwnership::Active(_) | PlayerOwnership::Returned(_) => match target {
                noon::MobjectTarget::Object(object) => self
                    .active_live_player()?
                    .live_world_affine_object(object, edit),
                noon::MobjectTarget::Family(family) => self
                    .active_live_player()?
                    .live_world_affine_family(family, edit),
            },
            PlayerOwnership::Transferred(_) => {
                Err("live execution session is running in the semantic engine".into())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use noon::{MeshOptions, SemanticRotation3D, SemanticWorldTransform3D, WorldAffineEdit};
    use noon_core::{ObjectId, SemanticProjection3D, SemanticSpatialMaterial};

    fn mesh_at(x: f64) -> MeshOptions {
        let transform = SemanticWorldTransform3D::new(
            SemanticVec3::new(x, 0.0, 0.0),
            SemanticRotation3D::IDENTITY,
            SemanticVec3::new(1.0, 1.0, 1.0),
        )
        .unwrap();
        MeshOptions::new(noon::cube_mesh(1.0).unwrap()).with_transform(transform)
    }

    fn camera() -> SemanticCamera3D {
        SemanticCamera3D::new(
            SemanticVec3::new(0.0, 0.0, 5.0),
            SemanticRotation3D::IDENTITY,
            SemanticProjection3D::Perspective {
                vertical_fov_radians: 1.0,
                near: 0.1,
                far: 100.0,
            },
        )
        .unwrap()
    }

    #[test]
    fn cold_mesh_creation_uses_one_store_and_preserves_world_precision() {
        let mut context = CanonicalAuthoringScene::default();
        let object = context.create_mesh(mesh_at(1234.5678901234567)).unwrap();
        assert_eq!(
            object.world_transform().unwrap().translation.x,
            1234.5678901234567
        );
        let state = object.state().unwrap();
        assert_eq!(state.spatial_material(), SemanticSpatialMaterial::Unlit);
        let resource = state.content.geometry().unwrap().resource_handle().unwrap();
        assert!(matches!(
            context
                .scene
                .integration_store()
                .borrow()
                .geometry_resources()
                .get(resource),
            Some(noon_core::GeometryResource::Mesh(_))
        ));
    }

    #[test]
    fn world_declarations_cannot_bypass_an_active_owner() {
        let mut context = CanonicalAuthoringScene::default();
        context
            .create_camera_3d(ObjectId::new(1), camera())
            .unwrap();
        let mesh = context.create_mesh(mesh_at(0.0)).unwrap();
        context.bind_mobject(ObjectId::new(2), &mesh).unwrap();
        context
            .declare_world_transform(
                &mesh,
                SemanticWorldTransform3D::IDENTITY,
                noon_core::AnimationOptions::new(),
            )
            .unwrap();
        context.live_player(1.0).unwrap();
        let revision = context.scene.revision();
        let nodes = context.scene.integration_store().borrow().len();
        assert!(context
            .declare_world_transform(
                &mesh,
                SemanticWorldTransform3D::IDENTITY,
                noon_core::AnimationOptions::new()
            )
            .is_err());
        assert_eq!(context.scene.revision(), revision);
        assert_eq!(context.scene.integration_store().borrow().len(), nodes);
    }

    #[test]
    fn active_mesh_creation_and_membership_use_the_existing_player_owner() {
        let mut context = CanonicalAuthoringScene::default();
        let marker = context.scene.circle(0.5).unwrap();
        context.bind_mobject(ObjectId::new(1), &marker).unwrap();
        context.live_player(2.0).unwrap();
        let mesh = context.create_mesh(mesh_at(0.125)).unwrap();
        assert_eq!(context.player_ownership.browser_name(), "active");
        assert_eq!(context.active_live_player().unwrap().time(), 0.0);
        context.bind_mobject(ObjectId::new(2), &mesh).unwrap();
        assert_eq!(context.active_live_player().unwrap().time(), 0.0);
        assert!(context.identities.contains_key(&mesh.node_id()));
        assert!(!context
            .scene
            .integration_store()
            .borrow()
            .geometry_resources()
            .is_empty());
    }

    #[test]
    fn world_center_queries_follow_scene_owner_and_authored_detached_fallback() {
        let mut context = CanonicalAuthoringScene::default();
        let object = context.create_mesh(mesh_at(2.0)).unwrap();
        let companion = context.create_mesh(mesh_at(6.0)).unwrap();
        let family = noon::MobjectFamily::create(
            Rc::clone(context.scene.integration_store()),
            &[(&object).into(), (&companion).into()],
        )
        .unwrap();
        context.scene.add_many(&[(&family).into()]).unwrap();
        let detached_member = context.create_mesh(mesh_at(7.0)).unwrap();
        let detached_family = noon::MobjectFamily::create(
            Rc::clone(context.scene.integration_store()),
            &[(&detached_member).into()],
        )
        .unwrap();
        assert_eq!(
            context.effective_world_center(&object).unwrap(),
            SemanticVec3::new(2.0, 0.0, 0.0)
        );
        assert_eq!(
            context.effective_world_family_center(&family).unwrap(),
            SemanticVec3::new(4.0, 0.0, 0.0)
        );
        context.live_player(1.0).unwrap();
        let mut pose = object.world_transform().unwrap();
        pose.translation = SemanticVec3::new(4.0, 5.0, 6.0);
        context.set_world_transform(&object, pose).unwrap();
        assert_eq!(
            context.effective_world_center(&object).unwrap(),
            SemanticVec3::new(4.0, 5.0, 6.0)
        );
        assert_eq!(
            context.effective_world_family_center(&family).unwrap(),
            SemanticVec3::new(5.0, 2.5, 3.0)
        );

        let detached = context.create_mesh(mesh_at(7.0)).unwrap();
        assert_eq!(
            context.effective_world_center(&detached).unwrap(),
            SemanticVec3::new(7.0, 0.0, 0.0)
        );
        assert_eq!(
            context
                .effective_world_family_center(&detached_family)
                .unwrap(),
            SemanticVec3::new(7.0, 0.0, 0.0)
        );

        let foreign = CanonicalAuthoringScene::default()
            .scene
            .circle(0.5)
            .unwrap();
        assert!(context.effective_world_center(&foreign).is_err());
    }

    #[test]
    fn camera_binding_and_point_light_are_cold_only_and_fail_without_mutation() {
        let mut context = CanonicalAuthoringScene::default();
        let camera_object = context
            .create_camera_3d(ObjectId::new(7), camera())
            .unwrap();
        assert_eq!(
            context.bindings.get(&ObjectId::new(7)),
            Some(&camera_object.node_id())
        );
        assert!(context
            .scene
            .integration_store()
            .borrow()
            .node(context.scene.root())
            .unwrap()
            .members()
            .contains(&camera_object.node_id()));

        let mut late = CanonicalAuthoringScene::default();
        late.live_player(1.0).unwrap();
        let nodes_before = late.scene.integration_store().borrow().len();
        assert!(late.create_camera_3d(ObjectId::new(8), camera()).is_err());
        assert!(late
            .create_point_light_3d(SemanticVec3::ZERO, Color::WHITE, 0.5)
            .is_err());
        assert_eq!(late.scene.integration_store().borrow().len(), nodes_before);

        let mut cold = CanonicalAuthoringScene::default();
        let light = cold
            .create_point_light_3d(SemanticVec3::new(1.0, 2.0, 3.0), Color::WHITE, 0.5)
            .unwrap();
        assert!(light.validate().is_ok());
        assert!(!cold
            .scene
            .integration_store()
            .borrow()
            .node(cold.scene.root())
            .unwrap()
            .members()
            .contains(&light.node_id()));
    }

    #[test]
    fn world_transform_read_and_write_keep_f64_pose_values() {
        let mut context = CanonicalAuthoringScene::default();
        let object = context.create_mesh(mesh_at(0.0)).unwrap();
        context.bind_mobject(ObjectId::new(11), &object).unwrap();
        context.live_player(1.0).unwrap();
        let expected = SemanticWorldTransform3D::new(
            SemanticVec3::new(0.12345678901234567, -9.876543210987654, 2.000000000000001),
            SemanticRotation3D::from_components(0.9, 0.1, 0.2, 0.3).unwrap(),
            SemanticVec3::new(1.25, 2.5, 0.75),
        )
        .unwrap();
        context.set_world_transform(&object, expected).unwrap();
        assert_eq!(
            context.effective_world_transform(&object).unwrap(),
            expected
        );
        assert_eq!(object.world_transform().unwrap(), expected);
    }

    #[test]
    fn family_world_affine_and_parallel_world_transform_composition_share_scene_state() {
        let mut context = CanonicalAuthoringScene::default();
        let first = context.create_mesh(mesh_at(-1.0)).unwrap();
        let second = context.create_mesh(mesh_at(1.0)).unwrap();
        context.bind_mobject(ObjectId::new(21), &first).unwrap();
        context.bind_mobject(ObjectId::new(22), &second).unwrap();
        let family = context
            .scene
            .family(&[(&first).into(), (&second).into()])
            .unwrap();
        context
            .world_affine(
                (&family).into(),
                WorldAffineEdit::Shift(SemanticVec3::new(0.5, -0.25, 1.0)),
            )
            .unwrap();
        assert_eq!(
            first.world_transform().unwrap().translation,
            SemanticVec3::new(-0.5, -0.25, 1.0)
        );
        assert_eq!(
            second.world_transform().unwrap().translation,
            SemanticVec3::new(1.5, -0.25, 1.0)
        );

        let first_end = SemanticWorldTransform3D::new(
            SemanticVec3::new(-2.0, 0.0, 2.0),
            SemanticRotation3D::IDENTITY,
            SemanticVec3::new(1.0, 1.0, 1.0),
        )
        .unwrap();
        let second_end = SemanticWorldTransform3D::new(
            SemanticVec3::new(2.0, 0.0, 2.0),
            SemanticRotation3D::IDENTITY,
            SemanticVec3::new(1.0, 1.0, 1.0),
        )
        .unwrap();
        let children = [
            crate::canonical_authoring_scene::OrdinaryCompositionChild::WorldTransform {
                target: first.clone(),
                transform: first_end,
                options: noon_core::AnimationOptions::new(),
            },
            crate::canonical_authoring_scene::OrdinaryCompositionChild::WorldTransform {
                target: second.clone(),
                transform: second_end,
                options: noon_core::AnimationOptions::new(),
            },
        ];
        context
            .ordinary_play_mixed_composition(
                noon_core::SemanticAnimationCompositionKind::Parallel,
                &children,
                noon_core::AnimationOptions::new(),
                noon_core::AnimationOptions::new(),
            )
            .unwrap();
        assert_eq!(
            context.effective_world_transform(&first).unwrap(),
            first_end
        );
        assert_eq!(
            context.effective_world_transform(&second).unwrap(),
            second_end
        );
    }
}
