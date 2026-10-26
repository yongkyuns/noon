use super::*;
use crate::{lower_semantic_execution_root, SemanticExecutionIndex};
use noon_core::{Color, GradientStop};

fn paint(start: [f64; 2], end: [f64; 2]) -> Arc<LinearGradient> {
    Arc::new(
        LinearGradient::new(
            start,
            end,
            [
                GradientStop::new(0.0, Color::rgba(1.0, 0.0, 0.0, 0.0)).unwrap(),
                GradientStop::new(1.0, Color::rgb(0.0, 0.0, 1.0)).unwrap(),
            ],
        )
        .unwrap(),
    )
}

#[test]
fn lowering_preserves_continuous_definition_and_separate_opacity_domains() {
    let mut store = SemanticStore::new();
    let root = store.insert_family();
    let gradient = paint([0.0, 0.5], [1.0, 0.5]);
    let mut state = SemanticObjectState::new(StoredGeometry::Circle { radius: 1.0 });
    state.style.fill = Some(SemanticPaint::LinearGradient(gradient.clone()));
    state.style.fill_opacity = 0.25;
    state.style.object_opacity = 0.5;
    let object = store.insert_semantic_object(state);
    store.add_semantic_family_member(root, object).unwrap();
    let output =
        lower_semantic_execution_root(&store, root, &mut SemanticExecutionIndex::new()).unwrap();
    let object = &output.compiled().objects()[0];
    assert!(Arc::ptr_eq(
        object.fill_gradient.as_ref().unwrap(),
        &gradient
    ));
    assert_eq!(
        object.base_style.fill,
        Some(Color::rgba(1.0, 1.0, 1.0, 0.25))
    );
    assert_eq!(object.base_style.opacity, 0.5);
}

#[test]
fn gradient_profile_rejects_collapsed_gpu_axes_and_stop_intervals_before_identity_publication() {
    let mut store = SemanticStore::new();
    let root = store.insert_family();
    for gradient in [
        paint([0.0, 0.0], [1e-60, 0.0]),
        Arc::new(
            LinearGradient::new(
                [0.0, 0.0],
                [1.0, 0.0],
                [
                    GradientStop::new(0.5, Color::WHITE).unwrap(),
                    GradientStop::new(0.5 + f64::EPSILON, Color::BLACK).unwrap(),
                ],
            )
            .unwrap(),
        ),
    ] {
        let mut state = SemanticObjectState::new(StoredGeometry::Circle { radius: 1.0 });
        state.style.fill = Some(SemanticPaint::LinearGradient(gradient));
        let object = store.insert_semantic_object(state);
        store.add_semantic_family_member(root, object).unwrap();
        let mut index = SemanticExecutionIndex::new();
        assert!(lower_semantic_execution_root(&store, root, &mut index).is_err());
        assert!(index.is_empty());
        store.remove_semantic_family_member(root, object).unwrap();
    }
}
