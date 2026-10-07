//! First-glow API review witnesses using the actual shared Rust owners.
//! These qualify authored declarations/options, not enabled effect playback.
use super::*;
use noon_core::{
    resolve_animation_options, AnimationDefaults, AnimationOptionsError, RateFunction,
    SemanticAnimationCompositionKind, SemanticAnimationIntent,
};

#[test]
fn motion_and_effect_targets_remain_ordinary_nested_animation_declarations() {
    let mut scene = Scene::new();
    let dot = scene.circle(0.08).unwrap();
    let title = scene.rectangle(2.0, 0.3).unwrap();
    scene.set_glow(&dot, GlowUpdate::default()).unwrap();
    scene.add_effect(&title, Glow::default(), "accent").unwrap();
    let mut dot_target = dot.target_editor().unwrap();
    dot_target.shift(2.0, 0.0).unwrap();
    dot_target
        .set_glow(GlowUpdate::default().intensity(1.2))
        .unwrap();
    let mut title_target = title.target_editor().unwrap();
    title_target.shift(0.0, 1.0).unwrap();
    title_target
        .set_effect("accent", GlowUpdate::default().intensity(1.4))
        .unwrap();
    let a = scene
        .declare_transform_to(
            &dot,
            &dot_target,
            AnimationOptions::new()
                .run_time(1.5)
                .rate_func(RateFunction::Linear),
        )
        .unwrap();
    let b = scene
        .declare_transform_to(
            &title,
            &title_target,
            AnimationOptions::new().run_time(0.75),
        )
        .unwrap();
    let parallel = scene
        .declare_animation(
            SemanticAnimationIntent::Composition {
                kind: SemanticAnimationCompositionKind::Parallel,
                children: vec![a.node_id(), b.node_id()],
            },
            AnimationOptions::new().lag_ratio(0.25).run_time(3.0),
        )
        .unwrap();
    let wait = scene
        .declare_animation(
            SemanticAnimationIntent::Wait,
            AnimationOptions::new().run_time(0.5),
        )
        .unwrap();
    let sequence = scene
        .declare_animation(
            SemanticAnimationIntent::Composition {
                kind: SemanticAnimationCompositionKind::Sequence,
                children: vec![parallel.node_id(), wait.node_id()],
            },
            AnimationOptions::new(),
        )
        .unwrap();
    let store = scene.integration_store().borrow();
    assert_eq!(
        store
            .semantic_animation_state(parallel.node_id())
            .unwrap()
            .intent()
            .children(),
        &[a.node_id(), b.node_id()]
    );
    assert_eq!(
        store
            .semantic_animation_state(sequence.node_id())
            .unwrap()
            .intent()
            .children(),
        &[parallel.node_id(), wait.node_id()]
    );
    drop(store);
    assert_eq!(parallel.options().unwrap().run_time, Some(3.0));
    assert_eq!(a.options().unwrap().run_time, Some(1.5));
    assert_eq!(dot.state().unwrap().transform.translation.x, 0.0);
    assert_eq!(value(&dot.get_effect("glow").unwrap()).intensity(), 0.35);
    assert_eq!(
        value(&title.get_effect("accent").unwrap()).intensity(),
        0.35
    );
    assert_eq!(
        value(&dot_target.get_effect("glow").unwrap()).intensity(),
        1.2
    );
    // Creating a composition must not secretly start an effect-only executor.
    assert!(scene.execution_session().is_err());
}

#[test]
fn zero_duration_effect_target_declaration_uses_the_ordinary_timing_error() {
    let mut scene = Scene::new();
    let dot = scene.circle(0.08).unwrap();
    scene.set_glow(&dot, GlowUpdate::default()).unwrap();
    let mut target = dot.target_editor().unwrap();
    target
        .set_glow(GlowUpdate::default().intensity(1.2))
        .unwrap();
    let before = scene.revision();
    let count = scene.integration_store().borrow().len();
    assert!(scene
        .declare_transform_to(&dot, &target, AnimationOptions::new().run_time(0.0))
        .is_err());
    assert_eq!(scene.revision(), before);
    assert_eq!(scene.integration_store().borrow().len(), count);
    assert_eq!(value(&dot.get_effect("glow").unwrap()).intensity(), 0.35);
}

#[test]
fn pulse_contract_uses_shared_defaults_local_options_and_play_overrides() {
    let defaults = AnimationDefaults {
        rate_func: RateFunction::ThereAndBack,
        ..AnimationDefaults::MANIM
    };
    let local = AnimationOptions::new().run_time(0.6);
    let resolved = resolve_animation_options(defaults, local, AnimationOptions::new()).unwrap();
    assert_eq!(resolved.run_time, 0.6);
    assert_eq!(resolved.rate_func, RateFunction::ThereAndBack);
    let overridden = resolve_animation_options(
        defaults,
        local,
        AnimationOptions::new()
            .run_time(1.2)
            .rate_func(RateFunction::Linear),
    )
    .unwrap();
    assert_eq!(overridden.run_time, 1.2);
    assert_eq!(overridden.rate_func, RateFunction::Linear);
    let prepared = GlowUpdate::default()
        .intensity(2.0)
        .prepare(Glow::default())
        .unwrap();
    assert_eq!(
        prepared
            .sample(resolved.rate_func.evaluate_f64(1.0))
            .unwrap()
            .intensity,
        Some(0.35)
    );
    assert_eq!(
        prepared
            .sample(overridden.rate_func.evaluate_f64(1.0))
            .unwrap()
            .intensity,
        Some(2.0)
    );
    // A returning rate is not the lifecycle restore: a linear override proves
    // that runtime must independently release/restore still-owned channels.
    for duration in [0.0, -1.0, f64::INFINITY, f64::NAN] {
        assert!(matches!(
            resolve_animation_options(
                defaults,
                AnimationOptions::new().run_time(duration),
                AnimationOptions::new()
            ),
            Err(AnimationOptionsError::InvalidRunTime(_))
        ));
    }
}

#[test]
fn names_resolve_on_the_exact_receiver_not_a_related_source_or_target() {
    let mut scene = Scene::new();
    let source = scene.circle(0.08).unwrap();
    scene
        .add_effect(&source, Glow::default(), "accent")
        .unwrap();
    let original = source.get_effect("accent").unwrap();
    let mut target = source.target_editor().unwrap();
    let revision = scene.revision();
    assert!(matches!(
        target.set_effect(&original, GlowUpdate::default().intensity(1.4)),
        Err(AuthoringError::EffectOwnerMismatch { .. })
    ));
    assert_eq!(scene.revision(), revision);
    target
        .set_effect("accent", GlowUpdate::default().intensity(1.4))
        .unwrap();
    assert_eq!(value(&original).intensity(), 0.35);
    assert_eq!(
        value(&target.get_effect("accent").unwrap()).intensity(),
        1.4
    );
}

#[cfg(all(feature = "native-text", feature = "bundled-fonts"))]
#[test]
fn mixed_text_path_image_and_nested_alias_copy_preserve_leaf_effect_meaning() {
    use crate::{SemanticStyle, Text, VectorPath};
    let mut scene = Scene::new();
    let title = scene
        .text(
            Text::new("Signal")
                .with_font("DejaVu Sans Mono")
                .with_font_size(48.0),
        )
        .unwrap();
    let path = scene
        .path(
            VectorPath::new()
                .move_to(Vec2::new(-2.0, -1.0))
                .line_to(Vec2::new(0.0, 1.0))
                .line_to(Vec2::new(2.0, -1.0)),
            SemanticStyle::default(),
        )
        .unwrap();
    let image = scene
        .image_rgba8(
            2,
            2,
            vec![
                255, 0, 0, 255, 0, 0, 255, 0, 0, 255, 0, 128, 255, 255, 255, 255,
            ],
        )
        .unwrap();
    let dot = scene.circle(0.08).unwrap();
    let leaves = [&title, &path, &image, &dot];
    for leaf in leaves {
        scene
            .add_effect(
                leaf,
                Glow::new(GlowUpdate::default().radius(Pixels(12.0))).unwrap(),
                "accent",
            )
            .unwrap();
    }
    let inner = scene.family(&[(&path).into(), (&image).into()]).unwrap();
    let outer = scene
        .family(&[
            (&title).into(),
            (&inner).into(),
            (&dot).into(),
            (&path).into(),
        ])
        .unwrap();
    scene.add_many(&[(&outer).into()]).unwrap();
    let before_members = scene
        .integration_store()
        .borrow()
        .node(outer.node_id())
        .unwrap()
        .members();
    let copy = outer.copy_family().unwrap();
    for leaf in leaves {
        let mut copied = copy.mobject(leaf).unwrap();
        let a = leaf.get_effect("accent").unwrap();
        let b = copied.get_effect("accent").unwrap();
        assert_ne!(a.node_id(), b.node_id());
        assert_eq!(value(&a), value(&b));
        copied
            .set_effect("accent", GlowUpdate::default().intensity(1.4))
            .unwrap();
        assert_eq!(value(&a).intensity(), 0.35);
        assert_eq!(value(&b).intensity(), 1.4);
    }
    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .node(outer.node_id())
            .unwrap()
            .members(),
        before_members
    );
    // No guessed family-wide handle: these are explicit per-leaf declarations.
    assert!(scene
        .integration_store()
        .borrow()
        .node(outer.node_id())
        .unwrap()
        .effect_ids()
        .is_empty());
    assert!(scene.execution_session().is_err());
}
