use noon::prelude::*;

fn scene(win_rect: Rect) -> Scene {
    let mut scene = Scene::new(win_rect);

    let mut animations = Vec::new();
    let mut show = Vec::new();
    let mut move_down = Vec::new();

    for _ in 0..1000 {
        if noon::rand::random::<bool>() {
            let (x, y, w, _h, ang, color) = gen_random_values();
            let circle = scene
                .circle()
                .with_position(x, y)
                .with_angle(ang)
                .with_color(color)
                .with_thin_stroke()
                .with_radius(w / 2.0)
                .make();

            show.push(circle.show_creation());
            move_down.push(circle.to_edge(Direction::Down));

            let (x, y, w, _h, _ang, color) = gen_random_values();
            animations.extend(vec![
                circle.set_color(color),
                circle.move_to(x, y),
                circle.set_radius(w / 2.0),
            ]);
        } else {
            let (x, y, w, h, ang, color) = gen_random_values();
            let rect = scene
                .rectangle()
                .with_position(x, y)
                .with_angle(ang)
                .with_color(color)
                .with_thin_stroke()
                .with_size(w, h)
                .make();

            show.push(rect.show_creation());
            move_down.push(rect.to_edge(Direction::Down));

            let (x, y, w, _h, ang, color) = gen_random_values();
            animations.extend(vec![
                rect.set_color(color),
                rect.move_to(x, y),
                rect.set_size(w, h),
                rect.rotate(ang),
            ]);
        }
    }

    scene.wait_for(0.5);
    scene.play(show).run_time(1.0).lag(0.001);

    scene
        .play(animations)
        .run_time(3.0)
        .lag(0.0001)
        .rate_func(EaseType::Quint);

    scene
        .play(move_down)
        .run_time(1.0)
        .rate_func(EaseType::BounceOut)
        .lag(0.001);

    scene
}

fn gen_random_values() -> (f32, f32, f32, f32, f32, Color) {
    let x_lim = 4.0;
    let y_lim = 2.0;
    let x = random_range::<f32>(-x_lim, x_lim);
    let y = random_range::<f32>(-y_lim, y_lim);
    let w = random_range::<f32>(0.1, 0.3);
    let h = random_range::<f32>(0.1, 0.3);
    let ang = random_range::<f32>(0.0, noon::TAU);
    let color = Color::random();

    (x, y, w, h, ang, color)
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

    scene(app.window_rect())
}

fn update(app: &App, scene: &mut Scene, _update: Update) {
    scene.update(app.time, app.window_rect());
    println!("FPS = {}", app.fps());
}

fn view(app: &App, scene: &mut Scene, frame: Frame) {
    let draw = app.draw();
    draw.background().color(BLACK);
    scene.draw(draw.clone());
    draw.to_frame(app, &frame).unwrap();
}
