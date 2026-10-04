//! Canonical scene bindings for finite Manim camera profiles.

use super::{CanonicalAuthoringScene, PlayerOwnership};

impl CanonicalAuthoringScene {
    pub(crate) fn effective_camera_profile(
        &mut self,
        object: &noon::Mobject,
    ) -> Result<(noon_core::ManimCamera3DProfile, f64, f64), crate::authoring_error::AuthoringFailure>
    {
        if !std::rc::Rc::ptr_eq(self.scene.integration_store(), object.integration_store()) {
            return Err(noon::AuthoringError::ForeignStore.into());
        }
        object.validate()?;
        match &mut self.player_ownership {
            PlayerOwnership::Unstarted => self
                .scene
                .effective_camera_profile(object)
                .map_err(Into::into),
            PlayerOwnership::Active(_) | PlayerOwnership::Returned(_) => self
                .active_live_player()?
                .live_effective_camera_profile(object),
            PlayerOwnership::Transferred(_) => {
                Err("effective camera profile is owned by the transferred execution session".into())
            }
        }
    }

    pub(crate) fn set_camera_profile(
        &mut self,
        object: &noon::Mobject,
        profile: noon_core::ManimCamera3DProfile,
    ) -> Result<(), crate::authoring_error::AuthoringFailure> {
        if !std::rc::Rc::ptr_eq(self.scene.integration_store(), object.integration_store()) {
            return Err(noon::AuthoringError::ForeignStore.into());
        }
        object.validate()?;
        match &mut self.player_ownership {
            PlayerOwnership::Unstarted => self
                .scene
                .set_camera_profile(object, profile)
                .map_err(Into::into),
            PlayerOwnership::Active(_) | PlayerOwnership::Returned(_) => self
                .active_live_player()?
                .live_set_camera_profile(object, profile),
            PlayerOwnership::Transferred(_) => {
                Err("live execution session is running in the semantic engine".into())
            }
        }
    }

    pub(crate) fn begin_ambient_camera_rotation(
        &mut self,
        object: &noon::Mobject,
        axis: noon_core::CameraRotationAxis,
        rate: f64,
    ) -> Result<(), crate::authoring_error::AuthoringFailure> {
        if !std::rc::Rc::ptr_eq(self.scene.integration_store(), object.integration_store()) {
            return Err(noon::AuthoringError::ForeignStore.into());
        }
        object.validate()?;
        match &mut self.player_ownership {
            PlayerOwnership::Unstarted => self
                .scene
                .begin_ambient_camera_rotation(object, axis, rate)
                .map_err(Into::into),
            PlayerOwnership::Active(_) | PlayerOwnership::Returned(_) => self
                .active_live_player()?
                .live_begin_ambient_camera_rotation(object, axis, rate),
            PlayerOwnership::Transferred(_) => {
                Err("live execution session is running in the semantic engine".into())
            }
        }
    }

    pub(crate) fn stop_ambient_camera_rotation(
        &mut self,
        object: &noon::Mobject,
    ) -> Result<(), crate::authoring_error::AuthoringFailure> {
        if !std::rc::Rc::ptr_eq(self.scene.integration_store(), object.integration_store()) {
            return Err(noon::AuthoringError::ForeignStore.into());
        }
        object.validate()?;
        match &mut self.player_ownership {
            PlayerOwnership::Unstarted => self
                .scene
                .stop_ambient_camera_rotation(object)
                .map_err(Into::into),
            PlayerOwnership::Active(_) | PlayerOwnership::Returned(_) => self
                .active_live_player()?
                .live_stop_ambient_camera_rotation(object),
            PlayerOwnership::Transferred(_) => {
                Err("live execution session is running in the semantic engine".into())
            }
        }
    }

    pub(crate) fn begin_3dillusion_camera_rotation(
        &mut self,
        object: &noon::Mobject,
        rate: f64,
        origin_phi: Option<f64>,
        origin_theta: Option<f64>,
    ) -> Result<(), crate::authoring_error::AuthoringFailure> {
        if !std::rc::Rc::ptr_eq(self.scene.integration_store(), object.integration_store()) {
            return Err(noon::AuthoringError::ForeignStore.into());
        }
        object.validate()?;
        match &mut self.player_ownership {
            PlayerOwnership::Unstarted => self
                .scene
                .begin_3dillusion_camera_rotation(object, rate, origin_phi, origin_theta)
                .map_err(Into::into),
            PlayerOwnership::Active(_) | PlayerOwnership::Returned(_) => self
                .active_live_player()?
                .live_begin_3dillusion_camera_rotation(object, rate, origin_phi, origin_theta),
            PlayerOwnership::Transferred(_) => {
                Err("live execution session is running in the semantic engine".into())
            }
        }
    }

    pub(crate) fn stop_3dillusion_camera_rotation(
        &mut self,
        object: &noon::Mobject,
    ) -> Result<(), crate::authoring_error::AuthoringFailure> {
        if !std::rc::Rc::ptr_eq(self.scene.integration_store(), object.integration_store()) {
            return Err(noon::AuthoringError::ForeignStore.into());
        }
        object.validate()?;
        match &mut self.player_ownership {
            PlayerOwnership::Unstarted => self
                .scene
                .stop_3dillusion_camera_rotation(object)
                .map_err(Into::into),
            PlayerOwnership::Active(_) | PlayerOwnership::Returned(_) => self
                .active_live_player()?
                .live_stop_3dillusion_camera_rotation(object),
            PlayerOwnership::Transferred(_) => {
                Err("live execution session is running in the semantic engine".into())
            }
        }
    }

    pub(crate) fn create_camera_profile(
        &mut self,
        id: noon_core::ObjectId,
        profile: noon_core::ManimCamera3DProfile,
        near: f64,
        far: f64,
    ) -> Result<noon::Mobject, crate::authoring_error::AuthoringFailure> {
        if !self.player_ownership.is_unstarted() {
            return Err("3D camera must be declared before execution starts".into());
        }
        if self.bindings.contains_key(&id) {
            return Err(format!("canonical object {} is already bound", id.get()).into());
        }
        let object = self.scene.camera_3d_profile(profile, near, far)?;
        let node = object.node_id();
        self.bindings.insert(id, node);
        self.identities.insert(node, id);
        Ok(object)
    }

    pub(crate) fn set_camera_orientation(
        &mut self,
        object: &noon::Mobject,
        phi: f64,
        theta: f64,
        gamma: f64,
    ) -> Result<(), crate::authoring_error::AuthoringFailure> {
        if !std::rc::Rc::ptr_eq(self.scene.integration_store(), object.integration_store()) {
            return Err(noon::AuthoringError::ForeignStore.into());
        }
        object.validate()?;
        match &mut self.player_ownership {
            PlayerOwnership::Unstarted => self
                .scene
                .set_camera_orientation(object, phi, theta, gamma)
                .map_err(Into::into),
            PlayerOwnership::Active(_) | PlayerOwnership::Returned(_) => self
                .active_live_player()?
                .live_set_camera_orientation(object, phi, theta, gamma),
            PlayerOwnership::Transferred(_) => {
                Err("live execution session is running in the semantic engine".into())
            }
        }
    }

    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn declare_camera_profile_move(
        &self,
        object: &noon::Mobject,
        profile: noon_core::ManimCamera3DProfile,
        options: noon_core::AnimationOptions,
    ) -> Result<noon::DeclaredAnimation, String> {
        if !self.player_ownership.is_unstarted() {
            return Err(
                "declare camera profile tracks before execution starts; use a live composition"
                    .into(),
            );
        }
        self.scene
            .declare_camera_profile_move(object, profile, options)
    }

    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn move_camera_profile(
        &mut self,
        object: &noon::Mobject,
        profile: noon_core::ManimCamera3DProfile,
        options: noon_core::AnimationOptions,
    ) -> Result<f64, String> {
        if !std::rc::Rc::ptr_eq(self.scene.integration_store(), object.integration_store()) {
            return Err(noon::AuthoringError::ForeignStore.to_string());
        }
        object.validate().map_err(|error| error.to_string())?;
        let child = super::OrdinaryCompositionChild::CameraProfile {
            target: object.clone(),
            profile,
            options,
        };
        self.ordinary_play_mixed_composition(
            noon_core::SemanticAnimationCompositionKind::Sequence,
            &[child],
            noon_core::AnimationOptions::new(),
            noon_core::AnimationOptions::new(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use noon_core::{
        AnimationOptions, CameraRotationAxis, ManimCamera3DProfile, SemanticAnimationIntent,
        SemanticVec3,
    };

    fn profile() -> ManimCamera3DProfile {
        ManimCamera3DProfile {
            phi: 0.7,
            theta: -1.1,
            gamma: 0.2,
            focal_distance: 5.0,
            zoom: 1.0,
            frame_height: 8.0,
            frame_center: SemanticVec3::new(0.0, 0.0, 0.0),
        }
    }

    #[test]
    fn canonical_camera_profile_creation_and_orientation_use_shared_scene_state() {
        let mut context = CanonicalAuthoringScene::default();
        let id = noon_core::ObjectId::new(12);
        let camera = context
            .create_camera_profile(id, profile(), 0.1, 100.0)
            .unwrap();
        assert_eq!(context.bindings.get(&id), Some(&camera.node_id()));
        assert_eq!(camera.state().unwrap().camera_profile(), Some(profile()));
        context
            .set_camera_orientation(&camera, 1.0, 4.0, -0.25)
            .unwrap();
        let state = camera.state().unwrap();
        assert_eq!(state.camera_profile().unwrap().phi, 1.0);
        assert_eq!(state.camera_profile().unwrap().theta, 4.0);
        assert_eq!(state.camera_profile().unwrap().gamma, -0.25);
    }

    #[test]
    fn ambient_profile_uses_one_canonical_player_and_orientation_remains_persistent() {
        let mut context = CanonicalAuthoringScene::default();
        let camera = context
            .create_camera_profile(noon_core::ObjectId::new(13), profile(), 0.1, 100.0)
            .unwrap();
        assert_eq!(
            context.effective_camera_profile(&camera).unwrap(),
            (profile(), 0.1, 100.0)
        );
        assert_eq!(context.live_execution_ownership(), "none");

        let mut authored = profile();
        authored.zoom = 1.75;
        authored.frame_center = SemanticVec3::new(0.25, -0.5, 0.75);
        context.set_camera_profile(&camera, authored).unwrap();
        assert_eq!(
            context.effective_camera_profile(&camera).unwrap(),
            (authored, 0.1, 100.0)
        );

        assert_eq!(context.ordinary_wait(0.25).unwrap(), 0.25);
        let identity = context.active_live_player().unwrap().ownership_identity();
        context
            .begin_ambient_camera_rotation(&camera, CameraRotationAxis::Theta, 0.5)
            .unwrap();
        assert_eq!(context.ordinary_wait(1.0).unwrap(), 1.25);
        let mut expected = authored;
        expected.theta += 0.5;
        assert_eq!(
            context.effective_camera_profile(&camera).unwrap(),
            (expected, 0.1, 100.0)
        );

        context.stop_ambient_camera_rotation(&camera).unwrap();
        assert_eq!(camera.state().unwrap().camera_profile(), Some(expected));
        assert_eq!(
            context.effective_camera_profile(&camera).unwrap(),
            (expected, 0.1, 100.0)
        );

        // A closed ambient history does not retain ownership over later edits.
        context
            .set_camera_orientation(&camera, 1.2, -0.7, 0.3)
            .unwrap();
        let mut oriented = expected;
        oriented.phi = 1.2;
        oriented.theta = -0.7;
        oriented.gamma = 0.3;
        assert_eq!(
            context.effective_camera_profile(&camera).unwrap(),
            (oriented, 0.1, 100.0)
        );
        assert_eq!(context.ordinary_wait(0.2).unwrap(), 1.45);
        assert_eq!(context.active_live_player().unwrap().time(), 1.45);
        assert_eq!(
            context.active_live_player().unwrap().ownership_identity(),
            identity
        );
        assert_eq!(
            context.effective_camera_profile(&camera).unwrap(),
            (oriented, 0.1, 100.0)
        );
    }

    #[test]
    fn declared_and_live_profile_moves_share_composition_and_the_existing_player() {
        let mut declaration = CanonicalAuthoringScene::default();
        let camera = declaration
            .create_camera_profile(noon_core::ObjectId::new(14), profile(), 0.1, 100.0)
            .unwrap();
        let mut declared_endpoint = profile();
        declared_endpoint.theta += std::f64::consts::TAU;
        declared_endpoint.zoom = 1.5;
        let declared = declaration
            .declare_camera_profile_move(
                &camera,
                declared_endpoint,
                AnimationOptions::new().run_time(0.75),
            )
            .unwrap();
        assert!(declaration.player_ownership.is_unstarted());
        assert!(matches!(
            declaration
                .scene
                .integration_store()
                .borrow()
                .semantic_animation_state(declared.node_id())
                .unwrap()
                .intent(),
            SemanticAnimationIntent::CameraProfileTo { target, profile }
                if *target == camera.node_id() && *profile == declared_endpoint
        ));

        let mut live = CanonicalAuthoringScene::default();
        let live_camera = live
            .create_camera_profile(noon_core::ObjectId::new(15), profile(), 0.1, 100.0)
            .unwrap();
        assert_eq!(live.ordinary_wait(0.25).unwrap(), 0.25);
        let identity = live.active_live_player().unwrap().ownership_identity();
        let mut endpoint = profile();
        endpoint.phi = 1.4;
        endpoint.focal_distance = 7.0;
        endpoint.zoom = 2.0;
        endpoint.frame_height = 6.0;
        endpoint.frame_center = SemanticVec3::new(-1.0, 0.5, 2.0);
        assert_eq!(
            live.move_camera_profile(
                &live_camera,
                endpoint,
                AnimationOptions::new().run_time(1.25),
            )
            .unwrap(),
            1.5
        );
        assert_eq!(
            live.effective_camera_profile(&live_camera).unwrap(),
            (endpoint, 0.1, 100.0)
        );
        assert_eq!(live.active_live_player().unwrap().time(), 1.5);
        assert_eq!(
            live.active_live_player().unwrap().ownership_identity(),
            identity
        );
        assert_eq!(live.live_execution_ownership(), "active");

        // Another ordinary segment advances the existing player without resetting the goal.
        assert_eq!(live.ordinary_wait(0.2).unwrap(), 1.7);
        assert_eq!(
            live.effective_camera_profile(&live_camera).unwrap(),
            (endpoint, 0.1, 100.0)
        );
        assert_eq!(
            live.active_live_player().unwrap().ownership_identity(),
            identity
        );
    }
}
