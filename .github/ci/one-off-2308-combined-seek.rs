
#[test]
fn merged_seek_preserves_paint_effect_and_reserved_storage() {
    use noon_compile::CompiledEffect;
    use noon_core::{EffectDefinition, Glow, GradientStop, LinearGradient, SemanticNodeId, SemanticPaint};

    let paint = Arc::new(SemanticPaint::LinearGradient(Arc::new(
        LinearGradient::new(
            [0.0, 0.5],
            [1.0, 0.5],
            [
                GradientStop::new(0.0, Color::RED).unwrap(),
                GradientStop::new(1.0, Color::BLUE).unwrap(),
            ],
        )
        .unwrap(),
    )));
    let effect = Arc::new(CompiledEffect {
        attachment: SemanticNodeId::new(7, 1),
        definition: EffectDefinition::Glow(Glow::default()),
    });
    let mut row = object(0);
    row.fill_paint = Some(paint.clone());
    row.effect = Some(effect.clone());
    let compiled = CompiledScene::compile_objects(vec![row], &[]).unwrap();
    let mut runtime = SceneInstance::new(compiled.clone());
    reserve(&mut runtime, 12);
    let allocated = storage(&runtime);
    for time in [2.0, 0.0, 0.5, 1.0, 0.0] {
        runtime.seek(time).unwrap();
        assert!(
            runtime.frame().objects[0]
                .fill_paint
                .as_ref()
                .is_some_and(|value| Arc::ptr_eq(value, &paint)),
            "seek lost the retained paint handle"
        );
        assert!(
            runtime.frame().objects[0]
                .effect
                .as_ref()
                .is_some_and(|value| Arc::ptr_eq(value, &effect)),
            "seek lost the retained effect handle"
        );
        assert_eq!(runtime.frame(), &reference_base_frame(&compiled, time));
        assert_eq!(storage(&runtime), allocated);
    }
}
