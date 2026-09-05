use noon::legacy::prelude::*;
use noon_ir::encode_scene;

fn triangle() -> Path {
    let path = VectorPath::new()
        .move_to(Vec2::new(0.0, 1.0))
        .line_to(Vec2::new(-0.866_025_4, -0.5))
        .line_to(Vec2::new(0.866_025_4, -0.5))
        .close();
    Path::new(path)
}

fn moving_camera_center() -> MovingCameraScene {
    let mut scene = MovingCameraScene::new();
    scene.wait(0.3).unwrap();

    let square = scene.add(
        Square::default()
            .color(RED)
            .set_fill(Some(RED), Some(0.5))
            .move_to(LEFT * 2.0),
    );
    let triangle = scene.add(
        triangle()
            .color(GREEN)
            .set_fill(Some(GREEN), Some(0.5))
            .move_to(RIGHT * 2.0),
    );

    let frame = scene.camera_frame();
    scene
        .play(frame.animate().move_to(LEFT * 2.0))
        .run_time(1.0)
        .unwrap();
    scene.wait(0.3).unwrap();
    scene
        .play(frame.animate().move_to(RIGHT * 2.0))
        .run_time(1.0)
        .unwrap();

    // Keep the target objects live so their semantic identities remain part of the
    // authored scene just as they are in Manim's MovingCameraCenter example.
    let _ = (square, triangle);
    scene
}

fn main() {
    let scene = moving_camera_center();
    println!(
        "MovingCameraCenter\t{}",
        encode_scene(&scene.into_definition()).unwrap()
    );
}
