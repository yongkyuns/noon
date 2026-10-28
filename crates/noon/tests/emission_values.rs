//! Ordinary typed authoring and sparse publication, without a language host.
use noon::{Emission, Scene};
use noon_core::SemanticMutationTransaction;

#[test]
fn checked_emission_preserves_precision_copy_and_target_state() {
    let mut scene = Scene::new();
    let mut source = scene.circle(0.5).unwrap();
    let strength = 3.000000000000001;
    source.set_emission(strength).unwrap();
    let before = source.state().unwrap();
    let copied = source.copy_handle().unwrap();
    let mut target = source.target_editor().unwrap();
    target.set_emission(7.0).unwrap();
    assert_eq!(source.emission().unwrap(), strength);
    assert_eq!(copied.emission().unwrap(), strength);
    assert_eq!(target.emission().unwrap(), 7.0);
    assert_eq!(source.state().unwrap().style, before.style);
    for invalid in [f64::NAN, f64::INFINITY, -0.1, 63.01] {
        assert!(source.set_emission(invalid).is_err());
        assert_eq!(source.state().unwrap(), before);
    }
    let encoded = serde_json::to_value(Emission::new(strength).unwrap()).unwrap();
    assert_eq!(
        serde_json::from_value::<Emission>(encoded)
            .unwrap()
            .strength(),
        strength
    );
    assert!(serde_json::from_str::<Emission>("64").is_err());
}

#[test]
fn live_style_material_publication_is_sparse_and_keeps_unrelated_state() {
    let mut scene = Scene::new();
    let source = scene.circle(0.5).unwrap();
    let other = scene.rectangle(1.0, 1.0).unwrap();
    scene.add(&source).unwrap();
    scene.add(&other).unwrap();
    let mut session = scene.execution_session().unwrap();
    let id = session.execution_object_id(source.node_id()).unwrap();
    let index = session
        .frame()
        .objects
        .iter()
        .position(|row| row.id == id)
        .unwrap();
    let other_id = session.execution_object_id(other.node_id()).unwrap();
    let other_index = session
        .frame()
        .objects
        .iter()
        .position(|row| row.id == other_id)
        .unwrap();
    let before_other = session.frame().objects[other_index].clone();
    drop(session.take_renderer_publication());
    let mut style = source.state().unwrap().style;
    style.emission = Emission::new(3.000000000000001).unwrap();
    let mut tx = SemanticMutationTransaction::new();
    tx.replace_style(source.node_id(), style.clone());
    scene.live(&mut session).apply(tx).unwrap();
    assert_eq!(
        session.frame().objects[index].style.emission,
        style.emission
    );
    assert_eq!(session.frame().objects[other_index], before_other);
    let published = session.take_renderer_publication();
    assert!(!published.changes().is_all());
    assert_eq!(published.changes().object_indices(), &[index]);
    drop(published);
    let mut tx = SemanticMutationTransaction::new();
    tx.replace_style(source.node_id(), style);
    scene.live(&mut session).apply(tx).unwrap();
    assert!(session
        .take_renderer_publication()
        .changes()
        .object_indices()
        .is_empty());
}

#[test]
fn removed_emissive_rows_do_not_block_a_compatibility_view() {
    let mut scene = Scene::new();
    let mut source = scene.circle(0.5).unwrap();
    source.set_emission(7.0).unwrap();
    scene.add(&source).unwrap();
    let mut session = scene.execution_session().unwrap();
    assert!(noon_render_wgpu::validate_emission_profile(
        session.frame(),
        &noon_runtime::FrameChanges::all(),
        noon_core::ViewColorProfile::Compatibility,
        true
    )
    .is_err());
    scene.live(&mut session).remove(&source).unwrap();
    assert!(noon_render_wgpu::validate_emission_profile(
        session.frame(),
        &noon_runtime::FrameChanges::all(),
        noon_core::ViewColorProfile::Compatibility,
        true
    )
    .is_ok());
}
