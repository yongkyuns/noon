use super::*;
use noon_core::Vec2;

fn circle() -> SelectionOverlayPresentation {
    SelectionOverlayPresentation {
        geometry: SelectionOverlayGeometry::Circle { radius: 1.0 },
        transform: Transform2D::IDENTITY,
        color: Color::rgba(1.0, 1.0, 0.0, 0.35),
    }
}

#[test]
fn round_trip_is_bounded_geometry_without_scene_identity_or_resources() {
    for geometry in [
        SelectionOverlayGeometry::Circle { radius: 1.5 },
        SelectionOverlayGeometry::Rectangle {
            width: 2.0,
            height: 1.0,
        },
    ] {
        let value = SelectionOverlayPresentation {
            geometry,
            color: Color::rgba(1.0, 1.0, 0.0, 0.35),
            transform: Transform2D {
                translation: Vec2::new(3.0, -2.0),
                scale: Vec2::new(-2.0, 0.5),
                rotation: 0.7,
            },
        };
        value.validate().unwrap();
        let json = serde_json::to_value(value).unwrap();
        let fields = json.as_object().unwrap();
        assert_eq!(fields.len(), 3);
        assert!(fields.contains_key("color"));
        assert!(fields.contains_key("geometry") && fields.contains_key("transform"));
        assert_eq!(
            serde_json::from_value::<SelectionOverlayPresentation>(json).unwrap(),
            value,
        );
    }
}

#[test]
fn nonanalytic_or_unknown_payloads_are_not_silently_accepted() {
    let json = serde_json::to_value(circle()).unwrap();
    for geometry in [
        serde_json::json!({"kind":"vector_path","points":[]}),
        serde_json::json!({"kind":"circle","radius":1.0,"semantic_id":7}),
        serde_json::json!({"kind":"rectangle","width":1.0}),
    ] {
        let mut bad = json.clone();
        bad["geometry"] = geometry;
        assert!(serde_json::from_value::<SelectionOverlayPresentation>(bad).is_err());
    }
    let mut bad = json;
    bad["target"] = serde_json::json!(4);
    assert!(serde_json::from_value::<SelectionOverlayPresentation>(bad).is_err());
}

#[test]
fn degenerate_and_nonfinite_geometry_or_transforms_fail_validation() {
    for v in [0.0, -1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let mut value = circle();
        value.geometry = SelectionOverlayGeometry::Circle { radius: v };
        assert!(value.validate().is_err());
        for geometry in [
            SelectionOverlayGeometry::Rectangle {
                width: v,
                height: 1.0,
            },
            SelectionOverlayGeometry::Rectangle {
                width: 1.0,
                height: v,
            },
        ] {
            value.geometry = geometry;
            assert!(value.validate().is_err());
        }
    }
    for axis in 0..5 {
        for v in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let mut value = circle();
            match axis {
                0 => value.transform.translation.x = v,
                1 => value.transform.translation.y = v,
                2 => value.transform.scale.x = v,
                3 => value.transform.scale.y = v,
                _ => value.transform.rotation = v,
            }
            assert!(value.validate().is_err());
        }
    }
    for scale in [Vec2::new(0.0, 1.0), Vec2::new(1.0, 0.0)] {
        let mut value = circle();
        value.transform.scale = scale;
        assert!(value.validate().is_err());
    }
}

#[cfg(feature = "renderer")]
#[test]
fn preparation_reuses_the_native_analytic_overlay_value() {
    let mut value = circle();
    value.transform.scale = Vec2::new(-2.0, 0.5);
    value.transform.rotation = 0.6;
    assert_eq!(
        value.prepare().unwrap(),
        noon_render_wgpu::AnalyticOverlay::new(
            &GeometryRef::Circle { radius: 1.0 },
            value.transform,
            noon_core::Color::rgba(1.0, 1.0, 0.0, 0.35),
        )
        .unwrap(),
    );
}

#[test]
fn transport_preserves_shared_style_and_rejects_invalid_color() {
    let presentation = noon::integration::PointerSelectionPresentation {
        geometry: GeometryRef::Circle { radius: 1.0 },
        transform: Transform2D::IDENTITY,
        color: Color::rgba(0.25, 0.5, 0.75, 0.4),
    };
    let wire = SelectionOverlayPresentation::from_presentation(&presentation).unwrap();
    assert_eq!(wire.color, presentation.color);
    #[cfg(feature = "renderer")]
    assert_eq!(
        wire.prepare().unwrap(),
        noon_render_wgpu::AnalyticOverlay::new(
            &presentation.geometry,
            presentation.transform,
            presentation.color
        )
        .unwrap()
    );
    for bad in [f32::NAN, f32::INFINITY, -0.1, 1.1] {
        for channel in 0..4 {
            let mut value = wire;
            match channel {
                0 => value.color.red = bad,
                1 => value.color.green = bad,
                2 => value.color.blue = bad,
                _ => value.color.alpha = bad,
            }
            assert!(value.validate().is_err());
        }
    }
}
