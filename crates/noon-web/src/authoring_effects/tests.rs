use super::*;

#[test]
fn defaults_and_empty_updates_are_resolved_by_shared_rust() {
    let update = effect_update(&[], None, false, None, None).unwrap();
    assert_eq!(update, EffectUpdate::default());
    let value = Glow::new(update).unwrap();
    assert_eq!(value, Glow::default());
    let custom = Glow::new(EffectUpdate::default().intensity(1.4).radius(0.3)).unwrap();
    assert_eq!(update.apply_to(custom).unwrap(), custom);
}

#[test]
fn boundary_shape_and_color_narrowing_fail_closed() {
    for color in [
        vec![1.0; 3],
        vec![1.0; 5],
        vec![f64::NAN; 4],
        vec![1.0 + f64::EPSILON, 0.0, 0.0, 1.0],
    ] {
        let error = effect_update(&color, None, false, None, None).unwrap_err();
        assert_eq!(error.category, "invalid_input");
    }
    assert!(effect_update(&[], None, true, None, None).is_err());
    assert!(effect_update(&[], None, false, None, Some("unknown")).is_err());
}

#[test]
fn invalid_scalars_keep_the_shared_parameter_error() {
    for value in [-1.0, f64::NAN, f64::INFINITY] {
        assert_eq!(
            effect_update(&[], Some(value), false, None, None)
                .unwrap_err()
                .category,
            "invalid_input"
        );
        assert_eq!(
            effect_update(&[], None, false, Some(value), None)
                .unwrap_err()
                .category,
            "invalid_input"
        );
    }
    assert!(effect_update(&[], None, false, Some(8.0001), None).is_err());
    assert!(effect_update(&[], None, false, Some(8.0), None).is_ok());
}

#[test]
fn adapter_and_direct_rust_produce_identical_authored_state() {
    let update = effect_update(
        &[0.2, 0.4, 0.6, 0.5],
        Some(12.0),
        true,
        Some(0.25),
        Some("silhouette"),
    )
    .unwrap();
    let direct = EffectUpdate::default()
        .color(Color::rgba(0.2, 0.4, 0.6, 0.5))
        .radius(noon::effects::Pixels(12.0))
        .intensity(0.25)
        .source(GlowSource::Silhouette);
    assert_eq!(update, direct);
    let mut scene = noon::Scene::new();
    let dot = scene.circle(0.08).unwrap();
    scene.set_glow(&dot, update).unwrap();
    let handle = scene.get_effect(&dot, "glow").unwrap();
    let value = (handle.authored_definition().unwrap()).as_glow().unwrap();
    assert_eq!(value, Glow::new(direct).unwrap());
    // The adapter emits only the explicit update, not defaults/old snapshots.
    let patch = effect_update(&[], None, false, Some(1.4), None).unwrap();
    scene.set_effect(&dot, &handle, patch).unwrap();
    let changed = (handle.authored_definition().unwrap()).as_glow().unwrap();
    assert_eq!(changed.radius(), value.radius());
    assert_eq!(changed.color(), value.color());
    assert_eq!(changed.source(), value.source());
    assert_eq!(changed.intensity(), 1.4);
}

#[test]
fn parsed_target_updates_share_copy_isolation_and_ordinary_declaration() {
    let mut scene = noon::Scene::new();
    let dot = scene.circle(0.08).unwrap();
    scene
        .set_glow(
            &dot,
            effect_update(&[], None, false, Some(0.25), None).unwrap(),
        )
        .unwrap();
    let mut target = dot.target_editor().unwrap();
    target.shift(2.0, 0.0).unwrap();
    target
        .set_glow(effect_update(&[], None, false, Some(1.4), None).unwrap())
        .unwrap();
    let original = (dot
        .get_effect("glow")
        .unwrap()
        .authored_definition()
        .unwrap())
    .as_glow()
    .unwrap();
    let changed = (target
        .get_effect("glow")
        .unwrap()
        .authored_definition()
        .unwrap())
    .as_glow()
    .unwrap();
    assert_eq!(original.intensity(), 0.25);
    assert_eq!(changed.intensity(), 1.4);
    let animation = scene
        .declare_transform_to(&dot, &target, noon::AnimationOptions::new().run_time(1.5))
        .unwrap();
    assert_eq!(animation.options().unwrap().run_time, Some(1.5));
    assert!(scene
        .execution_session()
        .unwrap()
        .frame()
        .objects
        .is_empty());
}
