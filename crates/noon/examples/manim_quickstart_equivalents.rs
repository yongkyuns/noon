use noon::legacy::prelude::*;
use noon_ir::encode_scene;

fn emit(name: &str, scene: &Scene) {
    println!("{name}\t{}", encode_scene(scene.definition()).unwrap());
}

fn create_circle() -> Scene {
    let mut scene = Scene::new();
    let circle = scene.add(Circle::default().set_fill(Some(PINK), Some(0.5)));
    scene.play(Create::new(circle)).run_time(1.0).unwrap();
    scene
}

fn square_to_circle() -> Scene {
    let mut scene = Scene::new();
    let circle = Circle::default().set_fill(Some(PINK), Some(0.5));
    let square = scene.add(Square::default().rotate(PI / 4.0));
    scene.play(Create::new(square)).run_time(1.0).unwrap();
    scene
        .play(Transform::new(square, circle))
        .run_time(1.0)
        .unwrap();
    scene.play(FadeOut::new(square)).run_time(1.0).unwrap();
    scene
}

fn square_and_circle() -> Scene {
    let mut scene = Scene::new();
    let circle = scene.add(Circle::default().set_fill(Some(PINK), Some(0.5)));
    let square = scene.add(Square::default().set_fill(Some(BLUE), Some(0.5)));
    scene
        .edit(square)
        .unwrap()
        .next_to(circle, RIGHT, 0.5)
        .unwrap();
    scene
        .play((Create::new(circle), Create::new(square)))
        .run_time(1.0)
        .unwrap();
    scene
}

fn animated_square_to_circle() -> Scene {
    let mut scene = Scene::new();
    let circle = Circle::default();
    let square = scene.add(Square::default());
    scene.play(Create::new(square)).run_time(1.0).unwrap();
    scene
        .play(square.animate().rotate(PI / 4.0))
        .run_time(1.0)
        .unwrap();
    scene
        .play(Transform::new(square, circle))
        .run_time(1.0)
        .unwrap();
    scene
        .play(square.animate().set_fill(Some(PINK), Some(0.5)))
        .run_time(1.0)
        .unwrap();
    scene
}

fn different_rotations() -> Scene {
    let mut scene = Scene::new();
    let left_square = scene.add(
        Square::default()
            .color(BLUE)
            .set_fill(Some(BLUE), Some(0.7))
            .shift(LEFT * 2.0),
    );
    let right_square = scene.add(
        Square::default()
            .color(GREEN)
            .set_fill(Some(GREEN), Some(0.7))
            .shift(RIGHT * 2.0),
    );
    scene
        .play((
            left_square.animate().rotate(PI),
            Rotate::new(right_square, PI),
        ))
        .run_time(2.0)
        .unwrap();
    scene.wait(1.0).unwrap();
    scene
}

fn main() {
    emit("CreateCircle", &create_circle());
    emit("SquareToCircle", &square_to_circle());
    emit("SquareAndCircle", &square_and_circle());
    emit("AnimatedSquareToCircle", &animated_square_to_circle());
    emit("DifferentRotations", &different_rotations());
}
