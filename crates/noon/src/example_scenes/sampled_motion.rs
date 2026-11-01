//! Nonuniform sampled rigid motion, paired with Python's sampled_motion.py.
//!
//! The center keys are an explicit piecewise-linear approximation. Angular
//! offsets remain signed and unwrapped; neither sampling nor interpolation
//! requires host callbacks or copied target objects.

use std::sync::Arc;

use crate::{
    AnimationCompositionRequest as Request, AnimationOptions, Color, RateFunction,
    SampledMotionData, Scene, SemanticAnimationCompositionKind, SemanticVec3,
};

pub const DURATION: f64 = 2.3;
pub const PHASES: [f64; 4] = [0.13, 0.4, 0.73, 1.0];

pub fn session() -> Result<crate::SceneExecution, String> {
    let mut scene = Scene::new();
    let mut line = scene
        .line((-0.5, 0.0), (0.5, 0.0))
        .map_err(|error| error.to_string())?;
    line.set_translation(-2.0, 1.0)
        .map_err(|error| error.to_string())?;
    line.set_rotation(0.25).map_err(|error| error.to_string())?;
    line.set_color(
        f64::from(Color::BLUE.red),
        f64::from(Color::BLUE.green),
        f64::from(Color::BLUE.blue),
        1.0,
    )
    .map_err(|error| error.to_string())?;
    let mut square = scene.square(0.6).map_err(|error| error.to_string())?;
    square
        .set_translation(-2.0, -1.0)
        .map_err(|error| error.to_string())?;
    square
        .set_rotation(-0.4)
        .map_err(|error| error.to_string())?;
    square
        .set_color(
            f64::from(Color::RED.red),
            f64::from(Color::RED.green),
            f64::from(Color::RED.blue),
            1.0,
        )
        .map_err(|error| error.to_string())?;
    square
        .set_fill_opacity(1.0)
        .map_err(|error| error.to_string())?;
    scene.add(&line).map_err(|error| error.to_string())?;
    scene.add(&square).map_err(|error| error.to_string())?;

    let motion = |centers: [(f64, f64); 4], radians: f64| {
        SampledMotionData::new(
            PHASES.to_vec(),
            centers
                .into_iter()
                .map(|(x, y)| SemanticVec3::new(x, y, 0.0))
                .collect(),
            PHASES.iter().map(|phase| radians * phase).collect(),
        )
        .map(Arc::new)
        .map_err(|error| error.to_string())
    };
    let options = AnimationOptions::new()
        .run_time(DURATION)
        .rate_func(RateFunction::Linear);
    let request = Request::Composition {
        kind: SemanticAnimationCompositionKind::Parallel,
        children: vec![
            Request::SampledMotion {
                target: &line,
                motion: motion(
                    [(-1.0, 2.0), (1.0, 1.0), (2.0, 2.0), (2.0, 0.0)],
                    -6.0 * std::f64::consts::PI,
                )?,
                options,
            },
            Request::SampledMotion {
                target: &square,
                motion: motion(
                    [(-1.0, -2.0), (1.0, -1.0), (2.0, -2.0), (2.0, 0.0)],
                    8.0 * std::f64::consts::PI,
                )?,
                options,
            },
        ],
        options: AnimationOptions::new(),
    };
    let mut session = scene
        .execution_session()
        .map_err(|error| error.to_string())?;
    scene
        .live(&mut session)
        .declare_and_activate_composition(&request, AnimationOptions::new())
        .map_err(|error| error.to_string())?;
    scene
        .into_execution_with_session(session)
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_phase(scene: &crate::SceneExecution, phase: f64) {
        let frame = scene.session().frame();
        let phases = [0.0, 0.13, 0.4, 0.73, 1.0];
        let x = [-2.0, -1.0, 1.0, 2.0, 2.0];
        let y = [1.0, 2.0, 1.0, 2.0, 0.0];
        let upper = phases.partition_point(|key| *key <= phase).clamp(1, 4);
        let lower = upper - 1;
        let alpha = (phase - phases[lower]) / (phases[upper] - phases[lower]);
        let expected_x = x[lower] + (x[upper] - x[lower]) * alpha;
        let expected_y = y[lower] + (y[upper] - y[lower]) * alpha;
        for (index, sign, initial, radians) in [
            (0, 1.0, 0.25, -6.0 * std::f64::consts::PI),
            (1, -1.0, -0.4, 8.0 * std::f64::consts::PI),
        ] {
            let transform = frame.objects[index].transform;
            assert!((f64::from(transform.translation.x) - expected_x).abs() < 2e-6);
            assert!((f64::from(transform.translation.y) - sign * expected_y).abs() < 2e-6);
            assert!((f64::from(transform.rotation) - (initial + radians * phase)).abs() < 4e-6);
        }
    }

    #[test]
    fn sampled_motion_forward_random_and_reverse_seek_share_the_same_retained_geometry() {
        let mut forward = session().unwrap();
        let mut seek = session().unwrap();
        let geometry = forward
            .session()
            .frame()
            .render_geometry(0)
            .unwrap()
            .clone();
        let revision = forward.session().publication_context().scene_revision();
        for phase in [0.0, 0.065, 0.13, 0.3, 0.4, 0.6, 0.73, 0.91, 1.0] {
            forward.session_mut().advance_to(phase * DURATION).unwrap();
            seek.seek(phase * DURATION).unwrap();
            assert_phase(&forward, phase);
            assert_phase(&seek, phase);
            for (left, right) in forward
                .session()
                .frame()
                .objects
                .iter()
                .zip(&seek.session().frame().objects)
            {
                assert_eq!(left.transform, right.transform);
                assert_eq!(left.style, right.style);
            }
            assert_eq!(
                forward.session().frame().render_geometry(0),
                Some(&geometry)
            );
            if let (
                crate::GeometryRef::VectorPath(original),
                Some(crate::GeometryRef::VectorPath(current)),
            ) = (&geometry, forward.session().frame().render_geometry(0))
            {
                assert!(Arc::ptr_eq(original, current));
            }
            assert_eq!(
                forward.session().publication_context().scene_revision(),
                revision
            );
        }
        for phase in [0.73, 0.0, 0.91, 0.13, 0.6, 0.3, 1.0, 0.065] {
            seek.seek(phase * DURATION).unwrap();
            assert_phase(&seek, phase);
        }
        forward.session_mut().take_frame_changes();
        forward.session_mut().advance_to(DURATION).unwrap();
        assert!(forward
            .session_mut()
            .take_frame_changes()
            .object_indices()
            .is_empty());
    }
}
