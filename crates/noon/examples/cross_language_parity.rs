use noon::legacy::prelude::*;
use noon_ir::encode_scene;

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
    let group = AnimationGroup::new((circle.animate().shift(UP), square.animate().shift(DOWN)))
        .lag_ratio(0.5)
        .rate_func(RateFunction::ThereAndBack);
    scene.play(group).run_time(3.0).unwrap();
    encode_scene(scene.definition()).unwrap()
}

fn main() {
    emit("animate_options", animate_options());
    emit("lifecycle", lifecycle());
    emit("nonlinear_composition", nonlinear_composition());
}
