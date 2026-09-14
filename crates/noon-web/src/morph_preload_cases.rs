#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::{
        RenderGeometryPreparation, RetainedFamilyExecutionDeltaEnvelope, RetainedResourceBundle,
        SemanticExecutionPlayer,
    };
    use noon::{AnimationCompositionRequest, TransformToRequest};
    use noon_core::{
        AnimationOptions, FontResourceArena, GeometryRef, GeometryResourceArena, RateFunction,
        Style, TextResourceArena, Transform2D, Vec2, VectorPath,
    };

    fn empty_bundle() -> RetainedResourceBundle {
        RetainedResourceBundle::capture(
            [],
            &TextResourceArena::new(),
            &GeometryResourceArena::new(),
            &FontResourceArena::new(),
        )
        .unwrap()
    }

    fn retained_path(x: f32) -> Arc<GeometryRef> {
        Arc::new(GeometryRef::path(
            VectorPath::new()
                .move_to(Vec2::new(x, 0.0))
                .line_to(Vec2::new(x + 1.0, 1.0)),
        ))
    }

    #[test]
    fn live_morph_publishes_renderer_resource_before_first_active_frame() {
        let mut scene = noon::Scene::new();
        let source = scene.circle(1.0).unwrap();
        let target = scene.rectangle(2.0, 1.0).unwrap();
        scene.add(&source).unwrap();

        let mut player = SemanticExecutionPlayer::from_live_session(
            scene.execution_session().unwrap(),
            std::rc::Rc::clone(scene.integration_store()),
            scene.root(),
            2.0,
            91,
        )
        .unwrap();

        let initial_bundle = RetainedResourceBundle::decode_binary(&player.resource_bundle_bytes())
            .unwrap()
            .install()
            .unwrap();
        assert_eq!(initial_bundle.render_geometries().len(), 0);
        player.initial_delta_json().unwrap();

        player
            .live_declare_and_activate_composition(
                &AnimationCompositionRequest::TransformTo(TransformToRequest::new(
                    &source,
                    &target,
                    AnimationOptions::new()
                        .run_time(1.0)
                        .rate_func(RateFunction::Linear),
                )),
                AnimationOptions::new(),
            )
            .unwrap();

        let json = player
            .drain_delta_json()
            .unwrap()
            .expect("activation must publish one retained update");
        let delta: RetainedFamilyExecutionDeltaEnvelope = serde_json::from_str(&json).unwrap();
        assert!(
            delta.resource_additions.is_some(),
            "first morph activation must publish immutable renderer resources"
        );
        assert!(
            json.contains("render_geometry_resources"),
            "morph geometry must be installed before its first rendered frame"
        );

        let morph_row = delta
            .retained
            .objects
            .iter()
            .find(|row| row.render_transform.is_some())
            .expect("transform activation must publish a renderer geometry row");
        assert!(morph_row.render_geometry_resource.is_some());
        assert!(morph_row.render_geometry.is_none());
    }

    #[test]
    fn second_live_morph_appends_a_new_renderer_resource_after_first_residency() {
        let mut scene = noon::Scene::new();
        let source = scene.circle(1.0).unwrap();
        let first_target = scene.rectangle(2.0, 1.0).unwrap();
        let second_target = scene.circle(0.75).unwrap();
        scene.add(&source).unwrap();

        let mut player = SemanticExecutionPlayer::from_live_session(
            scene.execution_session().unwrap(),
            std::rc::Rc::clone(scene.integration_store()),
            scene.root(),
            3.0,
            92,
        )
        .unwrap();

        let initial_bundle = RetainedResourceBundle::decode_binary(&player.resource_bundle_bytes())
            .unwrap()
            .install()
            .unwrap();
        assert_eq!(initial_bundle.render_geometries().len(), 0);
        player.initial_delta_json().unwrap();

        let first_end = player
            .live_declare_and_activate_composition(
                &AnimationCompositionRequest::TransformTo(TransformToRequest::new(
                    &source,
                    &first_target,
                    AnimationOptions::new()
                        .run_time(1.0)
                        .rate_func(RateFunction::Linear),
                )),
                AnimationOptions::new(),
            )
            .unwrap();
        let first_json = player
            .drain_delta_json()
            .unwrap()
            .expect("first activation must publish one retained update");
        let first_delta: RetainedFamilyExecutionDeltaEnvelope =
            serde_json::from_str(&first_json).unwrap();
        assert!(first_delta.resource_additions.is_some());
        let first_row = first_delta
            .retained
            .objects
            .iter()
            .find(|row| row.render_transform.is_some())
            .expect("first transform activation must publish a renderer geometry row");
        let first_resource = first_row
            .render_geometry_resource
            .expect("first morph must reference its retained renderer resource");
        assert_eq!(first_resource, 0);
        assert!(first_row.render_geometry.is_none());

        player.live_advance_segment_to(first_end).unwrap();
        let _ = player.drain_delta_json().unwrap();
        player.live_complete_segment().unwrap();
        let _ = player.drain_delta_json().unwrap();

        player
            .live_declare_and_activate_composition(
                &AnimationCompositionRequest::TransformTo(TransformToRequest::new(
                    &source,
                    &second_target,
                    AnimationOptions::new()
                        .run_time(1.0)
                        .rate_func(RateFunction::Linear),
                )),
                AnimationOptions::new(),
            )
            .unwrap();
        let second_json = player
            .drain_delta_json()
            .unwrap()
            .expect("second activation must publish one retained update");
        let second_delta: RetainedFamilyExecutionDeltaEnvelope =
            serde_json::from_str(&second_json).unwrap();
        assert!(
            second_delta.resource_additions.is_some(),
            "second morph activation must append immutable renderer resources"
        );
        let second_row = second_delta
            .retained
            .objects
            .iter()
            .find(|row| row.render_transform.is_some())
            .expect("second transform activation must publish a renderer geometry row");
        let second_resource = second_row
            .render_geometry_resource
            .expect("second morph must reference its appended renderer resource");
        assert_eq!(second_resource, first_resource + 1);
        assert!(second_row.render_geometry.is_none());
    }

    #[test]
    fn sequential_render_additions_preserve_the_installed_prefix() {
        let mut installed = empty_bundle().install().unwrap();

        let mut first_addition = empty_bundle();
        first_addition.set_render_geometries(
            92,
            vec![retained_path(0.0)].into(),
            vec![RenderGeometryPreparation {
                resource: 0,
                style: Style::default(),
                transform: Transform2D::IDENTITY,
            }],
        );
        let first_prepared = installed
            .prepare_additions_with_render(first_addition)
            .unwrap();
        assert_eq!(first_prepared.render_geometry_suffix().len(), 1);
        installed.commit_additions_with_render(first_prepared);
        let first_table = installed.render_geometries();
        assert_eq!(first_table.len(), 1);

        let mut second_addition = empty_bundle();
        second_addition.set_render_geometries(
            92,
            vec![retained_path(2.0)].into(),
            vec![RenderGeometryPreparation {
                resource: 0,
                style: Style::default(),
                transform: Transform2D::IDENTITY,
            }],
        );
        let second_prepared = installed
            .prepare_additions_with_render(second_addition)
            .unwrap();
        assert_eq!(second_prepared.render_geometry_suffix().len(), 1);
        let combined = second_prepared.render_geometries().unwrap();
        assert_eq!(combined.len(), 2);
        assert!(Arc::ptr_eq(&first_table[0], &combined[0]));

        installed.commit_additions_with_render(second_prepared);
        let second_table = installed.render_geometries();
        assert_eq!(second_table.len(), 2);
        assert!(Arc::ptr_eq(&first_table[0], &second_table[0]));
        assert_eq!(installed.render_geometry_preparations().len(), 2);
        assert_eq!(installed.render_geometry_preparations()[0].resource, 0);
        assert_eq!(installed.render_geometry_preparations()[1].resource, 1);
    }
}
