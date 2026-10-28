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

    let mut property = SemanticMutationTransaction::new();
    property.set_property(
        source.node_id(),
        noon_core::SemanticObjectProperty::Emission,
        7.000000000000001,
    );
    scene.live(&mut session).apply(property).unwrap();
    assert_eq!(
        session.frame().objects[index].style.emission.strength(),
        7.000000000000001
    );
    assert_eq!(source.emission().unwrap(), 7.000000000000001);
    let before = session.frame().clone();
    let authored = source.state().unwrap();
    let other_authored = other.state().unwrap();
    for invalid in [-0.1, 64.0, f64::NAN] {
        let mut tx = SemanticMutationTransaction::new();
        tx.set_property(
            other.node_id(),
            noon_core::SemanticObjectProperty::ObjectOpacity,
            0.2,
        );
        tx.set_property(
            source.node_id(),
            noon_core::SemanticObjectProperty::Emission,
            invalid,
        );
        assert!(scene.live(&mut session).apply(tx).is_err());
        assert_eq!(session.frame(), &before);
        assert_eq!(source.state().unwrap(), authored);
        assert_eq!(other.state().unwrap(), other_authored);
    }
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

#[test]
fn ordinary_material_targets_keep_exact_endpoints_seek_and_parallel_opacity() {
    use noon::{AnimationOptions, RateFunction};
    let mut scene = Scene::new();
    let mut source = scene.circle(0.5).unwrap();
    let other = scene.rectangle(1.0, 1.0).unwrap();
    let start = 0.30000000000000004;
    let end = 63.0;
    source.set_emission(start).unwrap();
    scene.add(&source).unwrap();
    scene.add(&other).unwrap();
    let mut session = scene.execution_session().unwrap();
    let source_id = session.execution_object_id(source.node_id()).unwrap();
    let other_id = session.execution_object_id(other.node_id()).unwrap();
    let index = |id| {
        session
            .frame()
            .objects
            .iter()
            .position(|row| row.id == id)
            .unwrap()
    };
    let source_index = index(source_id);
    let other_index = index(other_id);
    let unrelated = session.frame().objects[other_index].clone();
    let wait = scene.live(&mut session).wait_segment(0.5).unwrap();
    session.seek(0.5).unwrap();
    scene.live(&mut session).complete_segment(wait).unwrap();
    let segment = {
        let mut live = scene.live(&mut session);
        let target = live.target_editor(&source).unwrap();
        live.set_emission(&target, end).unwrap();
        live.set_object_opacity(&target, 0.5).unwrap();
        live.declare_and_activate_transform_to(
            &source,
            &target,
            AnimationOptions::new()
                .run_time(2.0)
                .rate_func(RateFunction::Linear),
        )
        .unwrap()
    };
    for alpha in [0.0, 0.5, 0.12345678912345, 0.9, 0.25, 1.0, 0.0, 1.0] {
        session.seek(segment.start_time() + 2.0 * alpha).unwrap();
        let row = &session.frame().objects[source_index];
        let expected = start * (1.0 - alpha) + end * alpha;
        assert!(
            (row.style.emission.strength() - expected).abs() < 1e-12,
            "material at {alpha}: {} != {expected}",
            row.style.emission.strength()
        );
        assert!((f64::from(row.style.opacity) - (1.0 - 0.5 * alpha)).abs() < 1e-7);
        assert_eq!(session.frame().objects[other_index], unrelated);
    }
    scene.live(&mut session).complete_segment(segment).unwrap();
    assert_eq!(source.emission().unwrap(), end);
    assert_eq!(source.state().unwrap().style.object_opacity, 0.5);
    session.seek(0.25).unwrap();
    assert_eq!(
        session.frame().objects[source_index]
            .style
            .emission
            .strength(),
        start
    );
    session.seek(2.5).unwrap();
    assert_eq!(
        session.frame().objects[source_index]
            .style
            .emission
            .strength(),
        end
    );

    let pulse = {
        let mut live = scene.live(&mut session);
        let target = live.target_editor(&source).unwrap();
        live.set_emission(&target, 0.0).unwrap();
        live.declare_and_activate_transform_to(
            &source,
            &target,
            AnimationOptions::new()
                .run_time(1.0)
                .rate_func(RateFunction::ThereAndBack),
        )
        .unwrap()
    };
    session.seek(pulse.start_time() + 0.5).unwrap();
    assert_eq!(
        session.frame().objects[source_index]
            .style
            .emission
            .strength(),
        0.0
    );
    session.seek(pulse.start_time() + 1.0).unwrap();
    scene.live(&mut session).complete_segment(pulse).unwrap();
    assert_eq!(source.emission().unwrap(), end);
    assert_eq!(
        session.frame().objects[source_index]
            .style
            .emission
            .strength(),
        end
    );
}
