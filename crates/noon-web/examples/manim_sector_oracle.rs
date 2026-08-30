use noon_core::ObjectSnapshot;
use noon_web::{
    manim_annular_sector_snapshot_json, manim_annulus_snapshot_json, manim_sector_snapshot_json,
};
use serde_json::{json, Map, Value};

fn decode(snapshot_json: String) -> ObjectSnapshot {
    serde_json::from_str(&snapshot_json).expect("sector bridge must emit ObjectSnapshot JSON")
}

fn observation(snapshot: ObjectSnapshot) -> Value {
    let center = snapshot.center();
    json!({
        "center": [center.x, center.y],
        "width": snapshot.width(),
        "height": snapshot.height(),
    })
}

fn main() {
    let mut observations = Map::new();

    observations.insert(
        "annular_default".to_owned(),
        observation(decode(
            manim_annular_sector_snapshot_json(
                1.0,
                2.0,
                std::f64::consts::FRAC_PI_2,
                0.0,
                9,
                0.0,
                0.0,
            )
            .expect("default annular sector"),
        )),
    );
    observations.insert(
        "annular_signed_offset".to_owned(),
        observation(decode(
            manim_annular_sector_snapshot_json(
                0.5,
                2.25,
                -std::f64::consts::FRAC_PI_3,
                std::f64::consts::FRAC_PI_4,
                9,
                1.25,
                -0.75,
            )
            .expect("signed offset annular sector"),
        )),
    );
    observations.insert(
        "sector_offset".to_owned(),
        observation(decode(
            manim_sector_snapshot_json(
                2.0,
                std::f64::consts::FRAC_PI_2,
                -std::f64::consts::FRAC_PI_4,
                9,
                -1.5,
                0.75,
            )
            .expect("offset sector"),
        )),
    );
    observations.insert(
        "annulus_offset".to_owned(),
        observation(decode(
            manim_annulus_snapshot_json(0.5, 1.75, 9, 0.8, -1.1).expect("offset annulus"),
        )),
    );

    println!(
        "{}",
        serde_json::to_string(&Value::Object(observations)).expect("serialize observations")
    );
}
