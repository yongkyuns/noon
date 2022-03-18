use noon::prelude::*;

fn scene(win_rect: Rect) -> Scene {
    let mut scene = Scene::new(win_rect);

    let text = scene.text().with_text("Hello!").make();

    let rectangle = scene.rectangle().with_position(2.0, 0.0).make();

    let circle = scene.circle().with_position(-2.0, 0.0).make();

    let line = scene.line().from(-2.0, -2.0).to(2.0, 2.0).make();

    scene
        .play(vec![
            line.show_creation(),
            circle.show_creation(),
            rectangle.show_creation(),
            text.show_creation(),
        ])
        .lag(1.0);

    scene
        .play(vec![
            line.morph(circle),
            circle.morph(rectangle),
            rectangle.morph(text),
        ])
        .run_time(2.0)
        .lag(2.0);

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
    println!("FPS = {}", app.fps());
}

fn view(app: &App, scene: &mut Scene, frame: Frame) {
    let draw = app.draw();
    draw.background().color(BLACK);
    scene.draw(draw.clone());
    draw.to_frame(app, &frame).unwrap();
}
