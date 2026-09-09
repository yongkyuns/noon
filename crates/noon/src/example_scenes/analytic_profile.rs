//! Analytic workloads for native/direct-WASM profiling, paired with `analytic_profile.py`.

use crate::{
    AnimationOptions, CompositionTimeMap, ExecutionSession, MobjectFamilyMember, RateFunction,
    Scene, SemanticAnimationCompositionKind, SemanticAnimationIntent, SemanticMutationTransaction,
    SemanticObjectTrackProperty, SemanticObjectTrackValues, SemanticVec3, TrackTiming,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layout {
    Fit,
    Fixed,
    Overdraw,
}

impl std::str::FromStr for Layout {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "fit" => Ok(Self::Fit),
            "fixed" => Ok(Self::Fixed),
            "overdraw" => Ok(Self::Overdraw),
            _ => Err("analytic layout must be fit, fixed, or overdraw".into()),
        }
    }
}

/// Build circles with an authored camera and one linear eight-unit position track.
/// Authoring/lowering happens once; subsequent advancement uses the shared runtime.
pub fn session(
    count: usize,
    layout: Layout,
    aspect: f64,
    duration: f64,
) -> Result<ExecutionSession, String> {
    if !(1..=100_000).contains(&count) {
        return Err("analytic object count must be between 1 and 100000".into());
    }
    if !aspect.is_finite() || aspect <= 0.0 {
        return Err("analytic aspect must be positive and finite".into());
    }
    if !duration.is_finite() || duration <= 0.0 {
        return Err("analytic duration must be positive and finite".into());
    }
    let columns = (count as f64 * aspect).sqrt().ceil();
    let rows = (count as f64 / columns).ceil();
    let camera_height = if layout == Layout::Fit { rows } else { 6.0 };
    let mut scene = Scene::new();
    let mut camera = scene.camera_frame()?;
    let scale = camera_height / noon_core::DEFAULT_FRAME_HEIGHT as f64;
    camera.set_scale(scale, scale)?;
    let mut circles = Vec::with_capacity(count);
    let mut driver_origin = None;
    for index in 0..count {
        let column = index as f64 % columns;
        let row = (index as f64 / columns).floor();
        let (radius, x, y, alpha) = match layout {
            Layout::Fit => (
                0.32,
                column - columns / 2.0 + 0.5,
                row - rows / 2.0 + 0.5,
                1.0,
            ),
            Layout::Fixed => {
                let camera_width = camera_height * aspect;
                (
                    0.06,
                    -camera_width / 2.0 + (column + 0.5) * camera_width / columns,
                    -camera_height / 2.0 + (row + 0.5) * camera_height / rows,
                    1.0,
                )
            }
            Layout::Overdraw => {
                let distance = 0.4 * ((index as f64 + 0.5) / count as f64).sqrt();
                let angle = index as f64 * std::f64::consts::PI * (3.0 - 5.0_f64.sqrt());
                (0.35, angle.cos() * distance, angle.sin() * distance, 0.16)
            }
        };
        let mut circle = scene.circle(radius)?;
        circle.set_translation(x, y)?;
        circle.set_color(0.27, 0.65, 0.96, 1.0)?;
        circle.set_fill(0.27, 0.65, 0.96, 1.0)?;
        circle.set_fill_opacity(alpha)?;
        circle.set_stroke_width(0.0)?;
        circles.push(circle);
        if index == 0 {
            driver_origin = Some((x, y));
        }
    }
    scene
        .add_many(
            &circles
                .iter()
                .map(MobjectFamilyMember::Mobject)
                .collect::<Vec<_>>(),
        )
        .map_err(|error| error.to_string())?;
    let circle = &circles[0];
    let (x, y) = driver_origin.expect("nonempty workload");
    let mut transaction = SemanticMutationTransaction::new();
    let track = transaction.create_object_property_track(
        circle.node_id(),
        SemanticObjectTrackProperty::Position,
        SemanticObjectTrackValues::Vec3 {
            from: SemanticVec3::new(x, y, 0.0),
            to: SemanticVec3::new(x + 8.0, y, 0.0),
        },
        TrackTiming::new(0.0, duration, RateFunction::Linear),
        CompositionTimeMap::identity(),
    );
    let committed = transaction
        .apply(&mut scene.store().borrow_mut())
        .map_err(|error| error.to_string())?;
    let root = scene.declare_animation(
        SemanticAnimationIntent::Composition {
            kind: SemanticAnimationCompositionKind::Parallel,
            children: vec![committed.resolve(track).expect("committed position track")],
        },
        AnimationOptions::new(),
    )?;
    scene.execution_session_with_animation_root(&root)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analytic_profile_advances_only_the_driver_and_agrees_with_seek() {
        for layout in [Layout::Fit, Layout::Fixed, Layout::Overdraw] {
            let mut forward = session(100, layout, 16.0 / 9.0, 60.0).unwrap();
            let mut direct_seek = session(100, layout, 16.0 / 9.0, 60.0).unwrap();
            forward.take_frame_changes();
            let before = forward.frame().objects.clone();
            assert_eq!(before.len(), 101, "100 circles and their semantic camera");
            forward.advance_to(1.0).unwrap();
            let changes = forward.take_frame_changes();
            assert_eq!(changes.object_indices().len(), 1);
            let driver = changes.object_indices()[0];
            assert!(
                (forward.frame().objects[driver].transform.translation.x
                    - before[driver].transform.translation.x
                    - 8.0 / 60.0)
                    .abs()
                    < 1e-5
            );
            direct_seek.seek(1.0).unwrap();
            assert_eq!(
                forward.frame().objects.len(),
                direct_seek.frame().objects.len()
            );
            for (left, right) in forward
                .frame()
                .objects
                .iter()
                .zip(&direct_seek.frame().objects)
            {
                assert_eq!(left.transform, right.transform);
                assert_eq!(left.style, right.style);
            }
            forward.advance_to(1.0).unwrap();
            assert!(forward.take_frame_changes().object_indices().is_empty());
            let expected_height = if layout == Layout::Fit { 8.0 } else { 6.0 };
            assert!((forward.camera().unwrap().height - expected_height).abs() < 1e-5);
        }
    }

    #[test]
    fn analytic_profile_rejects_invalid_configuration_before_authoring() {
        for count in [0, 100_001] {
            assert!(session(count, Layout::Fit, 1.0, 1.0).is_err());
        }
        for value in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            assert!(session(1, Layout::Fit, value, 1.0).is_err());
            assert!(session(1, Layout::Fit, 1.0, value).is_err());
        }
        assert!("unknown".parse::<Layout>().is_err());
    }
}
