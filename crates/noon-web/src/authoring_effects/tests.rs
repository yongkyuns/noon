use super::*;

#[test]
fn mask_boundary_checks_seed_before_narrowing_and_keeps_explicit_zero() {
    let update = mask_parameters(
        EffectUpdate::default(),
        Some(0.0),
        Some(0.08),
        Some(f64::from(u32::MAX)),
    )
    .unwrap();
    let value = update
        .apply_to(noon_core::NoiseDissolve::default())
        .unwrap();
    assert_eq!(
        (value.seed(), value.progress(), value.softness()),
        (u32::MAX, 0.0, 0.08)
    );
    assert!(update.writes(noon_core::EffectParameter::Progress));
    assert!(update.apply_to(Glow::default()).is_err());
    for seed in [
        -1.0,
        0.5,
        f64::NAN,
        f64::INFINITY,
        f64::from(u32::MAX) + 1.0,
    ] {
        assert_eq!(
            mask_parameters(EffectUpdate::default(), None, None, Some(seed))
                .unwrap_err()
                .code,
            "effect.invalid_seed"
        );
    }
    for invalid in [-0.1, 1.0 + f64::EPSILON, f64::NAN, f64::INFINITY] {
        assert!(mask_parameters(EffectUpdate::default(), Some(invalid), None, None).is_err());
        assert!(mask_parameters(EffectUpdate::default(), None, Some(invalid), None).is_err());
    }
}

#[test]
fn offset_shape_units_and_validation_reach_the_shared_shadow_value() {
    let update = effect_update(
        &[0.2, 0.4, 0.6, 0.5],
        Some(4.0),
        true,
        None,
        None,
        &[-3.25, 4.5],
        true,
    )
    .unwrap();
    let shadow = noon_core::Shadow::new(update.clone()).unwrap();
    assert_eq!(
        shadow.offset(),
        noon_core::EffectOffset::Pixels([-3.25, 4.5])
    );
    assert_eq!(shadow.color(), Color::rgba(0.2, 0.4, 0.6, 0.5));
    for offset in [vec![], vec![1.0], vec![1.0, 2.0, 3.0], vec![f64::NAN, 0.0]] {
        assert!(effect_update(&[], None, false, None, None, &offset, true).is_err());
    }
    assert!(Glow::new(update.clone()).is_err());
    assert!(noon_core::Blur::new(update.clone()).is_err());
}

#[test]
fn defaults_and_empty_updates_are_resolved_by_shared_rust() {
    let update = effect_update(&[], None, false, None, None, &[], false).unwrap();
    assert_eq!(update, EffectUpdate::default());
    let value = Glow::new(update.clone()).unwrap();
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
        let error = effect_update(&color, None, false, None, None, &[], false).unwrap_err();
        assert_eq!(error.category, "invalid_input");
    }
    assert!(effect_update(&[], None, true, None, None, &[], false).is_err());
    assert!(effect_update(&[], None, false, None, Some("unknown"), &[], false).is_err());
}

#[test]
fn invalid_scalars_keep_the_shared_parameter_error() {
    for value in [-1.0, f64::NAN, f64::INFINITY] {
        assert_eq!(
            effect_update(&[], Some(value), false, None, None, &[], false)
                .unwrap_err()
                .category,
            "invalid_input"
        );
        assert_eq!(
            effect_update(&[], None, false, Some(value), None, &[], false)
                .unwrap_err()
                .category,
            "invalid_input"
        );
    }
    assert!(effect_update(&[], None, false, Some(8.0001), None, &[], false).is_err());
    assert!(effect_update(&[], None, false, Some(8.0), None, &[], false).is_ok());
}

#[test]
fn adapter_and_direct_rust_produce_identical_authored_state() {
    let update = effect_update(
        &[0.2, 0.4, 0.6, 0.5],
        Some(12.0),
        true,
        Some(0.25),
        Some("silhouette"),
        &[],
        false,
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
    let patch = effect_update(&[], None, false, Some(1.4), None, &[], false).unwrap();
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
            effect_update(&[], None, false, Some(0.25), None, &[], false).unwrap(),
        )
        .unwrap();
    let mut target = dot.target_editor().unwrap();
    target.shift(2.0, 0.0).unwrap();
    target
        .set_glow(effect_update(&[], None, false, Some(1.4), None, &[], false).unwrap())
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
