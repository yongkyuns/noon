use noon::prelude::*;

fn scene(win_rect: Rect) -> Scene {
    let mut scene = Scene::new(win_rect);

    // let mut builder = scene.group();
    // for _ in 0..5 {
    //     let rect = scene
    //         .rectangle()
    //         .with_position(2.0, 0.0)
    //         .with_size(0.5, 0.5)
    //         .with_color(Color::random())
    //         .show();

    //     builder.add(rect);
    // }

    // let group = builder.make();

    // scene.play(group.arrange(Alignment::Vertical, 0.0));

    // scene
    //     .play(vec![
    //         group.(noon::PI),
    //         // group.scale(0.01),
    //         // group.move_to(2.0, 2.0),
    //     ])
    //     .run_time(2.0);

    // scene.play(vec![
    //     group.rotate(-noon::PI),
    //     group.scale(100.0),
    //     group.move_to(0.0, 0.0),
    // ]);

    // let text = scene.text().with_text("Hello!").make();
    // let line = scene.line().from(0.0, 0.0).to(1.0, 0.0).make();
    // let line = scene.line().from(-0.0, 0.0000000).to(1.0, 0.0).make();
    // let line = scene.line().from(2.0, 0.0).to(3.0, 1.0).make();
    // let line = scene.line().from(1.0, 0.0).to(2.0, 1.0).make();

    // let group = scene.group().add(line).add(text).make();

    // scene.play(vec![circle.show_creation(), rect.fade_in()]);
    // scene.play(circle.scale(0.5));
    // scene.play(circle.to_edge(Direction::Up));
    // // scene.play(circle.move_to(-2.0, 4.5));
    // scene.play(circle.scale(2.0));
    // scene.play(circle.to_edge(Direction::Right));

    // scene.play(rect.rotate(noon::PI / 4.0));
    // scene.play(rect.to_edge(Direction::Up));

    // scene.play(rect.scale(2.0));
    // scene.play(rect.to_edge(Direction::Left));

    // scene.play(vec![text.show_creation(), line.show_creation()]);

    // scene.play(vec![line.show_creation(), text.show_creation()]);

    // scene.play(group.rotate(noon::PI / 4.0));
    // scene.play(vec![rect.show_creation(), rect2.show_creation()]);
    // scene.play(rect.move_to(2.0, 2.0));
    // scene.play(rect.rotate(noon::PI / 4.0));
    // scene.play(rect.scale(0.1));
    // scene.play(rect.scale_x(0.1));
    // scene.play(rect.set_size(0.5, 1.0));
    // scene.play(rect.move_by(-2.0, -2.0));
    // scene.wait();
    // scene.play(rect.to_edge(Direction::Right));
    // scene.play(rect.morph(rect2));

    // let group = scene.group(vec![line,text]).make();
    // let group = scene.group().add(line).add(text).make();

    // scene.arrange(group).vertical();
    // scene.arrange(group).horizontal();
    // scene.play(group.rotate(noon::PI/4.0));

    // scene.play(text.scale(2.0));
    // scene.play(text.rotate(noon::TAU / 8.0));
    // scene.play(vec![text.scale(2.0), text.rotate(noon::TAU / 8.0)]);

    // scene.play(text.to_edge(Direction::Up));

    // scene.play(vec![text.scale(2.0), text.rotate(noon::PI / 4.0)]);
    // scene.play(line.show_creation());

    // scene.play(vec![
    //     text.rotate(noon::PI / 4.0),
    //     line.rotate(noon::PI / 4.0),
    //     rect.rotate(noon::PI / 4.0),
    // ]);
    // scene.play(vec![
    //     text.to_edge(Direction::Up),
    //     line.to_edge(Direction::Up),
    //     rect.to_edge(Direction::Up),
    // ]);

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
