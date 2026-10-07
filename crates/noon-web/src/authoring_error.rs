//! Browser-specific exception representation over shared language diagnostics.
pub use noon::integration::AuthoringFailure;
impl From<crate::ClockError> for AuthoringFailure {
    fn from(error: crate::ClockError) -> Self {
        // These are the existing presentation-clock guards used by liveEvaluate.
        // Other browser-control entrypoints are outside this slice.
        match error {
            crate::ClockError::InvalidSceneTime(_) => {
                Self::new("invalid_input", "clock.invalid_scene_time", error)
            }
            crate::ClockError::SceneTimeOutsideLoop { .. } => {
                Self::new("invalid_input", "clock.time_outside_loop", error)
            }
            other => Self::unclassified("clock.unclassified", &other),
        }
    }
}

#[cfg(any(target_arch = "wasm32", test))]
impl From<crate::canonical_authoring_scene::PlayerReturnError> for AuthoringFailure {
    fn from(error: crate::canonical_authoring_scene::PlayerReturnError) -> Self {
        use crate::canonical_authoring_scene::PlayerReturnError as E;
        let code = match error {
            E::NotLeased => "ownership.not_leased",
            E::ForeignScene => "ownership.foreign_scene",
            E::StaleLease => "ownership.stale_lease",
        };
        Self::new("ownership", code, error)
    }
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn js_error(error: impl Into<AuthoringFailure>) -> wasm_bindgen::JsValue {
    use wasm_bindgen::JsValue;
    let error = error.into();
    let object = js_sys::Error::new(&error.message);
    object.set_name("NoonError");
    for (name, value) in [
        ("noonErrorVersion", JsValue::from_f64(1.0)),
        ("category", JsValue::from_str(error.category)),
        ("code", JsValue::from_str(error.code)),
    ] {
        js_sys::Reflect::set(&object, &JsValue::from_str(name), &value)
            .expect("new Error object accepts own diagnostic properties");
    }
    if let Some(cause) = error.cause {
        js_sys::Reflect::set(&object, &JsValue::from_str("cause"), &js_error(*cause))
            .expect("new Error object accepts its cause");
    }
    object.into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use noon::{AuthoringError, ExecutionSessionPublicationError, LiveSessionError};
    use noon_core::{SemanticMutationTransactionError, SemanticNodeId, SemanticSceneOperationError};
    use std::error::Error;

    #[test]
    fn pending_admission_error_preserves_typed_membership_diagnostic() {
        let mut transaction = noon_core::SemanticMutationTransaction::new();
        let token = transaction.create_node(noon_core::SemanticNodeCreation::family());
        let error = SemanticSceneOperationError::InvalidPendingAdmission(token);
        let message = error.to_string();
        let failure = AuthoringFailure::from(error);
        assert_eq!(failure.category, "invalid_input");
        assert_eq!(failure.code, "membership.invalid_pending_admission");
        assert_eq!(failure.message, message);
        assert!(failure.cause.is_none());
    }

    #[test]
    fn typed_membership_chain_preserves_categories_and_sources() {
        let node = SemanticNodeId::new(7, 3);
        let failure = AuthoringFailure::from(LiveSessionError::Authoring(
            AuthoringError::Semantic(SemanticSceneOperationError::MissingMembershipTarget(node)),
        ));
        assert_eq!(failure.category, "invalid_input");
        assert_eq!(failure.code, "live.authoring");
        let authoring = failure.cause.as_ref().unwrap();
        assert_eq!(authoring.code, "authoring.semantic");
        assert_eq!(
            authoring.cause.as_ref().unwrap().code,
            "membership.missing_target"
        );
        assert!(failure.source().is_some());
        assert!(!failure.message.is_empty());
    }

    #[test]
    fn typed_family_paint_rejections_keep_their_concrete_cause() {
        for (cause, code) in [
            (
                AuthoringError::InvalidOpacity {
                    name: "opacity".into(),
                    value: 1.5,
                },
                "authoring.invalid_opacity",
            ),
            (
                AuthoringError::InvalidRenderNumber {
                    name: "stroke width".into(),
                    value: f64::INFINITY,
                },
                "authoring.invalid_render_number",
            ),
            (
                AuthoringError::NegativeStrokeWidth(-1.0),
                "authoring.negative_stroke_width",
            ),
        ] {
            let diagnostic = cause.to_string();
            let failure =
                AuthoringFailure::from(noon::FamilyCallbackPaintError::InvalidPaint(cause));
            assert_eq!(failure.category, "invalid_input");
            assert_eq!(failure.code, "callback.family.invalid_paint");
            assert_eq!(failure.message, diagnostic);
            let nested = failure.cause.as_deref().unwrap();
            assert_eq!(nested.category, "invalid_input");
            assert_eq!(nested.code, code);
            assert_eq!(nested.message, diagnostic);
            assert!(nested.cause.is_none());
            assert!(std::ptr::eq(
                nested,
                failure
                    .source()
                    .unwrap()
                    .downcast_ref::<AuthoringFailure>()
                    .unwrap()
            ));
        }
        // A newly accepted typed input keeps its own category/code even when
        // the user-supplied value names unrelated error categories.
        let failure = AuthoringFailure::from(AuthoringError::InvalidStrokeCap(
            "invalid opacity; negative stroke width".into(),
        ));
        assert_eq!(failure.category, "invalid_input");
        assert_eq!(failure.code, "authoring.invalid_stroke_cap");
    }

    #[test]
    fn diagnostics_cannot_select_categories() {
        for text in [
            "unsupported",
            "foreign store",
            "stale handle",
            "pending callback",
        ] {
            assert_eq!(AuthoringFailure::from(text).category, "unclassified");
        }
        let node = SemanticNodeId::new(7, 3);
        assert_eq!(
            AuthoringFailure::from(AuthoringError::ForeignStore).category,
            "foreign_handle"
        );
        assert_eq!(
            AuthoringFailure::from(SemanticSceneOperationError::UnknownNode(node)).category,
            "stale_handle"
        );
        assert_eq!(
            AuthoringFailure::from(SemanticSceneOperationError::AmbiguousCrossRootAlias(node))
                .category,
            "unsupported_operation"
        );
        let graph = AuthoringFailure::from(
            SemanticSceneOperationError::GraphMutationRequiresTransaction(node),
        );
        assert_eq!(graph.category, "unsupported_operation");
        assert_eq!(graph.code, "graph.requires_transaction");
        assert_eq!(
            AuthoringFailure::from(ExecutionSessionPublicationError::RequiredCallbackPending)
                .category,
            "pending_work"
        );
    }
    fn assert_authoring_projection(error: AuthoringError, category: &str, codes: &[&str]) {
        let diagnostic = error.to_string();
        let failure = AuthoringFailure::from(error);
        assert_eq!(failure.category, category);
        assert_eq!(failure.message, diagnostic);
        assert!(!diagnostic.is_empty());
        let mut projected = Vec::new();
        let mut current = Some(&failure);
        while let Some(cause) = current {
            projected.push(cause.code);
            assert_eq!(cause.category, category);
            assert!(!cause.message.is_empty());
            current = cause.cause.as_deref();
        }
        assert_eq!(projected, codes);
    }

    #[test]
    fn accepted_public_operations_project_real_rust_causes_and_retry() {
        use noon::{LayoutAnchor, MobjectTarget, Scene};
        let mut scene = Scene::new();
        let mut first = scene.circle(1.0).unwrap();
        let second = scene.square(1.0).unwrap();
        let family = scene.family(&[(&first).into(), (&second).into()]).unwrap();
        let before = (first.state().unwrap(), second.state().unwrap());
        let revision = scene.integration_store().borrow().scene_revision();
        let nodes = scene.integration_store().borrow().len();
        assert_authoring_projection(
            scene.circle(0.0).unwrap_err(),
            "invalid_input",
            &["authoring.non_positive_number"],
        );
        assert_authoring_projection(
            first.shift(f64::NAN, 0.0).unwrap_err(),
            "invalid_input",
            &["authoring.vector_lowering", "vector.invalid_coordinates"],
        );
        assert_authoring_projection(
            first.set_fill_opacity(1.5).unwrap_err(),
            "invalid_input",
            &["authoring.invalid_opacity"],
        );
        assert_authoring_projection(
            first
                .path_query()
                .unwrap()
                .point_from_proportion(-1.0)
                .unwrap_err(),
            "invalid_input",
            &["path.invalid_query"],
        );
        assert_authoring_projection(
            LayoutAnchor::from(&family).member(-3).layout().unwrap_err(),
            "invalid_input",
            &["authoring.invalid_submobject_index"],
        );
        assert_authoring_projection(
            family
                .arrange_in_grid(Some(1), Some(1), 0.1, 0.1)
                .unwrap_err(),
            "invalid_input",
            &["authoring.insufficient_grid_capacity"],
        );
        let foreign = Scene::new().circle(1.0).unwrap();
        assert_authoring_projection(
            family
                .copy_with_references(&[MobjectTarget::Object(&foreign)])
                .unwrap_err(),
            "foreign_handle",
            &["authoring.foreign_store"],
        );
        assert_authoring_projection(
            scene.value_tracker(f64::NAN).unwrap_err(),
            "invalid_input",
            &["authoring.signal", "signal.non_finite_value"],
        );
        assert_eq!((first.state().unwrap(), second.state().unwrap()), before);
        assert_eq!(
            scene.integration_store().borrow().scene_revision(),
            revision
        );
        assert_eq!(scene.integration_store().borrow().len(), nodes);
        first.shift(0.5, -0.5).unwrap();
        first.set_fill_opacity(0.5).unwrap();
        family.arrange_in_grid(Some(1), Some(2), 0.1, 0.1).unwrap();
        assert_eq!(first.fill_opacity().unwrap(), 0.5);
        assert!(family.copy_family().is_ok());
        let tracker = scene.value_tracker(2.0).unwrap();
        assert_eq!(scene.value_tracker_value(&tracker).unwrap(), 2.0);
    }

    #[test]
    fn accepted_live_projection_preserves_atomic_publication_and_local_retry() {
        let mut scene = noon::Scene::new();
        let target = scene.circle(1.0).unwrap();
        let unrelated = scene.square(1.0).unwrap();
        scene.add(&target).unwrap();
        scene.add(&unrelated).unwrap();
        let mut execution = scene.execution_session().unwrap();
        execution.take_frame_changes();
        let before = execution.frame().clone();
        let publication = execution.publication_context();
        let authored = target.state().unwrap();
        let error = scene
            .live(&mut execution)
            .set_fill_opacity(&target, 1.5)
            .unwrap_err();
        let message = error.to_string();
        let projected = AuthoringFailure::from(error);
        assert_eq!(projected.category, "invalid_input");
        assert_eq!(projected.code, "live.authoring");
        assert_eq!(
            projected.cause.as_ref().unwrap().code,
            "authoring.invalid_opacity"
        );
        assert_eq!(projected.message, message);
        assert_eq!(execution.frame(), &before);
        assert_eq!(execution.publication_context(), publication);
        assert_eq!(target.state().unwrap(), authored);
        assert!(execution.take_frame_changes().is_empty());
        scene
            .live(&mut execution)
            .set_fill_opacity(&target, 0.5)
            .unwrap();
        assert_eq!(execution.take_frame_changes().object_indices(), &[0]);
        assert_eq!(execution.frame().objects[1], before.objects[1]);
    }

    #[test]
    fn duplicate_effect_parameter_keeps_a_typed_language_boundary_error() {
        let effect = SemanticNodeId::new(7, 3);
        let error =
            AuthoringFailure::from(SemanticMutationTransactionError::DuplicateEffectParameter {
                index: 2,
                effect,
                parameter: noon_core::GlowParameter::Intensity,
            });
        assert_eq!(error.category, "invalid_input");
        assert_eq!(error.code, "transaction.duplicate_effect_parameter");
        assert!(error.message.contains("Intensity"));
        assert!(error.message.contains("generation: 3"));
    }

    #[test]
    fn effect_errors_project_shared_causes_without_text_matching() {
        use noon::effects::{Glow, GlowUpdate};
        let mut scene = noon::Scene::new();
        let object = scene.circle(0.08).unwrap();
        scene.set_glow(&object, GlowUpdate::default()).unwrap();
        let invalid = scene
            .set_glow(&object, GlowUpdate::default().intensity(-1.0))
            .unwrap_err();
        let projected = AuthoringFailure::from(invalid);
        assert_eq!(projected.category, "invalid_input");
        assert_eq!(
            projected.cause.as_ref().unwrap().code,
            "effect.invalid_parameter"
        );
        let duplicate = scene
            .add_effect(&object, Glow::default(), "glow")
            .unwrap_err();
        let projected = AuthoringFailure::from(duplicate);
        assert_eq!(projected.category, "invalid_input");
        assert_eq!(
            projected.cause.as_ref().unwrap().code,
            "transaction.invalid_effect_attachment"
        );
        let handle = object.get_effect("glow").unwrap();
        scene.remove_glow(&object).unwrap();
        let stale = handle.authored_definition().unwrap_err();
        assert_eq!(AuthoringFailure::from(stale).category, "stale_handle");
    }
}
