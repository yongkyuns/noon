//! Authored G4 workload: 1/100 animated glows in a fixed 600-object scene.
//! Paired with `web/python/examples/glow_profile.py`; no per-frame host callback.
use crate::{
    effects::{EffectUpdate, Pixels},
    AnimationOptions, Mobject, MobjectTarget, RateFunction, Scene, SceneExecution,
    SemanticAnimationCompositionKind, TransformToRequest,
};

pub const OBJECT_COUNT: usize = 600;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Property {
    Intensity,
    Radius,
}

impl std::str::FromStr for Property {
    type Err = String;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "intensity" => Ok(Self::Intensity),
            "radius" => Ok(Self::Radius),
            _ => Err("glow property must be intensity or radius".into()),
        }
    }
}

/// The frozen G4 layout and 3-pixel initial sigma, through ordinary Scene
/// authoring/target animation. Duration covers the declared >=10s trial horizon.
pub fn session(active: usize, property: Property, duration: f64) -> Result<SceneExecution, String> {
    if ![1, 100].contains(&active) {
        return Err("G4 active glow count must be 1 or 100".into());
    }
    if !duration.is_finite() || duration < 10.0 {
        return Err("G4 duration must be finite and at least 10 seconds".into());
    }
    let (scene, objects) = grid(active)?;
    let options = AnimationOptions::new()
        .run_time(duration)
        .rate_func(RateFunction::Linear);
    let mut execution = scene.into_execution().map_err(message)?;
    {
        let mut live = execution.live();
        let mut targets = Vec::with_capacity(active);
        for dot in &objects[..active] {
            let target = live.target_editor(dot).map_err(message)?;
            live.set_glow(
                &target,
                match property {
                    Property::Intensity => EffectUpdate::default().intensity(1.0),
                    Property::Radius => EffectUpdate::default().radius(Pixels(6.0)),
                },
            )
            .map_err(message)?;
            targets.push(target);
        }
        let requests = objects[..active]
            .iter()
            .zip(&targets)
            .map(|(dot, target)| TransformToRequest::new(dot, target, options).method_target())
            .collect::<Vec<_>>();
        let segment = live
            .declare_and_activate_transform_composition(
                SemanticAnimationCompositionKind::Parallel,
                &requests,
                AnimationOptions::new().rate_func(RateFunction::Linear),
                AnimationOptions::new(),
            )
            .map_err(message)?;
        // Finish ordinary CPU authoring, then use its existing replay interval.
        // This is setup, not the completed rendered cold pass required by M5.
        live.advance_segment_to(segment, duration)
            .map_err(message)?;
        live.complete_segment(segment).map_err(message)?;
    }
    execution.seek(0.0).map_err(message)?;
    Ok(execution)
}

pub(super) fn grid(active: usize) -> Result<(Scene, Vec<Mobject>), String> {
    let mut scene = Scene::new();
    let mut camera = scene.camera_frame().map_err(message)?;
    let scale = 6.0 / f64::from(noon_core::DEFAULT_FRAME_HEIGHT);
    camera.set_scale(scale, scale).map_err(message)?;
    let mut objects = Vec::with_capacity(OBJECT_COUNT);
    for index in 0..OBJECT_COUNT {
        let mut dot = scene.circle(0.05).map_err(message)?;
        dot.set_translation(
            -3.75 + (index % 30) as f64 * 0.25,
            -2.375 + (index / 30) as f64 * 0.25,
        )
        .map_err(message)?;
        dot.set_fill(1.0, 1.0, 1.0, 1.0).map_err(message)?;
        dot.disable_stroke().map_err(message)?;
        if index < active {
            dot.set_glow(EffectUpdate::default().radius(Pixels(3.0)).intensity(0.4))
                .map_err(message)?;
        }
        objects.push(dot);
    }
    scene
        .add_many(&objects.iter().map(MobjectTarget::from).collect::<Vec<_>>())
        .map_err(message)?;
    Ok((scene, objects))
}

fn message(error: impl std::fmt::Display) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn authored_glow_profiles_touch_only_active_effects_and_replay_exactly() {
        for active in [1, 100] {
            for property in [Property::Intensity, Property::Radius] {
                let mut forward = session(active, property, 10.0).unwrap();
                let mut replay = session(active, property, 10.0).unwrap();
                let before = forward.session().frame().objects.clone();
                assert_eq!(before.len(), OBJECT_COUNT + 1, "600 dots and camera");
                let expected = before
                    .iter()
                    .enumerate()
                    .filter_map(|(i, row)| row.effects.is_some().then_some(i))
                    .collect::<BTreeSet<_>>();
                assert_eq!(expected.len(), active);
                forward.session_mut().take_frame_changes();
                for time in [0.5, 5.0, 9.5] {
                    forward.session_mut().advance_to(time).unwrap();
                    let changes = forward.session_mut().take_frame_changes();
                    assert_eq!(
                        changes
                            .object_indices()
                            .iter()
                            .copied()
                            .collect::<BTreeSet<_>>(),
                        expected
                    );
                    replay.seek(time).unwrap();
                    for (index, (left, right)) in forward
                        .session()
                        .frame()
                        .objects
                        .iter()
                        .zip(&replay.session().frame().objects)
                        .enumerate()
                    {
                        assert_eq!(left.geometry(), before[index].geometry());
                        assert_eq!(left.transform, before[index].transform);
                        assert_eq!(left.style, before[index].style);
                        assert_eq!(left.effects, right.effects);
                        if let Some(stack) = &left.effects {
                            let glow = stack.definitions()[0].as_glow().unwrap();
                            let progress = time / 10.0;
                            match property {
                                Property::Intensity => {
                                    assert!(
                                        (glow.intensity() - (0.4 + 0.6 * progress)).abs() < 1e-12
                                    );
                                    assert_eq!(glow.radius(), Pixels(3.0).into());
                                }
                                Property::Radius => {
                                    assert_eq!(glow.intensity(), 0.4);
                                    let noon_core::EffectRadius::Pixels(radius) = glow.radius()
                                    else {
                                        panic!("G4 radius must retain backing-pixel units");
                                    };
                                    assert!((radius - (3.0 + 3.0 * progress)).abs() < 1e-12);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn invalid_g4_configuration_rejects_before_authoring() {
        for active in [0, 2, 601] {
            assert!(session(active, Property::Intensity, 10.0).is_err());
        }
        for duration in [0.0, 9.5, f64::NAN, f64::INFINITY] {
            assert!(session(1, Property::Intensity, duration).is_err());
        }
        assert!("position".parse::<Property>().is_err());
    }
}
