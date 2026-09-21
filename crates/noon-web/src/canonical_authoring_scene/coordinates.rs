//! Dispatch only to the currently owned execution player. In particular a
//! transferred context cannot create unpublished cold coordinate identities.
use super::*;

impl CanonicalAuthoringScene {
    pub(super) fn live_create_axes(
        &mut self,
        options: &noon::ManimAxesOptions,
    ) -> Result<noon::ManimAxes, AuthoringFailure> {
        self.active_live_player()?.live_create_axes(options)
    }

    pub(super) fn live_create_number_line(
        &mut self,
        options: &noon::ManimNumberLineOptions,
    ) -> Result<noon::ManimNumberLine, AuthoringFailure> {
        self.active_live_player()?.live_create_number_line(options)
    }

    pub(super) fn live_create_number_plane(
        &mut self,
        options: &noon::ManimNumberPlaneOptions,
    ) -> Result<noon::ManimNumberPlane, AuthoringFailure> {
        self.active_live_player()?.live_create_number_plane(options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options() -> noon::ManimAxesOptions {
        noon::ManimAxesOptions::new([-1.0, 1.0, 1.0], [-1.0, 1.0, 1.0], 4.0, 2.0)
    }

    #[test]
    fn coordinate_construction_rejects_unstarted_and_transferred_owners() {
        let mut context = CanonicalAuthoringScene::default();
        let before = context.scene.integration_store().borrow().len();
        assert!(context.live_create_axes(&options()).is_err());
        assert_eq!(context.scene.integration_store().borrow().len(), before);
        let player = context.take_execution_player(1.0, 41).unwrap();
        let identity = player.ownership_identity();
        let revision = context.scene.integration_store().borrow().scene_revision();
        assert!(context.live_create_axes(&options()).is_err());
        assert!(context
            .live_create_number_line(&noon::ManimNumberLineOptions::new([0.0, 2.0, 1.0]))
            .is_err());
        assert_eq!(context.scene.integration_store().borrow().len(), before);
        assert_eq!(
            context.scene.integration_store().borrow().scene_revision(),
            revision
        );
        context.return_execution_player(player).unwrap();
        let axes = context.live_create_axes(&options()).unwrap();
        assert_eq!(
            context.active_live_player().unwrap().ownership_identity(),
            identity
        );
        assert_eq!(context.live_execution_ownership(), "returned");
        assert_eq!(axes.x_axis().unwrap().range().unwrap(), [-1.0, 1.0, 1.0]);
        // Construction did not bind frontend IDs or attach to the visible root.
        assert!(context.bindings.is_empty());
        assert!(context.identities.is_empty());
        assert!(context
            .scene
            .integration_store()
            .borrow()
            .semantic_family_members_checked(context.scene.root())
            .unwrap()
            .is_empty());
    }

    #[test]
    fn returned_owner_can_create_after_wait_and_reject_invalid_input_without_reset() {
        let mut context = CanonicalAuthoringScene::default();
        let player = context.take_execution_player(1.0, 41).unwrap();
        let identity = player.ownership_identity();
        context.return_execution_player(player).unwrap();
        context.begin_ordinary_wait(0.25).unwrap();
        let mut player = context.resume_execution_player().unwrap();
        player.live_advance_segment_to(0.25).unwrap();
        player.live_complete_segment().unwrap();
        context.return_execution_player(player).unwrap();
        let mut invalid = options();
        invalid.y_length = f64::NAN;
        let revision = context.scene.integration_store().borrow().scene_revision();
        let count = context.scene.integration_store().borrow().len();
        assert!(context.live_create_axes(&invalid).is_err());
        assert_eq!(context.scene.integration_store().borrow().len(), count);
        assert_eq!(
            context.scene.integration_store().borrow().scene_revision(),
            revision
        );
        context.live_create_axes(&options()).unwrap();
        let line = context
            .live_create_number_line(&noon::ManimNumberLineOptions::new([2.0, 6.0, 1.0]))
            .unwrap();
        assert_eq!(line.range().unwrap(), [2.0, 6.0, 1.0]);
        let player = context.active_live_player().unwrap();
        assert_eq!(player.ownership_identity(), identity);
        assert_eq!(player.time(), 0.25);
    }

    #[test]
    fn number_plane_requires_returned_owner_and_stays_detached() {
        let mut context = CanonicalAuthoringScene::default();
        let options = noon::ManimNumberPlaneOptions::default();
        let count = context.scene.integration_store().borrow().len();
        let revision = context.scene.integration_store().borrow().scene_revision();
        assert!(context.live_create_number_plane(&options).is_err());
        assert_eq!(context.scene.integration_store().borrow().len(), count);
        assert_eq!(
            context.scene.integration_store().borrow().scene_revision(),
            revision
        );

        let player = context.take_execution_player(1.0, 41).unwrap();
        let identity = player.ownership_identity();
        let revision = context.scene.integration_store().borrow().scene_revision();
        assert!(context.live_create_number_plane(&options).is_err());
        assert_eq!(context.scene.integration_store().borrow().len(), count);
        assert_eq!(
            context.scene.integration_store().borrow().scene_revision(),
            revision
        );
        context.return_execution_player(player).unwrap();

        let plane = context.live_create_number_plane(&options).unwrap();
        assert_eq!(plane.x_axis().unwrap().range().unwrap(), options.x_range);
        assert_eq!(plane.y_axis().unwrap().range().unwrap(), options.y_range);
        assert_eq!(
            context.active_live_player().unwrap().ownership_identity(),
            identity
        );
        assert_eq!(context.live_execution_ownership(), "returned");
        assert!(context.bindings.is_empty());
        assert!(context.identities.is_empty());
        assert!(context
            .scene
            .integration_store()
            .borrow()
            .semantic_family_members_checked(context.scene.root())
            .unwrap()
            .is_empty());
    }

    #[test]
    fn number_plane_rejects_invalid_input_after_wait_without_reset_or_partial_publication() {
        let mut context = CanonicalAuthoringScene::default();
        let player = context.take_execution_player(1.0, 41).unwrap();
        let identity = player.ownership_identity();
        context.return_execution_player(player).unwrap();
        context.begin_ordinary_wait(0.25).unwrap();
        let mut player = context.resume_execution_player().unwrap();
        player.live_advance_segment_to(0.25).unwrap();
        player.live_complete_segment().unwrap();
        context.return_execution_player(player).unwrap();

        let invalid = noon::ManimNumberPlaneOptions {
            y_length: Some(f64::NAN),
            ..Default::default()
        };
        let revision = context.scene.integration_store().borrow().scene_revision();
        let count = context.scene.integration_store().borrow().len();
        let resources = context
            .scene
            .integration_store()
            .borrow()
            .geometry_resources()
            .len();
        assert!(context.live_create_number_plane(&invalid).is_err());
        assert_eq!(context.scene.integration_store().borrow().len(), count);
        assert_eq!(
            context.scene.integration_store().borrow().scene_revision(),
            revision
        );
        assert_eq!(
            context
                .scene
                .integration_store()
                .borrow()
                .geometry_resources()
                .len(),
            resources
        );

        let plane = context
            .live_create_number_plane(&noon::ManimNumberPlaneOptions::default())
            .unwrap();
        assert!(plane.authored_frame().is_ok());
        assert!(context.bindings.is_empty());
        assert!(context.identities.is_empty());
        assert!(context
            .scene
            .integration_store()
            .borrow()
            .semantic_family_members_checked(context.scene.root())
            .unwrap()
            .is_empty());
        let player = context.active_live_player().unwrap();
        assert_eq!(player.ownership_identity(), identity);
        assert_eq!(player.time(), 0.25);
    }
}
