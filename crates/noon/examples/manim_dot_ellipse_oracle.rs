//! Explicit differential observations from shared Rust authoring and execution.
use noon::{Mobject, Scene};
use serde_json::{json, Value};
use std::rc::Rc;

fn observe(object: &Mobject) -> Value {
    let (x, y) = object.center().unwrap();
    json!({ "center": [x, y], "width": object.width().unwrap(),
        "height": object.height().unwrap() })
}

fn main() {
    let mut scene = Scene::new();
    let dot = Mobject::manim_dot(Rc::clone(scene.store()), 0.0, 0.0, 0.08).unwrap();
    let shifted = Mobject::manim_dot(Rc::clone(scene.store()), -2.0, 0.75, 0.18).unwrap();
    let ellipse = Mobject::manim_ellipse(Rc::clone(scene.store()), 2.0, 1.0).unwrap();
    let mut transformed = Mobject::manim_ellipse(Rc::clone(scene.store()), 4.0, 1.5).unwrap();
    transformed.rotate(std::f64::consts::PI / 6.0).unwrap();
    transformed.shift(1.25, -0.5).unwrap();
    for object in [&dot, &shifted, &ellipse, &transformed] {
        scene.add(object).unwrap();
    }
    let _session = scene.execution_session().unwrap();
    println!(
        "{}",
        json!({
            "dot_geometry": { "default": observe(&dot), "shifted": observe(&shifted) },
            "ellipse_geometry": { "default": observe(&ellipse), "transformed": observe(&transformed) }
        })
    );
}
