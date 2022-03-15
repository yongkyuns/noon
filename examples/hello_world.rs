use noon::prelude::*;

fn scene(win_rect: Rect) -> Scene {
    let mut scene = Scene::new(win_rect);

    let rect = scene
        .rectangle()
        .with_position(2.0, 0.0)
        .with_color(Color::random())
        .make();

    let circle = scene
        .circle()
        .with_position(-2.0, 0.0)
        .with_color(Color::random())
        .make();

    scene.wait();
    scene.play(rect.show_creation()).run_time(1.5);
    scene.play(circle.show_creation()).run_time(1.5);
    scene.play(circle.morph(rect)).run_time(1.5);

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
    scene.update(app.time);
    println!("FPS = {}", app.fps());
}

fn view(app: &App, scene: &mut Scene, frame: Frame) {
    let draw = app.draw();
    draw.background().color(BLACK);
    scene.draw(draw.clone());
    draw.to_frame(app, &frame).unwrap();
}
