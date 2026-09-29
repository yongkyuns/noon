use super::*;
use noon_compile::{CompiledObject, CompiledScene};
use noon_core::{Color, GeometryRef, Vec2};

#[test]
fn indicate_ticks_and_release_preserve_unowned_effective_channels() {
    let object = ObjectId::new(1);
    let baseline = Style {
        fill: Some(Color::BLUE),
        stroke: Some(Color::WHITE),
        ..Style::default()
    };
    let compiled = CompiledScene::compile_objects(
        vec![CompiledObject::new(
            object,
            GeometryRef::circle(1.0),
            Transform2D::IDENTITY,
            baseline,
        )],
        &[],
    )
    .unwrap();
    let mut runtime = SceneInstance::new(compiled);
    let effect = runtime
        .prepare_click_indicate(object, SemanticClickIndicate::new(1.2, Color::YELLOW, 1.0))
        .unwrap()
        .unwrap();
    runtime.start_transient_animation(effect).unwrap();
    runtime.advance_interactions(0.0).unwrap();
    for (tick, angle, opacity) in [(0.5, 0.4, 0.3), (1.0, 0.9, 0.7)] {
        let independent = runtime
            .prepare_effective_property_batch(&[
                EffectivePropertyWrite::Translation {
                    object,
                    translation: Vec2::new(8.0, 4.0),
                },
                EffectivePropertyWrite::Rotation {
                    object,
                    rotation: angle,
                },
                EffectivePropertyWrite::Opacity { object, opacity },
                EffectivePropertyWrite::StrokeWidth {
                    object,
                    stroke_width: 7.0,
                },
            ])
            .unwrap();
        runtime
            .commit_transient_effective_properties(independent)
            .unwrap();
        runtime.advance_interactions(tick).unwrap();
        let row = runtime.effective_object(object).unwrap();
        assert_eq!(row.transform.translation, Vec2::new(8.0, 4.0));
        assert_eq!(row.transform.rotation, angle);
        assert_eq!(row.style.opacity, opacity);
        assert_eq!(row.style.stroke_width, 7.0);
        if tick == 0.5 {
            assert_eq!(row.transform.scale, Vec2::new(1.2, 1.2));
            assert_eq!(row.style.fill, Some(Color::YELLOW));
        } else {
            assert_eq!(row.transform.scale, Vec2::new(1.0, 1.0));
            assert_eq!(row.style.fill, baseline.fill);
            assert_eq!(row.style.stroke, baseline.stroke);
        }
    }
    assert!(!runtime.interactions_active());
    assert_eq!(runtime.frame().time, 0.0);
    let settled = runtime.publication_context();
    runtime.advance_interactions(2.0).unwrap();
    assert_eq!(runtime.publication_context(), settled);
}
