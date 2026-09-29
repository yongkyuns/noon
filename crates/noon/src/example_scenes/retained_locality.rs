//! Large sparse scene shared by native renderer and direct-WASM locality proofs.

use crate::{AnimationOptions, DeclaredAnimation, Mobject, MobjectTarget, RateFunction, Scene};

pub const OBJECT_COUNT: usize = 100_000;
pub const TARGET_INDEX: usize = OBJECT_COUNT / 2;

/// Build 100k spatially distributed circles with only the selected target in view.
/// The sparse layout keeps the browser qualification from measuring overdraw.
pub fn scene() -> Result<(Scene, Vec<Mobject>), String> {
    let mut scene = Scene::new();
    let mut objects = Vec::with_capacity(OBJECT_COUNT);
    for index in 0..OBJECT_COUNT {
        let mut circle = scene.circle(0.25).map_err(|error| error.to_string())?;
        if index != TARGET_INDEX {
            let column = index % 1_000;
            let row = index / 1_000;
            circle
                .set_translation(100.0 + column as f64 * 2.0, 100.0 + row as f64 * 2.0)
                .map_err(|error| error.to_string())?;
        }
        objects.push(circle);
    }
    scene
        .add_many(&objects.iter().map(MobjectTarget::from).collect::<Vec<_>>())
        .map_err(|error| error.to_string())?;
    Ok((scene, objects))
}

/// Add the single-target track used by the direct-WASM authored-time proof.
pub fn target_animation(scene: &Scene, target: &Mobject) -> Result<DeclaredAnimation, String> {
    let mut endpoint = target.target_editor().map_err(|error| error.to_string())?;
    endpoint
        .shift(1.0, 0.0)
        .map_err(|error| error.to_string())?;
    scene.declare_transform_to(
        target,
        &endpoint,
        AnimationOptions::new()
            .run_time(1.0)
            .rate_func(RateFunction::Linear),
    )
}
