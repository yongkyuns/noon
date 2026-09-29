use super::*;
use noon_compile::{CompiledObject, CompiledScene};
use noon_core::{Color, GeometryRef, Vec2};

fn animated_runtime(property: Property, start: f64, duration: f64) -> SceneInstance {
    let object = ObjectId::new(1);
    let values = match property {
        Property::Rotation => noon_core::TrackValues::Scalar { from: 0.0, to: 2.0 },
        Property::Scale => noon_core::TrackValues::Vec2 {
            from: Vec2::new(1.0, 1.0),
            to: Vec2::new(2.0, 2.0),
        },
        _ => unreachable!(),
    };
    SceneInstance::new(
        CompiledScene::compile_objects(
            vec![CompiledObject::new(
                object,
                GeometryRef::circle(1.0),
                Transform2D::IDENTITY,
                Style {
                    fill: Some(Color::BLUE),
                    ..Style::default()
                },
            )],
            &[noon_core::TrackDefinition {
                id: noon_core::TrackId::new(1),
                object,
                property,
                values,
                timing: noon_core::TrackTiming::new(start, duration, RateFunction::Linear),
                time_map: noon_core::CompositionTimeMap::default(),
            }],
        )
        .unwrap(),
    )
}

fn start_effect(runtime: &mut SceneInstance) {
    let effect = runtime
        .prepare_click_indicate(
            ObjectId::new(1),
            SemanticClickIndicate::new(1.2, Color::YELLOW, 2.0),
        )
        .unwrap()
        .unwrap();
    runtime.start_transient_animation(effect).unwrap();
    runtime.advance_interactions(0.0).unwrap();
}

#[test]
fn indicate_coexists_with_disjoint_authored_rotation() {
    let mut runtime = animated_runtime(Property::Rotation, 0.0, 2.0);
    start_effect(&mut runtime);
    runtime.advance_to(1.0).unwrap();
    runtime.advance_interactions(1.0).unwrap();
    let row = runtime.effective_object(ObjectId::new(1)).unwrap();
    assert_eq!(row.transform.rotation, 1.0);
    assert_eq!(row.transform.scale, Vec2::new(1.2, 1.2));
    assert_eq!(row.style.fill, Some(Color::YELLOW));
    assert!(runtime.interactions_active());
    runtime.advance_to(2.0).unwrap();
    runtime.advance_interactions(2.0).unwrap();
    let row = runtime.effective_object(ObjectId::new(1)).unwrap();
    assert_eq!(row.transform.rotation, 2.0);
    assert_eq!(row.transform.scale, Vec2::new(1.0, 1.0));
    assert_eq!(row.style.fill, Some(Color::BLUE));
    assert!(!runtime.interactions_active());
}

#[test]
fn conflicting_timeline_defers_acquisition_and_supersedes_an_active_effect() {
    let mut active = animated_runtime(Property::Scale, 0.0, 2.0);
    active.advance_to(0.5).unwrap();
    assert!(active
        .prepare_click_indicate(
            ObjectId::new(1),
            SemanticClickIndicate::new(1.2, Color::YELLOW, 2.0)
        )
        .unwrap()
        .is_none());
    // Cover both active and fully crossed intervals, including an instant event.
    for (duration, time) in [(1.0, 1.0), (1.0, 2.0), (0.0, 2.0)] {
        let mut runtime = animated_runtime(Property::Scale, 0.5, duration);
        start_effect(&mut runtime);
        runtime.advance_interactions(0.25).unwrap();
        runtime.advance_to(time).unwrap();
        runtime.advance_interactions(0.5).unwrap();
        assert!(!runtime.interactions_active());
        let mut fresh = animated_runtime(Property::Scale, 0.5, duration);
        fresh.advance_to(time).unwrap();
        assert_eq!(runtime.frame(), fresh.frame());
    }
}

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
