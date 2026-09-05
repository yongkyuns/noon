#![cfg(test)]

use super::compiled_render_geometries;
use noon_core::{GeometryRef, Vec2};

#[test]
fn compiled_table_keeps_stable_local_pairs_but_excludes_dynamic_screen_space_fallback() {
    use noon_core::{
        Easing, ObjectId, ObjectSnapshot, Property, RetainedObjectDefinition, StrokeWidthMode,
        Style, TrackDefinition, TrackId, TrackTiming, TrackValues, Transform2D,
    };
    for (mode, target_scale_x, expected_count) in [
        (StrokeWidthMode::ScreenSpace, 2.0, 1),
        (StrokeWidthMode::ScreenSpace, -1.0, 0),
        (StrokeWidthMode::ScaleWithObject, 2.0, 1),
    ] {
        let style = Style {
            stroke_width_mode: mode,
            ..Style::default()
        };
        let from = ObjectSnapshot {
            geometry: GeometryRef::circle(1.0),
            transform: Transform2D::IDENTITY,
            style,
        };
        let to = ObjectSnapshot {
            geometry: GeometryRef::rectangle(1.0, 1.0),
            transform: Transform2D {
                scale: Vec2::new(target_scale_x, 1.0),
                ..Transform2D::IDENTITY
            },
            style,
        };
        let object = RetainedObjectDefinition {
            id: ObjectId::new(0),
            content: from.geometry.clone().into(),
            transform: from.transform,
            style,
        };
        let track = TrackDefinition {
            id: TrackId::new(0),
            object: object.id,
            property: Property::Transform,
            values: TrackValues::Object { from, to },
            timing: TrackTiming::new(0.0, 1.0, Easing::Linear),
            time_map: Default::default(),
        };
        let compiled = noon_compile::RetainedCompiledScene::compile(&[object], &[track]).unwrap();
        assert_eq!(
            compiled_render_geometries(&compiled).len(),
            expected_count,
            "{mode:?}, target scale {target_scale_x}"
        );
    }
}
