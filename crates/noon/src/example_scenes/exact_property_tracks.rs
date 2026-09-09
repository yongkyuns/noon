//! Exact authored channels lower once into the shared runtime on native and WASM.

use crate::{
    AnimationOptions, ExecutionSession, RateFunction, Scene, SemanticAnimationCompositionKind,
    SemanticVec3,
};
use noon_core::{
    CompositionTimeMap, SemanticAnimationIntent, SemanticMutationTransaction,
    SemanticObjectTrackProperty, SemanticObjectTrackValues, TrackTiming,
};

/// A moving, fading red circle above an independently rotating blue square.
/// Python's paired example expresses the same endpoints with ordinary animation builders.
pub fn session() -> Result<ExecutionSession, String> {
    let mut scene = Scene::new();
    let mut circle = scene.circle(0.75)?;
    circle.set_translation(-2.0, 1.0)?;
    circle.set_color(1.0, 0.0, 0.0, 1.0)?;
    circle.set_fill(1.0, 0.0, 0.0, 1.0)?;
    let mut square = scene.square(1.5)?;
    square.set_translation(0.0, -1.0)?;
    square.set_color(0.0, 0.0, 1.0, 1.0)?;
    square.set_fill(0.0, 0.0, 1.0, 1.0)?;
    scene.add(&circle).map_err(|error| error.to_string())?;
    scene.add(&square).map_err(|error| error.to_string())?;

    let mut transaction = SemanticMutationTransaction::new();
    let timing = TrackTiming::new(0.0, 2.0, RateFunction::Linear);
    let position = transaction.create_object_property_track(
        circle.node_id(),
        SemanticObjectTrackProperty::Position,
        SemanticObjectTrackValues::Vec3 {
            from: SemanticVec3::new(-2.0, 1.0, 0.0),
            to: SemanticVec3::new(2.0, 1.0, 0.0),
        },
        timing,
        CompositionTimeMap::identity(),
    );
    let opacity = transaction.create_object_property_track(
        circle.node_id(),
        SemanticObjectTrackProperty::Opacity,
        SemanticObjectTrackValues::Scalar {
            from: 1.0,
            to: 0.25,
        },
        timing,
        CompositionTimeMap::identity(),
    );
    let rotation = transaction.create_object_property_track(
        square.node_id(),
        SemanticObjectTrackProperty::Rotation,
        SemanticObjectTrackValues::Scalar {
            from: 0.0,
            to: std::f64::consts::PI,
        },
        timing,
        CompositionTimeMap::identity(),
    );
    let result = transaction
        .apply(&mut scene.integration_store().borrow_mut())
        .map_err(|error| error.to_string())?;
    let root = scene.declare_animation(
        SemanticAnimationIntent::Composition {
            kind: SemanticAnimationCompositionKind::Parallel,
            children: [position, opacity, rotation]
                .map(|token| result.resolve(token).expect("committed track"))
                .to_vec(),
        },
        AnimationOptions::new(),
    )?;
    scene.execution_session_with_animation_root(&root)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_property_tracks_seek_agrees_with_forward_and_unchanged_frames_stay_clean() {
        let mut forward = session().unwrap();
        let mut seek = session().unwrap();
        for time in [0.0, 0.25, 1.0, 1.75, 2.0] {
            forward.advance_to(time).unwrap();
            seek.seek(time).unwrap();
            for (left, right) in forward.frame().objects.iter().zip(&seek.frame().objects) {
                assert_eq!(left.transform, right.transform);
                assert_eq!(left.style, right.style);
            }
            let circle = &forward.frame().objects[0];
            assert!((circle.transform.translation.x - (-2.0 + 2.0 * time as f32)).abs() < 1e-6);
            assert!((circle.style.opacity - (1.0 - 0.375 * time as f32)).abs() < 1e-6);
            let square = &forward.frame().objects[1];
            assert!(
                (square.transform.rotation - std::f32::consts::PI * time as f32 / 2.0).abs() < 1e-6
            );
        }
        forward.take_frame_changes();
        forward.advance_to(2.0).unwrap();
        assert!(forward.take_frame_changes().object_indices().is_empty());
        seek.seek(0.25).unwrap();
        assert!((seek.frame().objects[0].transform.translation.x + 1.5).abs() < 1e-6);
    }
}
