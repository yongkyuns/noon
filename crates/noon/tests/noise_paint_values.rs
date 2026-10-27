//! Explicit persistence/debug boundary; native paint execution remains typed.
use noon::{Color, NoisePaint};

#[test]
fn checked_noise_codec_preserves_seed_precision_and_rejects_unchecked_fields() {
    let value = NoisePaint::new(
        u32::MAX,
        [8.123456789123456, 12.0],
        [-0.173, 4_294_967_296.231],
        [Color::rgba(1.0, 0.0, 0.0, 0.5), Color::BLUE],
    )
    .unwrap();
    let encoded = serde_json::to_value(&value).unwrap();
    assert_eq!(
        serde_json::from_value::<NoisePaint>(encoded.clone()).unwrap(),
        value
    );
    for (field, replacement) in [
        ("seed", serde_json::json!(-1)),
        ("seed", serde_json::json!(0.5)),
        ("frequency", serde_json::json!([0.0, 8.0])),
        ("phase", serde_json::json!([null, 0])),
        ("colors", serde_json::json!([])),
        ("extra", serde_json::json!(true)),
    ] {
        let mut invalid = encoded.clone();
        invalid[field] = replacement;
        assert!(serde_json::from_value::<NoisePaint>(invalid).is_err());
    }
    let mut invalid = encoded;
    invalid["colors"][0]["red"] = serde_json::json!(1.0001);
    assert!(serde_json::from_value::<NoisePaint>(invalid).is_err());
}
