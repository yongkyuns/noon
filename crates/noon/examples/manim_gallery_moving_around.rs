use noon::prelude::*;
use noon_ir::encode_scene;

fn moving_around() -> Scene {
    let mut scene = Scene::new();
    let square = scene.add(
        Square::default()
            .color(BLUE)
            .set_fill(Some(BLUE), Some(1.0)),
    );

    scene
        .play(square.animate().shift(LEFT))
        .run_time(1.0)
        .unwrap();
    scene
        .play(square.animate().set_fill(Some(ORANGE), None))
        .run_time(1.0)
        .unwrap();
    scene
        .play(square.animate().scale(0.3))
        .run_time(1.0)
        .unwrap();
    scene
        .play(square.animate().rotate(0.4))
        .run_time(1.0)
        .unwrap();
    scene
}

fn main() {
    let scene = moving_around();
    println!(
        "MovingAround\t{}",
        encode_scene(scene.definition()).unwrap()
    );
}
