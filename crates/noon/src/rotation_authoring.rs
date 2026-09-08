//! Exact centered 2D procedural rotation over the shared angular-path intent.
use noon_core::Bounds2D64;

/// Python may eagerly capture a point or defer its center/edge choice. Rust owns
/// resolution and support checks against coherent authored/effective geometry.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum ManimRotationPivot {
    #[default]
    Center,
    Point(f64, f64),
    Edge(f64, f64),
}

impl ManimRotationPivot {
    pub(crate) fn validate(
        self,
        bounds: Option<Bounds2D64>,
        origin: (f64, f64),
    ) -> Result<(), String> {
        let center = bounds.map_or(origin, |b| {
            ((b.min_x + b.max_x) * 0.5, (b.min_y + b.max_y) * 0.5)
        });
        // Layout and authored values are f64; runtime transforms are f32. Compare
        // at the rendering precision so an unchanged captured pivot stays valid.
        let close = |a: f64, b: f64| {
            a.is_finite()
                && b.is_finite()
                && (a - b).abs() <= 4.0 * f64::from(f32::EPSILON) * a.abs().max(b.abs()).max(1.0)
        };
        if !close(center.0, origin.0) || !close(center.1, origin.1) {
            return Err(
                "procedural rotation requires geometry centered on its transform origin".into(),
            );
        }
        let point = match self {
            Self::Center => center,
            Self::Point(x, y) => (x, y),
            Self::Edge(x, y) => {
                if !x.is_finite() || !y.is_finite() {
                    return Err("rotation edge direction must be finite".into());
                }
                bounds.map_or(center, |b| {
                    (
                        if x < 0.0 {
                            b.min_x
                        } else if x > 0.0 {
                            b.max_x
                        } else {
                            center.0
                        },
                        if y < 0.0 {
                            b.min_y
                        } else if y > 0.0 {
                            b.max_y
                        } else {
                            center.1
                        },
                    )
                })
            }
        };
        if !close(point.0, center.0) || !close(point.1, center.1) {
            return Err("rotation about an external point/edge requires curved translation and is not yet supported".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        AnimationCompositionRequest as Request, AnimationOptions, RateFunction, Scene,
        SemanticAnimationCompositionKind, TransformToRequest,
    };

    #[test]
    fn procedural_rotation_rejects_stale_external_edge_and_offset_pivots_atomically() {
        let scene = Scene::new();
        let mut square = scene.square(2.0).unwrap();
        let captured = square.center().unwrap();
        square.shift(2.0, 1.0).unwrap();
        let line = scene.line((0.0, 0.0), (2.0, 0.0)).unwrap();
        let mut session = scene.execution_session().unwrap();
        session.take_frame_changes();
        let before = session.publication_context();
        let nodes = square.store().borrow().len();
        for (target, pivot) in [
            (&square, ManimRotationPivot::Point(captured.0, captured.1)),
            (&square, ManimRotationPivot::Edge(1.0, 0.0)),
            (&square, ManimRotationPivot::Point(f64::NAN, 1.0)),
            (&line, ManimRotationPivot::Center),
        ] {
            let result = scene.live(&mut session).declare_and_activate_composition(
                &Request::ManimRotate {
                    target,
                    angle: 1.0,
                    pivot,
                    options: AnimationOptions::new(),
                },
                AnimationOptions::new(),
            );
            assert!(result.is_err(), "unsupported pivot accepted");
            assert_eq!(session.publication_context(), before);
            assert_eq!(square.store().borrow().len(), nodes);
            assert!(session.frame().objects.is_empty());
            assert!(session.take_frame_changes().is_empty());
        }
    }

    #[test]
    fn procedural_rotation_captures_effective_center_and_holds_it_through_samples() {
        let mut scene = Scene::new();
        let mut square = scene.square(2.0).unwrap();
        square.set_translation(0.1, -0.2).unwrap();
        let eager = square.center().unwrap();
        scene.add(&square).unwrap();
        let mut session = scene.execution_session().unwrap();
        let mut live = scene.live(&mut session);
        for pivot in [
            ManimRotationPivot::Point(eager.0, eager.1),
            ManimRotationPivot::Center,
            ManimRotationPivot::Edge(0.0, 0.0),
        ] {
            let start = live.effective(&square).unwrap().transform;
            let segment = live
                .declare_and_activate_composition(
                    &Request::ManimRotate {
                        target: &square,
                        angle: std::f64::consts::TAU,
                        pivot,
                        options: AnimationOptions::new()
                            .run_time(2.0)
                            .rate_func(RateFunction::Linear),
                    },
                    AnimationOptions::new(),
                )
                .unwrap();
            live.advance_segment_to(segment, segment.start_time() + 1.0)
                .unwrap();
            let middle = live.effective(&square).unwrap().transform;
            assert_eq!(middle.translation, start.translation);
            assert!((middle.rotation - start.rotation - std::f32::consts::PI).abs() < 2e-6);
            live.advance_segment_to(segment, segment.end_time())
                .unwrap();
            live.complete_segment(segment).unwrap();
            assert_eq!(
                live.effective(&square).unwrap().transform.translation,
                start.translation
            );
        }
    }

    #[test]
    fn procedural_rotation_conflicts_with_motion_on_the_same_target_in_either_order() {
        for reverse in [false, true] {
            let mut scene = Scene::new();
            let square = scene.square(2.0).unwrap();
            let mut target = square.target_editor().unwrap();
            target.set_translation(2.0, 0.0).unwrap();
            scene.add(&square).unwrap();
            let mut session = scene.execution_session().unwrap();
            session.take_frame_changes();
            let before = session.publication_context();
            let before_frame = session.frame().clone();
            let nodes = square.store().borrow().len();
            let mut children = vec![
                Request::ManimRotate {
                    target: &square,
                    angle: 1.0,
                    pivot: ManimRotationPivot::Center,
                    options: AnimationOptions::new(),
                },
                Request::TransformTo(TransformToRequest::new(
                    &square,
                    &target,
                    AnimationOptions::new(),
                )),
            ];
            if reverse {
                children.reverse();
            }
            let result = scene.live(&mut session).declare_and_activate_composition(
                &Request::Composition {
                    kind: SemanticAnimationCompositionKind::Parallel,
                    children,
                    options: AnimationOptions::new(),
                },
                AnimationOptions::new(),
            );
            assert!(
                matches!(
                    &result,
                    Err(crate::LiveSessionError::Activation(
                        crate::ExecutionSessionAnimationError::PreparedAnimation(
                            noon_compile::PreparedSemanticAnimationLoweringError::MultipleDrivers { .. }
                        )
                    ))
                ),
                "unexpected rejection: {result:?}"
            );
            assert_eq!(session.publication_context(), before);
            assert_eq!(square.store().borrow().len(), nodes);
            assert_eq!(session.frame(), &before_frame);
            assert!(session.take_frame_changes().is_empty());
        }
    }
}
