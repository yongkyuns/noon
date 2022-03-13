use noon::prelude::*;

fn main() {
    // noon::play(model);
    noon::app(model).update(update).view(view).run();
}

fn scene(win_rect: Rect) -> Scene {
    let mut scene = Scene::new(win_rect);

    let mut morph = Vec::new();
    let mut show = Vec::new();

    for _ in 0..5 {
        let (x, y, _w, _h, _ang, color) = gen_random_values();

        let circle = scene
            .circle()
            .with_position(x, y)
            .with_color(color)
            .with_radius(200.0 / 2.0)
            .make();
        show.push(circle.show_creation());

        let (x, y, _w, _h, _ang, color) = gen_random_values();

        let text = scene
            .text()
            .with_text("oijaweijfowiefowijfejwofeji")
            .with_font_size(50)
            .with_color(color)
            .with_position(x, y)
            .make();
        show.push(text.show_creation());

        morph.push(circle.morph(text));

        // self.play(vec![circle.move_to(400.0, 400.0), circle.fade_in()]);
        // self.play(vec![line.show_creation(), text.show_creation()]);

        // let (x, y, _w, _h, _ang, color) = gen_random_values();
        // let circle = self
        //     .circle()
        //     .with_position(0.0, 0.0)
        //     .with_color(color)
        //     .with_radius(200.0 / 2.0)
        //     .show();

        // self.wait();
        // let (x, y, _w, _h, _ang, color) = gen_random_values();
        // let rect = self
        //     .rectangle()
        //     .with_position(0.0, 0.0)
        //     .with_color(color)
        //     .with_size(150.0, 150.0)
        //     .show();

        // self.play(rect.show_creation()).run_time(3.0);

        // self.play(line.morph(circle)).run_time(3.0);
        // self.play(circle.morph(rect)).run_time(3.0);
        // self.play(rect.morph(text)).run_time(10.0);
    }

    scene.play(show).run_time(2.0).lag(0.01);
    scene.play(morph).run_time(5.0).lag(0.01);

    // self.play(rect.morph(text)).run_time(5.0);
    // self.play(rect.morph(text)).run_time(15.0);
    // self.play(text.morph(circle)).run_time(15.0);

    // self.play(vec![
    //     circle.move_to_object(rect),
    //     circle.set_color_from(rect),
    // ])
    // .rate_func(EaseType::Quint)
    // .run_time(2.0);

    // self.wait();
    // self.play(circle.move_to(400.0, 400.0))
    //     .rate_func(EaseType::Elastic);
    // self.play(circle.move_to(400.0, 400.0))
    //     .run_time(1.0)
    //     .lag(0.0001)
    //     .rate_func(EaseType::Quad);
    scene
}

fn gen_random_values() -> (f32, f32, f32, f32, f32, Color) {
    let x_lim = 1920.0 / 2.0;
    let y_lim = 1080.0 / 2.0;

    let x = random_range::<f32>(-x_lim, x_lim);
    let y = random_range::<f32>(-y_lim, y_lim);
    let w = random_range::<f32>(4.0, 60.0);
    let h = random_range::<f32>(4.0, 60.0);
    let ang = random_range::<f32>(0.0, 360.0);
    let color = Color::random();

    (x, y, w, h, ang, color)
}

fn model<'a>(app: &App) -> Scene {
    // app.new_window().size(640, 480).view(view).build().unwrap();
    app.new_window()
        .size(1920, 1080)
        .key_pressed(key_pressed)
        .mouse_pressed(mouse_pressed)
        .view(view)
        .build()
        .unwrap();

    let scene = scene(app.window_rect());
    scene
}

fn mouse_pressed(app: &App, scene: &mut Scene, _button: MouseButton) {
    scene.add_circle(app.mouse.x, app.mouse.y);
}

fn key_pressed(_app: &App, _model: &mut Scene, key: Key) {
    match key {
        Key::Key1 => {
            // model.interpolate_shortest = true;
        }
        Key::Key2 => {
            // model.interpolate_shortest = false;
        }
        Key::S => {
            // app.main_window()
            //     .capture_frame(app.exe_name().unwrap() + ".png");
        }
        _other_key => {}
    }
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
