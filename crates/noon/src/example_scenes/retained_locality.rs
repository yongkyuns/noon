//! Large sparse scene shared by native renderer and direct-WASM locality proofs.

use crate::{
    AnimationOptions, DeclaredAnimation, Mobject, MobjectTarget, RateFunction, Scene,
    SemanticAnimationCompositionKind, SemanticVec3,
};
use noon_core::{
    CompositionTimeMap, SemanticAnimationIntent, SemanticMutationTransaction,
    SemanticObjectTrackProperty, SemanticObjectTrackValues, TrackTiming,
};

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
    target_animation_with_duration(scene, target, 1.0)
}

/// Add the single-target track with an explicit duration for sustained browser sampling.
pub fn target_animation_with_duration(
    scene: &Scene,
    target: &Mobject,
    duration: f64,
) -> Result<DeclaredAnimation, String> {
    let from = target
        .state()
        .map_err(|error| error.to_string())?
        .transform
        .translation;
    let mut transaction = SemanticMutationTransaction::new();
    let track = transaction.create_object_property_track(
        target.node_id(),
        SemanticObjectTrackProperty::Position,
        SemanticObjectTrackValues::Vec3 {
            from,
            to: SemanticVec3::new(from.x + 1.0, from.y, from.z),
        },
        TrackTiming::new(0.0, duration, RateFunction::Linear),
        CompositionTimeMap::identity(),
    );
    let committed = transaction
        .apply(&mut scene.integration_store().borrow_mut())
        .map_err(|error| error.to_string())?;
    scene.declare_animation(
        SemanticAnimationIntent::Composition {
            kind: SemanticAnimationCompositionKind::Parallel,
            children: vec![committed.resolve(track).expect("committed position track")],
        },
        AnimationOptions::new(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use noon_core::Vec2;

    #[test]
    fn one_object_target_animation_lowers_and_seeks_as_an_exact_track_root() {
        let mut scene = Scene::new();
        let target = scene.circle(0.25).unwrap();
        scene.add(&target).unwrap();
        let animation = target_animation(&scene, &target).unwrap();
        let mut session = scene
            .execution_session_with_animation_root(&animation)
            .unwrap();
        session.seek(0.5).unwrap();
        assert_eq!(
            session.frame().objects[0].transform.translation,
            Vec2::new(0.5, 0.0)
        );
        session.seek(0.0).unwrap();
        assert_eq!(session.frame().objects[0].transform.translation, Vec2::ZERO);
    }
}
