use noon::prelude::*;

fn scene(win_rect: Rect) -> Scene {
    let mut scene = Scene::new(win_rect);

    let circle = scene
        .circle()
        .with_position(-2.0, 0.0)
        .with_color(Color::random())
        .make();

    let rect = scene
        .rectangle()
        .with_position(2.0, 0.0)
        .with_color(Color::random())
        .make();

    scene.play(vec![circle.show_creation(), rect.fade_in()]);
    // scene.play(circle.scale(0.5));
    // scene.play(circle.to_edge(Direction::Up));
    // // scene.play(circle.move_to(-2.0, 4.5));
    // scene.play(circle.scale(2.0));
    // scene.play(circle.to_edge(Direction::Right));

    scene.play(rect.rotate(noon::PI / 4.0));
    scene.play(rect.to_edge(Direction::Up));
    // scene.play(circle.move_to(-2.0, 4.5));
    // scene.play(circle.scale(2.0));
    // scene.play(circle.to_edge(Direction::Right));

    // for i in 0..1000 {
    //     let change = (i % 2) as f32;

    //     scene
    //         .play(vec![
    //             circle.set_radius(0.5 + change / 2.0),
    //             rect.scale_x(0.5 + 1.5 * change),
    //             rect.shift(LEFT),
    //             rect.rotate(noon::PI / 4.0),
    //         ])
    //         .run_time(1.0)
    //         .rate_func(EaseType::BackOut);
    // }

    scene
}

fn main() {
    noon::app(model).update(update).view(view).run();
}

fn model<'a>(app: &App) -> Scene {
    app.new_window()
        .size(1920, 1080)
        .view(view)
        .build()
        .unwrap();

    let scene = scene(app.window_rect());
    scene
}

fn update(app: &App, scene: &mut Scene, _update: Update) {
    scene.update(app.time, app.window_rect());
    // println!("FPS = {}", app.fps());
}

fn view(app: &App, scene: &mut Scene, frame: Frame) {
    let draw = app.draw();
    draw.background().color(BLACK);
    scene.draw(draw.clone());
    draw.to_frame(app, &frame).unwrap();
}
