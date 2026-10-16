//! Route coordinate authoring through the current owner; transferred contexts
//! cannot create unpublished cold identities.
use super::*;

impl CanonicalAuthoringScene {
    pub(crate) fn number_line_frame(
        &mut self,
        line: &noon::ManimNumberLine,
    ) -> Result<noon::NumberLineFrame, AuthoringFailure> {
        let shaft = line
            .shaft()
            .map_err(crate::plot_error::coordinate_failure)?;
        self.mobject_observation(
            &shaft,
            |_| {
                line.authored_frame()
                    .map_err(crate::plot_error::coordinate_failure)
            },
            |player, _| player.live_effective_number_line_frame(line),
        )
    }

    #[cfg(target_arch = "wasm32")]
    pub(super) fn live_create_path_family(
        &mut self,
        paths: Vec<(noon::VectorPath, noon_core::SemanticStyle)>,
    ) -> Result<noon::MobjectFamily, AuthoringFailure> {
        self.active_live_player()?.live_create_path_family(paths)
    }

    #[cfg(target_arch = "wasm32")]
    pub(super) fn live_create_polar_plane(
        &mut self,
        options: &noon::ManimPolarPlaneOptions,
    ) -> Result<noon::ManimPolarPlane, AuthoringFailure> {
        self.active_live_player()?.live_create_polar_plane(options)
    }

    pub(super) fn live_create_axes(
        &mut self,
        options: &noon::ManimAxesOptions,
    ) -> Result<noon::ManimAxes, AuthoringFailure> {
        self.active_live_player()?.live_create_axes(options)
    }

    #[cfg(target_arch = "wasm32")]
    pub(super) fn live_create_three_d_axes(
        &mut self,
        options: &noon::ManimThreeDAxesOptions,
    ) -> Result<noon::ManimThreeDAxes, AuthoringFailure> {
        self.active_live_player()?.live_create_three_d_axes(options)
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_create_bar_chart(
        &mut self,
        options: &noon::ManimBarChartOptions,
    ) -> Result<noon::ManimBarChart, AuthoringFailure> {
        if self.player_ownership.is_unstarted() {
            return self
                .scene
                .bar_chart(options)
                .map_err(crate::plot_error::coordinate_failure);
        }
        self.active_live_player()?.live_create_bar_chart(options)
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_change_bar_values(
        &mut self,
        chart: &mut noon::ManimBarChart,
        values: &[f64],
        update_colors: bool,
    ) -> Result<(), AuthoringFailure> {
        self.active_live_player()?
            .live_change_bar_values(chart, values, update_colors)
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_create_labeled_bar_chart(
        &mut self,
        options: &noon::ManimBarChartOptions,
        labels: &noon::plot_presentation::NumberLabelOptions,
        compiler: &mut impl noon::LatexBackend,
    ) -> Result<noon::ManimBarChart, AuthoringFailure> {
        if self.player_ownership.is_unstarted() {
            return self
                .scene
                .bar_chart_with_axis_labels(options, labels, compiler)
                .map_err(|error| AuthoringFailure::unclassified("bar_chart.create", &error));
        }
        self.active_live_player()?
            .live_create_labeled_bar_chart(options, labels, compiler)
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_bar_labels(
        &mut self,
        chart: &noon::ManimBarChart,
        compiler: &mut impl noon::LatexBackend,
        options: &noon::BarLabelOptions,
    ) -> Result<noon::MobjectFamily, AuthoringFailure> {
        if self.player_ownership.is_unstarted() {
            return chart
                .get_bar_labels(compiler, options)
                .map_err(|error| AuthoringFailure::unclassified("bar_chart.labels", &error));
        }
        self.active_live_player()?
            .live_bar_labels(chart, compiler, options)
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
    fn coordinate_queries_preserve_tip_endpoints_through_player_ownership() {
        for include_tip in [false, true] {
            let mut context = CanonicalAuthoringScene::default();
            let options = noon::ManimNumberLineOptions {
                length: Some(2.0),
                include_tip,
                ..noon::ManimNumberLineOptions::new([-1.0, 1.0, 1.0])
            };
            let line = context.scene.number_line(&options).unwrap();
            let shaft = line.shaft().unwrap();
            assert!(context.number_line_frame(&line).is_err(), "unbound shaft");
            context.bind_mobject(ObjectId::new(0), &shaft).unwrap();
            context.scene.add_many(&[line.family().into()]).unwrap();
            let frame = context.number_line_frame(&line).unwrap();
            assert_eq!(frame.number_to_point(-1.0).unwrap(), [-1.0, 0.0]);
            assert_eq!(frame.number_to_point(1.0).unwrap(), [1.0, 0.0]);
            if include_tip {
                assert!(shaft.path_query().unwrap().end().unwrap().0 < 1.0);
            }

            let player = context.take_execution_player(1.0, 41).unwrap();
            assert!(
                context.number_line_frame(&line).is_err(),
                "transferred owner"
            );
            context.return_execution_player(player).unwrap();
            context
                .active_live_player()
                .unwrap()
                .live_shift_family(line.family(), 2.0, -1.0)
                .unwrap();
            let frame = context.number_line_frame(&line).unwrap();
            assert_eq!(frame.number_to_point(-1.0).unwrap(), [1.0, -1.0]);
            assert_eq!(frame.number_to_point(1.0).unwrap(), [3.0, -1.0]);
            assert_eq!(frame.unit_size(), 1.0);

            let mut foreign = CanonicalAuthoringScene::default();
            assert!(foreign.number_line_frame(&line).is_err(), "foreign store");
        }
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
