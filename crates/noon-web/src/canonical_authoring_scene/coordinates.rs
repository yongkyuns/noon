//! Route coordinate authoring through the current owner; transferred contexts
//! cannot create unpublished cold identities.
use super::*;

impl CanonicalAuthoringScene {
    pub(crate) fn coordinate_scene(&self) -> Result<&noon::Scene, AuthoringFailure> {
        Ok(self.effective_observation_scene()?.unwrap_or(&self.scene))
    }

    pub(crate) fn number_line_frame(
        &self,
        line: &noon::ManimNumberLine,
    ) -> Result<noon::NumberLineFrame, AuthoringFailure> {
        self.coordinate_scene()?
            .number_line_frame(line)
            .map_err(crate::plot_error::coordinate_failure)
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn prepare_riemann_plan(
        &mut self,
        axes: &noon::ManimAxes,
        graph: &noon::Mobject,
        options: noon::RiemannRectangleOptions,
    ) -> Result<noon::RiemannRectanglePlan, AuthoringFailure> {
        self.local_authoring_scene()?
            .riemann_plan(axes, graph, options)
            .map_err(crate::plot_error::coordinate_failure)
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn publish_riemann_plan(
        &mut self,
        plan: &noon::RiemannRectanglePlan,
        top: Option<&[f64]>,
        baseline: Option<&[f64]>,
    ) -> Result<noon::MobjectFamily, AuthoringFailure> {
        let running = self.requires_live_mutation_routing();
        self.local_authoring_scene()?
            .publish_riemann_plan(plan, top, baseline)
            .map_err(|error| match error {
                noon::CoordinateAuthoringError::Authoring(error) => {
                    AuthoringFailure::from_scene_mutation(error, running)
                }
                error => crate::plot_error::coordinate_failure(error),
            })
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn create_bar_chart(
        &mut self,
        options: &noon::ManimBarChartOptions,
    ) -> Result<noon::ManimBarChart, AuthoringFailure> {
        let running = self.requires_live_mutation_routing();
        self.local_authoring_scene()?
            .bar_chart(options)
            .map_err(|error| {
                if running {
                    noon::integration::live_coordinate_publication_failure(error)
                } else {
                    crate::plot_error::coordinate_failure(error)
                }
            })
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn change_bar_values(
        &mut self,
        chart: &mut noon::ManimBarChart,
        values: &[f64],
        update_colors: bool,
    ) -> Result<(), AuthoringFailure> {
        let running = self.requires_live_mutation_routing();
        chart
            .change_bar_values(self.local_authoring_scene()?, values, update_colors)
            .map_err(|error| {
                if running {
                    noon::integration::live_coordinate_publication_failure(error)
                } else {
                    crate::plot_error::coordinate_failure(error)
                }
            })
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn create_labeled_bar_chart(
        &mut self,
        options: &noon::ManimBarChartOptions,
        labels: &noon::plot_presentation::NumberLabelOptions,
        compiler: &mut impl noon::LatexBackend,
    ) -> Result<noon::ManimBarChart, AuthoringFailure> {
        let running = self.requires_live_mutation_routing();
        self.local_authoring_scene()?
            .bar_chart_with_axis_labels(options, labels, compiler)
            .map_err(|error| {
                if running {
                    AuthoringFailure::new("invalid_input", "plot.bar_chart", error.to_string())
                } else {
                    AuthoringFailure::unclassified("bar_chart.create", &error)
                }
            })
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn bar_labels(
        &mut self,
        chart: &noon::ManimBarChart,
        compiler: &mut impl noon::LatexBackend,
        options: &noon::BarLabelOptions,
    ) -> Result<noon::MobjectFamily, AuthoringFailure> {
        let running = self.requires_live_mutation_routing();
        self.local_authoring_scene()?
            .bar_labels(chart, compiler, options)
            .map_err(|error| {
                if running {
                    AuthoringFailure::new("invalid_input", "plot.bar_labels", error.to_string())
                } else {
                    AuthoringFailure::unclassified("bar_chart.labels", &error)
                }
            })
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
        let near = |actual: [f64; 2], expected: [f64; 2]| {
            for (actual, expected) in actual.into_iter().zip(expected) {
                assert!((actual - expected).abs() < 1e-12, "{actual} != {expected}");
            }
        };
        for scale in [
            noon_core::SemanticNumberLineScale::default(),
            noon_core::SemanticNumberLineScale::linear(2.0),
            noon_core::SemanticNumberLineScale::log(10.0),
        ] {
            for include_tip in [false, true] {
                let first = scale.map(-1.0).unwrap();
                let last = scale.map(1.0).unwrap();
                let mut context = CanonicalAuthoringScene::default();
                let options = noon::ManimNumberLineOptions {
                    length: Some(2.0),
                    include_tip,
                    scale,
                    ..noon::ManimNumberLineOptions::new([-1.0, 1.0, 1.0])
                };
                let line = context.scene.number_line(&options).unwrap();
                let shaft = line.shaft().unwrap();
                let detached = context.number_line_frame(&line).unwrap();
                near(detached.number_to_point(first).unwrap(), [-1.0, 0.0]);
                context.bind_mobject(ObjectId::new(0), &shaft).unwrap();
                context.scene.add_many(&[line.family().into()]).unwrap();
                let frame = context.number_line_frame(&line).unwrap();
                near(frame.number_to_point(first).unwrap(), [-1.0, 0.0]);
                near(frame.number_to_point(last).unwrap(), [1.0, 0.0]);
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
                    .scene_mut()
                    .shift_family_3d(line.family(), noon::SemanticVec3::new(2.0, -1.0, 0.0))
                    .unwrap();
                let frame = context.number_line_frame(&line).unwrap();
                near(frame.number_to_point(first).unwrap(), [1.0, -1.0]);
                near(frame.number_to_point(last).unwrap(), [3.0, -1.0]);
                assert_eq!(frame.unit_size(), 1.0);
                assert_eq!(
                    frame.point_to_number([2.0, -1.0]).unwrap(),
                    scale.map(0.0).unwrap()
                );

                let foreign = CanonicalAuthoringScene::default();
                assert!(foreign.number_line_frame(&line).is_err(), "foreign store");
            }
        }
    }

    #[test]
    fn coordinate_construction_rejects_unstarted_and_transferred_owners() {
        let mut context = CanonicalAuthoringScene::default();
        let before = context.scene.integration_store().borrow().len();
        assert!(context.active_live_player().is_err());
        assert_eq!(context.scene.integration_store().borrow().len(), before);
        let player = context.take_execution_player(1.0, 41).unwrap();
        let identity = player.ownership_identity();
        let revision = context.scene.integration_store().borrow().scene_revision();
        assert!(context.active_live_player().is_err());
        assert_eq!(context.scene.integration_store().borrow().len(), before);
        assert_eq!(
            context.scene.integration_store().borrow().scene_revision(),
            revision
        );
        context.return_execution_player(player).unwrap();
        let axes = context
            .active_live_player()
            .unwrap()
            .scene_mut()
            .axes(&options())
            .unwrap();
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
        assert!(context
            .active_live_player()
            .unwrap()
            .scene_mut()
            .axes(&invalid)
            .is_err());
        assert_eq!(context.scene.integration_store().borrow().len(), count);
        assert_eq!(
            context.scene.integration_store().borrow().scene_revision(),
            revision
        );
        let scene = context.active_live_player().unwrap().scene_mut();
        scene.axes(&options()).unwrap();
        let line = scene
            .number_line(&noon::ManimNumberLineOptions::new([2.0, 6.0, 1.0]))
            .unwrap();
        assert_eq!(line.range().unwrap(), [2.0, 6.0, 1.0]);
        let player = context.active_live_player().unwrap();
        assert_eq!(player.ownership_identity(), identity);
        assert_eq!(player.time(), 0.25);
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
        assert!(context
            .active_live_player()
            .unwrap()
            .scene_mut()
            .number_plane(&invalid)
            .is_err());
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

        let scene = context.active_live_player().unwrap().scene_mut();
        let options = noon::ManimNumberPlaneOptions::default();
        let plane = scene.number_plane(&options).unwrap();
        assert_eq!(plane.x_axis().unwrap().range().unwrap(), options.x_range);
        assert_eq!(plane.y_axis().unwrap().range().unwrap(), options.y_range);
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
