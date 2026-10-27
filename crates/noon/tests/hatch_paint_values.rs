//! Explicit codec boundary; ordinary native hatch execution remains typed.
use noon::{Color, HatchPaint, HatchPaintOptions};

#[test]
fn hatch_codec_retains_f64_options_and_rejects_unchecked_values() {
    let value = HatchPaint::new(
        Color::rgba(1.0, 0.0, 0.0, 0.5),
        Color::BLUE,
        HatchPaintOptions {
            spacing: 0.123456789123456,
            width: 0.0123456789123456,
            angle: 12.3456789123456,
            phase: [4_294_967_296.125, -1e-100],
            cross: true,
        },
    )
    .unwrap();
    let encoded = serde_json::to_value(&value).unwrap();
    assert_eq!(
        serde_json::from_value::<HatchPaint>(encoded.clone()).unwrap(),
        value
    );
    for (field, replacement) in [
        ("spacing", serde_json::json!(0.0)),
        ("width", serde_json::json!(1.0)),
        ("angle", serde_json::json!(null)),
        ("phase", serde_json::json!([0])),
        ("cross", serde_json::json!(1)),
        ("extra", serde_json::json!(true)),
    ] {
        let mut invalid = encoded.clone();
        invalid["options"][field] = replacement;
        assert!(serde_json::from_value::<HatchPaint>(invalid).is_err());
    }
    let mut invalid = encoded.clone();
    invalid["foreground"]["red"] = serde_json::json!(1.0001);
    assert!(serde_json::from_value::<HatchPaint>(invalid).is_err());
    let mut invalid = encoded;
    invalid["extra"] = serde_json::json!(0);
    assert!(serde_json::from_value::<HatchPaint>(invalid).is_err());
}
