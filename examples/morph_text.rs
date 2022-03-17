use noon::prelude::*;

fn scene(win_rect: Rect) -> Scene {
    let mut scene = Scene::new(win_rect);

    let mut morph = Vec::new();
    let mut show = Vec::new();

    for _ in 0..3 {
        let text2 = random_text(&mut scene, "This example shows shape transfrom");
        show.push(text2.show_creation());

        let text = random_text(&mut scene, "Hello World! This is some text");
        show.push(text.show_creation());

        morph.push(text.morph(text2));
        morph.push(text2.fade_out());
    }

    scene.play(show).run_time(2.0);
    scene.play(morph).run_time(2.0);

    scene
}

fn random_text(scene: &mut Scene, text: &str) -> TextId {
    let (x, y, _w, _h, _ang, color) = gen_random_values();
    scene
        .text()
        .with_text(text)
        .with_font_size(50)
        .with_color(color)
        .with_position(x - 2.0, y)
        .make()
}

fn gen_random_values() -> (f32, f32, f32, f32, f32, Color) {
    let x_lim = 4.0;
    let y_lim = 2.0;
    let x = random_range::<f32>(-x_lim, x_lim);
    let y = random_range::<f32>(-y_lim, y_lim);
    let w = random_range::<f32>(0.1, 0.3);
    let h = random_range::<f32>(0.1, 0.3);
    let ang = random_range::<f32>(0.0, 360.0);
    let color = Color::random();

    (x, y, w, h, ang, color)
}

fn main() {
    noon::app(model).update(update).view(view).run();
}

fn model<'a>(app: &App) -> Scene {
    // app.new_window().size(640, 480).view(view).build().unwrap();
    app.new_window()
        .size(1920, 1080)
        // .key_pressed(key_pressed)
        // .mouse_pressed(mouse_pressed)
        .view(view)
        .build()
        .unwrap();

    let scene = scene(app.window_rect());
    scene
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

// fn mouse_pressed(app: &App, scene: &mut Scene, _button: MouseButton) {
//     scene.add_circle(app.mouse.x, app.mouse.y);
// }

// fn key_pressed(_app: &App, _model: &mut Scene, key: Key) {
//     match key {
//         Key::Key1 => {
//             // model.interpolate_shortest = true;
//         }
//         Key::Key2 => {
//             // model.interpolate_shortest = false;
//         }
//         Key::S => {
//             // app.main_window()
//             //     .capture_frame(app.exe_name().unwrap() + ".png");
//         }
//         _other_key => {}
//     }
// }
