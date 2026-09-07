use std::{cell::RefCell, rc::Rc};

use noon::Mobject;
use noon_core::SemanticStore;
use serde_json::{json, Map, Value};

fn observation(object: &Mobject) -> Value {
    let center = object.center().expect("typed sector center");
    json!({
        "center": [center.0, center.1],
        "width": object.width().expect("typed sector width"),
        "height": object.height().expect("typed sector height"),
    })
}

fn main() {
    let store = Rc::new(RefCell::new(SemanticStore::new()));
    let mut observations = Map::new();

    observations.insert(
        "annular_default".to_owned(),
        observation(
            &Mobject::manim_annular_sector(
                Rc::clone(&store),
                1.0,
                2.0,
                std::f64::consts::FRAC_PI_2,
                0.0,
                9,
                0.0,
                0.0,
            )
            .expect("default annular sector"),
        ),
    );
    observations.insert(
        "annular_signed_offset".to_owned(),
        observation(
            &Mobject::manim_annular_sector(
                Rc::clone(&store),
                0.5,
                2.25,
                -std::f64::consts::FRAC_PI_3,
                std::f64::consts::FRAC_PI_4,
                9,
                1.25,
                -0.75,
            )
            .expect("signed offset annular sector"),
        ),
    );
    observations.insert(
        "sector_offset".to_owned(),
        observation(
            &Mobject::manim_sector(
                Rc::clone(&store),
                2.0,
                std::f64::consts::FRAC_PI_2,
                -std::f64::consts::FRAC_PI_4,
                9,
                -1.5,
                0.75,
            )
            .expect("offset sector"),
        ),
    );
    observations.insert(
        "annulus_offset".to_owned(),
        observation(
            &Mobject::manim_annulus(Rc::clone(&store), 0.5, 1.75, 9, 0.8, -1.1)
                .expect("offset annulus"),
        ),
    );

    println!(
        "{}",
        serde_json::to_string(&Value::Object(observations)).expect("serialize observations")
    );
}
