//! A complete synchronous lesson using ordinary helpers and shared resources.
//!
//! The paired Python source is `web/python/examples/first_lesson.py`. Native
//! execution is `cargo run -p noon-native --example first_lesson`; the same source
//! exports with `cargo run -p noon-export --example first_lesson -- lesson.mp4`.

use crate::{
    AnimationOptions, BlockingScene, Color, MathTypst, MobjectFamily, RateFunction, ScriptError,
    Text, Vec2,
};

fn step(scene: &BlockingScene, shapes: &MobjectFamily) -> Result<(), ScriptError> {
    scene.play_with(
        shapes.animate().shift(Vec2::new(0.75, 0.0)),
        AnimationOptions::new()
            .run_time(0.5)
            .rate_func(RateFunction::Linear),
    )
}

/// Construct two seconds of text, math and grouped motion without async syntax.
pub fn construct(scene: &BlockingScene) -> Result<(), ScriptError> {
    let title = scene.text(
        Text::new("One scene, ordinary functions")
            .with_font_size(36.0)
            .shift(Vec2::new(0.0, 2.7)),
    )?;
    let equation = scene.math_typst(
        MathTypst::new("x(t) = x_0 + v t")
            .with_font_size(48.0)
            .shift(Vec2::new(0.0, 1.6)),
    )?;
    scene.add(&title)?;
    scene.add(&equation)?;

    let square = scene.square(0.8)?;
    let circle = scene.circle(0.4)?;
    scene.set_fill(&square, Color::BLUE, 1.0)?;
    scene.set_fill(&circle, Color::ORANGE, 1.0)?;
    scene.shift(&square, Vec2::new(-0.8, -0.4))?;
    scene.shift(&circle, Vec2::new(0.8, -0.4))?;
    let shapes = scene.family([&square, &circle])?;
    scene.add(&shapes)?;
    scene.wait(0.5)?;
    for _ in 0..2 {
        step(scene, &shapes)?;
    }

    // This resource is constructed after execution has started. The same Scene
    // admits it atomically; the source never needs raw storage or host plumbing.
    let caption = scene.text(
        Text::new("Two steps, one shared scene")
            .with_font_size(24.0)
            .shift(Vec2::new(0.0, -2.0)),
    )?;
    scene.add(&caption)?;
    scene.wait(0.5)
}
