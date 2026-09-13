#[cfg(test)]
mod tests {
    use crate::{RetainedFamilyExecutionDeltaEnvelope, RetainedResourceBundle, SemanticExecutionPlayer};
    use noon::{AnimationCompositionRequest, TransformToRequest};
    use noon_core::{AnimationOptions, RateFunction};

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
}
