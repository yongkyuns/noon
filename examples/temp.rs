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

    // scene.wait();
    scene.play(vec![circle.fade_in(), rect.fade_in()]);

    let mut animation = Vec::new();

    for i in 0..1000 {
        let change = (i % 2) as f32;
        animation.extend(vec![
            circle.set_radius(0.5 + change / 2.0),
            rect.set_size(1.0 - change / 2.0, 1.0 + change),
        ]);
    }
    scene
        .play(animation)
        .run_time(0.3)
        .lag(0.3)
        .rate_func(EaseType::BackOut);

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
    // println!("FPS = {}", app.fps());
}

fn view(app: &App, scene: &mut Scene, frame: Frame) {
    let draw = app.draw();
    draw.background().color(BLACK);
    scene.draw(draw.clone());
    draw.to_frame(app, &frame).unwrap();
}
