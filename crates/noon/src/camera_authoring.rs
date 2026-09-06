use noon_core::{
    SemanticMutationImpact, SemanticMutationTransaction, SemanticNodeCreation, SemanticObjectRole,
    SemanticObjectState, SemanticStyle, StoredGeometry, Vec2, DEFAULT_FRAME_HEIGHT,
    DEFAULT_FRAME_WIDTH,
};

use crate::{Mobject, Scene};

impl Scene {
    /// Create and attach the one ordinary semantic object that defines this scene's 2D camera.
    ///
    /// Camera creation is scene initialization: the root must still be empty. Allocation and root
    /// membership then commit in one semantic transaction. The frame remains an ordinary
    /// transformable Mobject; its role only tells lowering which effective transform supplies the
    /// renderer viewport.
    pub fn camera_frame(&mut self) -> Result<Mobject, String> {
        let store = self.store().borrow();
        let root_is_empty = store
            .node(self.root())
            .ok_or("scene root is unavailable")?
            .members()
            .is_empty();
        drop(store);
        if !root_is_empty {
            return Err("2D camera frame must be created before scene content".into());
        }

        let mut state = SemanticObjectState::new(StoredGeometry::Rectangle {
            size: Vec2::new(DEFAULT_FRAME_WIDTH, DEFAULT_FRAME_HEIGHT),
        });
        state.style = SemanticStyle {
            object_opacity: 0.0,
            ..SemanticStyle::default()
        };
        state.set_role(SemanticObjectRole::Camera2D);

        let mut transaction = SemanticMutationTransaction::new();
        let frame = transaction.create_node(SemanticNodeCreation::object(state));
        transaction.add_member(self.root(), frame);
        let result = transaction
            .apply(&mut self.store().borrow_mut())
            .map_err(|error| error.to_string())?;
        let id = result
            .resolve(frame)
            .ok_or("camera-frame transaction returned no semantic identity")?;
        debug_assert!(matches!(
            result.impacts(),
            [
                SemanticMutationImpact::NodeAdded { .. },
                SemanticMutationImpact::FamilyMemberAdded { .. }
            ]
        ));
        Mobject::from_node(std::rc::Rc::clone(self.store()), id)
    }
}

#[cfg(test)]
mod tests {
    use noon_core::{
        AnimationOptions, Camera2DState, RateFunction, SemanticObjectRole, Vec2,
        DEFAULT_FRAME_HEIGHT,
    };

    use super::*;

    #[test]
    fn camera_creation_is_atomic_unique_and_uses_the_scene_identity_space() {
        let mut scene = Scene::new();
        let revision = scene.store().borrow().scene_revision();
        let frame = scene.camera_frame().unwrap();
        let state = frame.state().unwrap();
        assert_eq!(state.role(), SemanticObjectRole::Camera2D);
        assert_eq!(state.style.object_opacity, 0.0);
        assert_eq!(
            scene.store().borrow().node(scene.root()).unwrap().members(),
            [frame.node_id()]
        );
        assert_ne!(scene.store().borrow().scene_revision(), revision);

        let before = scene.store().borrow().scene_revision();
        assert_eq!(
            scene.camera_frame().unwrap_err(),
            "2D camera frame must be created before scene content"
        );
        assert_eq!(scene.store().borrow().scene_revision(), before);
    }

    #[test]
    fn camera_creation_rejects_existing_scene_content_without_mutation() {
        let mut scene = Scene::new();
        let square = scene.square(2.0).unwrap();
        scene.add(&square).unwrap();
        let before = scene.store().borrow().scene_revision();

        assert_eq!(
            scene.camera_frame().unwrap_err(),
            "2D camera frame must be created before scene content"
        );
        assert_eq!(scene.store().borrow().scene_revision(), before);
        assert_eq!(
            scene.store().borrow().node(scene.root()).unwrap().members(),
            [square.node_id()]
        );
    }

    #[test]
    fn ordinary_transform_drives_the_effective_camera_and_publishes_its_endpoint() {
        let mut scene = Scene::new();
        let frame = scene.camera_frame().unwrap();
        let mut target = frame.target_editor().unwrap();
        target.set_translation(-2.0, 0.0).unwrap();
        let mut session = scene.execution_session().unwrap();
        assert_eq!(
            session.camera().unwrap(),
            Camera2DState {
                center: Vec2::ZERO,
                height: DEFAULT_FRAME_HEIGHT,
            }
        );

        let segment = scene
            .live(&mut session)
            .declare_and_activate_transform_to(
                &frame,
                &target,
                AnimationOptions::new()
                    .run_time(1.0)
                    .rate_func(RateFunction::Linear),
            )
            .unwrap();
        scene
            .live(&mut session)
            .advance_segment_to(segment, 0.5)
            .unwrap();
        assert_eq!(session.camera().unwrap().center, Vec2::new(-1.0, 0.0));
        scene
            .live(&mut session)
            .advance_segment_to(segment, segment.end_time())
            .unwrap();
        scene.live(&mut session).complete_segment(segment).unwrap();
        assert_eq!(session.camera().unwrap().center, Vec2::new(-2.0, 0.0));
        assert_eq!(frame.state().unwrap().transform.translation.x, -2.0);
    }
}
