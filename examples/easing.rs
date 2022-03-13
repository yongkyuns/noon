use noon::prelude::*;

fn scene(win_rect: Rect) -> Scene {
    let mut scene = Scene::new(win_rect);

    let mut circles = Vec::new();
    let mut show = Vec::new();
    let mut to_right = Vec::new();

    for i in 0..8 {
        let c = scene
            .circle()
            .with_position(-4.0, 2.0 - i as f32 * 0.5)
            .with_radius(0.2)
            .with_color(Color::random())
            .with_stroke_weight(5.0)
            .make();

        circles.push(c);
        show.push(c.fade_in());
        to_right.push(c.move_by(8.0, 0.0));
        // to_right.push(c.move_to(4.0, 2.0 - i as f32 * 0.5));
    }

    scene.wait();
    scene.play(show).lag(0.2);

    let easing = [
        EaseType::Linear,
        EaseType::Quad,
        EaseType::Quint,
        EaseType::Expo,
        EaseType::Sine,
        EaseType::Back,
        EaseType::Bounce,
        EaseType::Elastic,
    ];
    for i in 0..8 {
        scene
            .play(to_right[i].clone())
            .lag(0.5)
            .rate_func(easing[i]);
    }

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
