//! Default clouds, a customized paper ramp, and independent reuse of a paint.
//! Paired with `web/python/examples/fractal_noise.py`.
use crate::{
    AnimationOptions, Color, GradientStop, LiveProgram, NoisePaint, RateFunction, Scene,
    SceneScript, Vec2,
};
use std::sync::Arc;

pub fn paint(paper: bool, phase: [f64; 2]) -> Result<NoisePaint, String> {
    let (low, high) = if paper {
        (Color::rgb(0.86, 0.80, 0.68), Color::rgb(1.0, 0.97, 0.90))
    } else {
        (Color::rgb(0.05, 0.10, 0.25), Color::rgb(0.50, 0.80, 1.0))
    };
    let value = NoisePaint::fractal(low, high)
        .map_err(message)?
        .with_seed(7)
        .with_frequency(if paper { [18.0; 2] } else { [8.0; 2] })
        .map_err(message)?
        .with_phase(phase)
        .map_err(message)?;
    if paper {
        value
            .with_stops([
                GradientStop::new(0.0, low).map_err(message)?,
                GradientStop::new(0.45, Color::rgb(0.96, 0.91, 0.81)).map_err(message)?,
                GradientStop::new(1.0, high).map_err(message)?,
            ])
            .map_err(message)
    } else {
        Ok(value)
    }
}

pub fn program() -> Result<LiveProgram<SceneScript>, String> {
    let clouds = Arc::new(paint(false, [0.0; 2])?);
    let moving_clouds = paint(false, [1.5, 0.0])?;
    let grain = paint(true, [0.0; 2])?;
    Scene::script(move |scene| async move {
        let sky = scene.rectangle(3.6, 2.4)?;
        scene.set_stroke_width(&sky, 0.0)?;
        scene.shift(&sky, Vec2::new(-2.1, 0.0))?;
        scene.set_fill_noise(&sky, Arc::clone(&clouds), Some(1.0))?;

        let paper = scene.rectangle(3.6, 2.4)?;
        scene.set_stroke_width(&paper, 0.0)?;
        scene.shift(&paper, Vec2::new(2.1, 0.0))?;
        scene.set_fill_noise(&paper, grain, Some(1.0))?;

        let swatch = scene.rectangle(1.4, 0.45)?;
        scene.set_stroke_width(&swatch, 0.0)?;
        scene.shift(&swatch, Vec2::new(-2.1, -2.3))?;
        scene.set_fill_noise(&swatch, clouds, Some(1.0))?;
        for object in [&sky, &paper, &swatch] {
            scene.add(object)?;
        }
        scene.wait(0.5).await?;
        scene
            .play_with(
                sky.animate()
                    .shift(Vec2::new(0.0, 0.4))
                    .set_fill_noise(moving_clouds),
                AnimationOptions::new()
                    .run_time(3.0)
                    .rate_func(RateFunction::Linear),
            )
            .await?;
        scene.wait(0.5).await?;
        Ok(())
    })
    .map_err(message)
}

/// Bounded backend corpus: default, ramp/full seed, finest scale and single octave.
pub fn profile_scene(profile: u32) -> Result<Scene, String> {
    let mut value = NoisePaint::fractal(Color::rgba(1.0, 0.0, 0.0, 0.5), Color::rgb(0.0, 0.0, 1.0))
        .map_err(message)?
        .with_seed(7);
    let size = match profile {
        0 => [4.0, 2.0],
        1 => {
            value = value
                .with_seed(u32::MAX)
                .with_frequency([4.0, 5.0])
                .map_err(message)?
                .with_detail(5, 2.0, 0.6)
                .map_err(message)?
                .with_stops([
                    GradientStop::new(0.0, Color::rgba(1.0, 0.0, 0.0, 0.0)).map_err(message)?,
                    GradientStop::new(0.4, Color::rgb(0.0, 1.0, 0.0)).map_err(message)?,
                    GradientStop::new(1.0, Color::rgb(0.0, 0.0, 1.0)).map_err(message)?,
                ])
                .map_err(message)?;
            [4.0, 2.0]
        }
        2 => {
            value = value
                .with_frequency([1.0; 2])
                .map_err(message)?
                .with_detail(8, 2.0, 0.6)
                .map_err(message)?;
            [0.7, 0.5]
        }
        3 => {
            value = value.with_detail(1, 2.0, 0.5).map_err(message)?;
            [4.0, 2.0]
        }
        _ => return Err("fractal profile must be in [0,3]".into()),
    };
    value = value.with_phase([-0.173, 0.231]).map_err(message)?;
    let mut scene = Scene::new();
    let mut source = scene.rectangle(size[0], size[1]).map_err(message)?;
    source.disable_stroke().map_err(message)?;
    source.set_fill_noise(value, Some(0.7)).map_err(message)?;
    source.set_object_opacity(0.6).map_err(message)?;
    scene.add(&source).map_err(message)?;
    Ok(scene)
}
fn message(error: impl std::fmt::Display) -> String {
    error.to_string()
}
