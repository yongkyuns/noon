//! Retained trace and sampled pen sharing explicit elapsed-phase keys.
//!
//! Paired with Python's phase_reveal.py. Unequal chord lengths and nonuniform
//! key times distinguish elapsed phase from both curve count and arc length.

use std::sync::Arc;

use crate::{
    AnimationCompositionRequest as Request, AnimationOptions, Color, RateFunction, RevealPhaseMap,
    SampledMotionData, Scene, SemanticAnimationCompositionKind, SemanticPaint, SemanticStyle,
    SemanticVec3, Vec2, VectorPath,
};

pub const DURATION: f64 = 2.3;
pub const PHASES: [f64; 5] = [0.0, 0.13, 0.4, 0.73, 1.0];
pub const POINTS: [(f64, f64); 5] = [
    (-3.0, -1.0),
    (-2.0, 1.0),
    (0.5, -1.0),
    (2.0, 1.0),
    (2.5, -0.5),
];

pub fn session() -> Result<crate::SceneExecution, String> {
    let build = || -> Result<_, Box<dyn std::error::Error>> {
        let mut scene = Scene::new();
        let mut path = VectorPath::new().move_to(Vec2::new(POINTS[0].0 as f32, POINTS[0].1 as f32));
        for &(x, y) in &POINTS[1..] {
            path = path.line_to(Vec2::new(x as f32, y as f32));
        }
        let trace = scene.path(
            path,
            SemanticStyle {
                fill: None,
                stroke: Some(SemanticPaint::Solid(Color::BLUE)),
                stroke_width: 0.06,
                ..SemanticStyle::default()
            },
        )?;
        let mut pen = scene.circle(0.09)?;
        pen.set_translation(POINTS[0].0, POINTS[0].1)?;
        pen.set_color(
            f64::from(Color::RED.red),
            f64::from(Color::RED.green),
            f64::from(Color::RED.blue),
            1.0,
        )?;
        pen.set_fill_opacity(1.0)?;
        pen.set_stroke_width(0.0)?;
        scene.add(&pen)?;

        let options = AnimationOptions::new()
            .run_time(DURATION)
            .rate_func(RateFunction::Linear);
        let request = Request::Composition {
            kind: SemanticAnimationCompositionKind::Parallel,
            children: vec![
                Request::PhaseReveal {
                    target: &trace,
                    phase_map: Arc::new(RevealPhaseMap::new(PHASES.to_vec())?),
                    options,
                },
                Request::SampledMotion {
                    target: &pen,
                    motion: Arc::new(SampledMotionData::new(
                        PHASES[1..].to_vec(),
                        POINTS[1..]
                            .iter()
                            .map(|&(x, y)| SemanticVec3::new(x, y, 0.0))
                            .collect(),
                        vec![0.0; PHASES.len() - 1],
                    )?),
                    options,
                },
            ],
            options: AnimationOptions::new(),
        };
        let mut execution = scene.execution_session()?;
        scene
            .live(&mut execution)
            .declare_and_activate_composition(&request, AnimationOptions::new())?;
        Ok(scene.into_execution_with_session(execution)?)
    };
    build().map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GeometryRef;

    fn trace_index(frame: &noon_runtime::FrameState) -> usize {
        frame
            .objects
            .iter()
            .position(|object| (object.style.stroke_width - 0.06).abs() < 1e-6)
            .expect("retained blue trace")
    }

    fn assert_phase(scene: &crate::SceneExecution, phase: f64) {
        let frame = scene.session().frame();
        assert_eq!(frame.objects.len(), 2);
        let trace = trace_index(frame);
        let pen = 1 - trace;
        assert!(frame.is_present(trace));
        assert!(frame.is_present(pen));
        let upper = PHASES.partition_point(|key| *key <= phase).clamp(1, 4);
        let lower = upper - 1;
        let fraction = (phase - PHASES[lower]) / (PHASES[upper] - PHASES[lower]);
        let endpoint = Vec2::new(
            (POINTS[lower].0 + (POINTS[upper].0 - POINTS[lower].0) * fraction) as f32,
            (POINTS[lower].1 + (POINTS[upper].1 - POINTS[lower].1) * fraction) as f32,
        );
        let expected_reveal = (lower as f64 + fraction) / (PHASES.len() - 1) as f64;
        assert!((f64::from(frame.reveal(trace)) - expected_reveal).abs() <= 2e-7);
        assert!((frame.objects[pen].transform.translation - endpoint).length() <= 2e-5);
        let GeometryRef::VectorPath(path) = frame.render_geometry(trace).unwrap() else {
            panic!("trace must retain one vector path");
        };
        assert_eq!(path.commands().len(), POINTS.len());
        let partial = noon_geometry::authored_partial_path(path, 0.0, frame.reveal(trace));
        if phase > 0.0 {
            let (start, end) = noon_geometry::drawable_endpoints(&partial).unwrap();
            assert_eq!(start, Vec2::new(-3.0, -1.0));
            assert!((end - endpoint).length() <= 2e-5);
            let prefix = POINTS[..=lower]
                .iter()
                .map(|&(x, y)| Vec2::new(x as f32, y as f32))
                .chain(std::iter::once(endpoint));
            let expected_bounds = crate::Rect::from_points(prefix).unwrap();
            let bounds = GeometryRef::path(partial)
                .world_bounds(frame.objects[trace].transform)
                .unwrap();
            assert!((bounds.min - expected_bounds.min).length() <= 2e-5);
            assert!((bounds.max - expected_bounds.max).length() <= 2e-5);
        }
    }

    #[test]
    fn phase_reveal_forward_and_reverse_seek_keep_the_pen_on_the_retained_prefix() {
        let mut forward = session().unwrap();
        let mut seek = session().unwrap();
        let trace = trace_index(forward.session().frame());
        let GeometryRef::VectorPath(geometry) = forward
            .session()
            .frame()
            .render_geometry(trace)
            .unwrap()
            .clone()
        else {
            panic!("trace must retain one vector path");
        };
        let GeometryRef::VectorPath(seek_geometry) = seek
            .session()
            .frame()
            .render_geometry(trace)
            .unwrap()
            .clone()
        else {
            panic!("trace must retain one vector path");
        };
        let revision = forward.session().publication_context().scene_revision();
        let identities: Vec<_> = forward
            .session()
            .frame()
            .objects
            .iter()
            .map(|object| object.id)
            .collect();
        for phase in [
            0.0,
            0.065,
            0.13 - 1e-7,
            0.13,
            0.13 + 1e-7,
            0.3,
            0.4,
            0.6,
            0.73,
            0.91,
            1.0,
        ] {
            forward.session_mut().advance_to(phase * DURATION).unwrap();
            seek.seek(phase * DURATION).unwrap();
            assert_phase(&forward, phase);
            assert_phase(&seek, phase);
            for (index, (left, right)) in forward
                .session()
                .frame()
                .objects
                .iter()
                .zip(&seek.session().frame().objects)
                .enumerate()
            {
                assert_eq!(left.transform, right.transform);
                assert_eq!(left.style, right.style);
                assert_eq!(
                    forward.session().frame().reveal(index),
                    seek.session().frame().reveal(index)
                );
                assert_eq!(
                    forward.session().frame().is_present(index),
                    seek.session().frame().is_present(index)
                );
            }
            let Some(GeometryRef::VectorPath(current)) =
                forward.session().frame().render_geometry(trace)
            else {
                panic!("trace must retain one vector path");
            };
            assert!(Arc::ptr_eq(&geometry, current));
            assert_eq!(
                forward.session().publication_context().scene_revision(),
                revision
            );
        }
        for phase in [0.73, 0.0, 0.91, 0.13, 0.6, 0.3, 1.0, 0.065, 0.73] {
            seek.seek(phase * DURATION).unwrap();
            assert_phase(&seek, phase);
            let Some(GeometryRef::VectorPath(current)) =
                seek.session().frame().render_geometry(trace)
            else {
                panic!("trace must retain one vector path");
            };
            assert!(Arc::ptr_eq(&seek_geometry, current));
            assert_eq!(
                seek.session()
                    .frame()
                    .objects
                    .iter()
                    .map(|object| object.id)
                    .collect::<Vec<_>>(),
                identities
            );
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
