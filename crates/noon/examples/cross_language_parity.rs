use noon::prelude::*;
use noon_ir::{encode_scene, encode_timed_semantic_scene};

fn emit(name: &str, document: String) {
    println!("{name}\t{document}");
}

fn animate_options() -> String {
    let mut scene = Scene::new();
    let circle = scene.add(Circle::new(1.0));
    scene
        .play(circle.animate().shift(RIGHT).rotate(0.25))
        .rate_func(RateFunction::Smooth)
        .run_time(2.0)
        .unwrap();
    encode_scene(scene.definition()).unwrap()
}

fn lifecycle() -> String {
    let mut scene = Scene::new();
    let circle = scene.add(Circle::new(0.5));
    scene.play(Create::new(circle)).run_time(1.0).unwrap();
    scene.play(FadeOut::new(circle)).run_time(0.5).unwrap();
    scene.play(FadeIn::new(circle)).run_time(0.5).unwrap();
    encode_scene(scene.definition()).unwrap()
}

fn nonlinear_composition() -> String {
    let mut scene = Scene::new();
    let circle = scene.add(Circle::new(0.4));
    let square = scene.add(Square::new(0.8));
    let group = AnimationGroup::new((
        circle.animate().shift(UP),
        square.animate().shift(DOWN),
    ))
    .lag_ratio(0.5)
    .rate_func(RateFunction::ThereAndBack);
    scene.play(group).run_time(3.0).unwrap();
    encode_scene(scene.definition()).unwrap()
}

fn value_tracker() -> String {
    let mut scene = ReactiveTimelineScene::new();
    let circle = scene.add(Circle::new(0.3));
    let tracker = scene.value_tracker(0.0);
    let position = scene.position_from_tracker(tracker, RIGHT, UP);
    scene.bind_position(circle, position);
    scene
        .play_value(tracker, 2.0)
        .rate_func(RateFunction::Linear)
        .run_time(2.0)
        .unwrap();
    encode_timed_semantic_scene(&scene.timed_semantic_scene().unwrap()).unwrap()
}

fn main() {
    emit("animate_options", animate_options());
    emit("lifecycle", lifecycle());
    emit("nonlinear_composition", nonlinear_composition());
    emit("value_tracker", value_tracker());
}
