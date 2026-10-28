//! Explicit persisted/debug codec checks; native sampling needs no JSON.
use noon_core::{ImageAddressMode, ImageFit, ImageMapping, ImageUvTransform};

#[test]
fn checked_mapping_round_trip_preserves_f64_and_rejects_unknown_or_invalid_fields() {
    let paint = ImageMapping::new(
        ImageFit::Contain,
        [0.12345678912345678, 0.75],
        ImageAddressMode::Repeat,
        ImageUvTransform::new(
            [0.12345678912345678, -0.7],
            [-1.25, 2.0],
            0.8765432123456789,
        )
        .unwrap(),
    )
    .unwrap();
    let encoded = serde_json::to_value(paint).unwrap();
    assert_eq!(
        serde_json::from_value::<ImageMapping>(encoded.clone()).unwrap(),
        paint
    );
    for (field, value) in [
        ("alignment", serde_json::json!([-0.01, 0.5])),
        ("fit", serde_json::json!("tile")),
        ("address", serde_json::json!("mirror")),
        ("extra", serde_json::json!(true)),
    ] {
        let mut invalid = encoded.clone();
        invalid[field] = value;
        assert!(serde_json::from_value::<ImageMapping>(invalid).is_err());
    }
    for (field, value) in [
        ("scale", serde_json::json!([0.0, 1.0])),
        ("translation", serde_json::json!([null, 1.0])),
        ("rotation", serde_json::json!(null)),
        ("extra", serde_json::json!(true)),
    ] {
        let mut invalid = encoded.clone();
        invalid["transform"][field] = value;
        assert!(serde_json::from_value::<ImageMapping>(invalid).is_err());
    }
}

#[test]
fn explicit_shared_image_paint_codec_is_strict_and_has_no_pixel_payload() {
    use noon_core::{ImagePaint, RasterImageResourceArena, RasterImageSampling, SemanticPaint};
    let mut arena = RasterImageResourceArena::new();
    let handle = arena.intern_rgba8(1, 1, [17, 73, 191, 128]).unwrap();
    let paint = SemanticPaint::Image(std::sync::Arc::new(ImagePaint::new(
        handle,
        ImageMapping::default(),
        RasterImageSampling::Bicubic,
    )));
    let encoded = serde_json::to_value(&paint).unwrap();
    assert_eq!(
        serde_json::from_value::<SemanticPaint>(encoded.clone()).unwrap(),
        paint
    );
    assert!(!encoded.to_string().contains("rgba8"));
    for path in ["image", "resource", "mapping"] {
        let mut invalid = encoded.clone();
        let field = match path {
            "image" => &mut invalid["image"],
            "resource" => &mut invalid["image"]["resource"],
            _ => &mut invalid["image"]["mapping"],
        };
        field["extra"] = serde_json::json!(true);
        assert!(
            serde_json::from_value::<SemanticPaint>(invalid).is_err(),
            "unknown field in {path}"
        );
    }
}
