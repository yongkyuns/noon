//! Explicit persistence/debug boundary; ordinary native paint stays typed.
use noon::{Color, GradientError, GradientStop, RadialGradient};

#[test]
fn radial_codec_preserves_values_and_rejects_unchecked_or_unknown_fields() {
    let paint = RadialGradient::new(
        [0.12345678912345678, 0.75],
        [0.5, 0.25],
        [
            GradientStop::new(0.0, Color::WHITE).unwrap(),
            GradientStop::new(1.0, Color::BLACK).unwrap(),
        ],
    )
    .unwrap();
    let encoded = serde_json::to_value(&paint).unwrap();
    assert_eq!(
        serde_json::from_value::<RadialGradient>(encoded.clone()).unwrap(),
        paint
    );
    for (field, value) in [
        ("center", serde_json::json!([-0.1, 0.5])),
        ("radii", serde_json::json!([0.0, 0.5])),
        ("stops", serde_json::json!([])),
        ("extra", serde_json::json!(true)),
    ] {
        let mut invalid = encoded.clone();
        invalid[field] = value;
        assert!(serde_json::from_value::<RadialGradient>(invalid).is_err());
    }
    let mut invalid = encoded;
    invalid["stops"][0]["extra"] = serde_json::json!(true);
    assert!(serde_json::from_value::<RadialGradient>(invalid).is_err());
    assert_eq!(
        RadialGradient::new([0.5; 2], [0.0, 0.5], []),
        Err(GradientError::InvalidRadii)
    );
}
