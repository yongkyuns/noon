use nannou::prelude::*;

use crate::scene::{self, Construct, Scene};

pub fn run() {
    nannou::app(scene).update(update).view(view).run();
}

fn scene<'a>(app: &App) -> Scene {
    // app.new_window().size(640, 480).view(view).build().unwrap();
    app.new_window()
        .size(1920, 1080)
        .view(view)
        .build()
        .unwrap();
    let win_rect = app.main_window().rect();

    let mut scene = scene::Scene::new(win_rect);
    scene.construct();
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
