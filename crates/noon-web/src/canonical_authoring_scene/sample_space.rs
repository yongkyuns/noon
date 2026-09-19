//! Live SampleSpace operations dispatch through the currently retained player.
use super::*;

impl CanonicalAuthoringScene {
    pub(crate) fn live_create_sample_space(
        &mut self,
        options: &noon::SampleSpaceOptions,
    ) -> Result<noon::SampleSpace, AuthoringFailure> {
        self.active_live_player()?.live_create_sample_space(options)
    }

    pub(crate) fn live_get_sample_space_division(
        &mut self,
        sample_space: &noon::SampleSpace,
        probabilities: &[f64],
        colors: &[noon::Color],
        vertical: bool,
    ) -> Result<noon::MobjectFamily, AuthoringFailure> {
        self.active_live_player()?.live_get_sample_space_division(
            sample_space,
            probabilities,
            colors,
            vertical,
        )
    }

    pub(crate) fn live_divide_sample_space(
        &mut self,
        sample_space: &mut noon::SampleSpace,
        probabilities: &[f64],
        colors: &[noon::Color],
        vertical: bool,
    ) -> Result<noon::MobjectFamily, AuthoringFailure> {
        self.active_live_player()?.live_divide_sample_space(
            sample_space,
            probabilities,
            colors,
            vertical,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_sample_space_construction_and_division_publish_atomically() {
        let mut context = CanonicalAuthoringScene::default();
        let options = noon::SampleSpaceOptions::default();
        let player = context.take_execution_player(1.0, 77).unwrap();
        let identity = player.ownership_identity();
        assert!(context.live_create_sample_space(&options).is_err());
        context.return_execution_player(player).unwrap();

        let before = context.scene.integration_store().borrow().scene_revision();
        let mut space = context.live_create_sample_space(&options).unwrap();
        let constructed = context.scene.integration_store().borrow().scene_revision();
        assert_ne!(constructed, before);
        assert_eq!(
            context.active_live_player().unwrap().ownership_identity(),
            identity
        );

        let rejected_revision = context.scene.integration_store().borrow().scene_revision();
        let rejected_len = context.scene.integration_store().borrow().len();
        assert!(
            context
                .live_divide_sample_space(&mut space, &[0.7, 0.5], &[noon::BLUE], false)
                .is_err()
        );
        assert_eq!(
            context.scene.integration_store().borrow().scene_revision(),
            rejected_revision
        );
        assert_eq!(
            context.scene.integration_store().borrow().len(),
            rejected_len
        );
        assert!(space.horizontal_parts().unwrap().is_none());

        let parts = context
            .live_divide_sample_space(&mut space, &[0.25, 0.5], &[noon::RED, noon::BLUE], false)
            .unwrap();
        assert_ne!(
            context.scene.integration_store().borrow().scene_revision(),
            rejected_revision
        );
        assert_eq!(
            context
                .scene
                .integration_store()
                .borrow()
                .semantic_family_members_checked(parts.node_id())
                .unwrap()
                .len(),
            3
        );
        assert_eq!(
            space.horizontal_parts().unwrap().unwrap().node_id(),
            parts.node_id()
        );
        assert_eq!(
            context.active_live_player().unwrap().ownership_identity(),
            identity
        );
    }

    #[test]
    fn live_get_division_uses_current_effective_world_axis_bounds_without_attaching() {
        let mut context = CanonicalAuthoringScene::default();
        let player = context.take_execution_player(1.0, 78).unwrap();
        context.return_execution_player(player).unwrap();
        let space = context
            .live_create_sample_space(&noon::SampleSpaceOptions::default())
            .unwrap();
        let family = space.family().clone();
        let family_target = noon::MobjectTarget::Family(&family);
        context
            .active_live_player()
            .unwrap()
            .live_edit_membership(noon::SceneMembershipRequest::Add(&[family_target]))
            .unwrap();
        let rectangle = space.rectangle().clone();
        context
            .active_live_player()
            .unwrap()
            .live_move_to(
                &rectangle,
                noon::LiveLayoutTarget::Point(2.0, -1.0),
                (0.0, 0.0),
                (1.0, 1.0),
            )
            .unwrap();
        let effective = context
            .active_live_player()
            .unwrap()
            .live_effective(&rectangle)
            .unwrap();
        assert!((f64::from(effective.transform.translation.x) - 2.0).abs() < 1e-6);
        assert!((f64::from(effective.transform.translation.y) + 1.0).abs() < 1e-6);
        let revision = context.scene.integration_store().borrow().scene_revision();
        let parts = context
            .live_get_sample_space_division(&space, &[0.5], &[noon::GREEN], true)
            .unwrap();
        assert_ne!(
            context.scene.integration_store().borrow().scene_revision(),
            revision
        );
        assert!(space.vertical_parts().unwrap().is_none());
        let layout = parts.layout().unwrap();
        assert!((layout.center().0 - 2.0).abs() < 1e-6);
        assert!((layout.center().1 + 1.0).abs() < 1e-6);
        // A single 0.5 probability is completed with its 0.5 remainder,
        // so the two vertical leaves span the full effective rectangle.
        assert!((layout.width() - 3.0).abs() < 1e-6);
        let bounds = layout.bounds().unwrap();
        assert!((bounds.min_x - 0.5).abs() < 1e-6);
        assert!((bounds.max_x - 3.5).abs() < 1e-6);
    }
}
