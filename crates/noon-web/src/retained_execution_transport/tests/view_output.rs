use super::*;
use crate::{
    InstalledRetainedExecutionMirror, RetainedFamilyExecutionDeltaEnvelope, SemanticExecutionPlayer,
};

#[test]
fn ordinary_camera_bloom_round_trips_through_worker_publication_and_seek() {
    let execution = noon::example_scenes::camera_bloom::execution().unwrap();
    let mut player = SemanticExecutionPlayer::from_scene_execution(execution, 5.0, 71).unwrap();
    let mut mirror =
        InstalledRetainedExecutionMirror::from_bundle_bytes(&player.resource_bundle_bytes())
            .unwrap();
    let initial: RetainedFamilyExecutionDeltaEnvelope =
        serde_json::from_str(&player.initial_delta_json().unwrap()).unwrap();
    let view = initial.retained.primary_view.unwrap();
    assert_eq!(view.color_profile, noon_core::ViewColorProfile::LinearHdr);
    assert!(initial
        .retained
        .objects
        .iter()
        .any(|row| row.object == view.camera));
    mirror.apply_family(initial.clone()).unwrap();
    for time in [0.5, 1.5, 2.5, 2.75, 3.0, 4.0, 5.0, 2.75, 0.0] {
        let wire = player.seek_delta_json(time).unwrap().unwrap();
        mirror
            .apply_family(serde_json::from_str(&wire).unwrap())
            .unwrap();
        let publication = mirror.renderer_publication(&FrameChanges::all()).unwrap();
        assert_eq!(publication.primary_view(), Some(view.camera));
        assert_eq!(publication.color_profile(), view.color_profile);
        let bloom = publication.view_effects().unwrap().iter().next().unwrap();
        let TransportEffectDefinition::Bloom { intensity, .. } =
            TransportEffectState::try_from(&bloom).unwrap().definition
        else {
            panic!("camera Bloom lost");
        };
        let phase = if time <= 0.5 {
            0.0
        } else if time <= 2.5 {
            (time - 0.5) / 2.0
        } else if time <= 3.0 {
            1.0
        } else {
            (5.0 - time) / 2.0
        };
        assert!((intensity - phase * 3.0).abs() < 1e-12);
        let panel = publication
            .frame()
            .objects
            .iter()
            .find(|row| row.id != view.camera)
            .unwrap();
        assert!((panel.style.emission.strength() - phase * 7.0).abs() < 1e-12);
    }
}

#[test]
fn invalid_worker_view_metadata_and_bloom_preserve_frame_and_sequence() {
    let execution = noon::example_scenes::camera_bloom::execution().unwrap();
    let mut player = SemanticExecutionPlayer::from_scene_execution(execution, 5.0, 72).unwrap();
    let mut mirror =
        InstalledRetainedExecutionMirror::from_bundle_bytes(&player.resource_bundle_bytes())
            .unwrap();
    let initial: RetainedFamilyExecutionDeltaEnvelope =
        serde_json::from_str(&player.initial_delta_json().unwrap()).unwrap();
    mirror.apply_family(initial.clone()).unwrap();
    let before = mirror.frame().unwrap().clone();
    let mut next = initial;
    next.retained.sequence += 1;
    for invalid in 0..6 {
        let mut bad = next.clone();
        match invalid {
            0 => bad.retained.primary_view = None,
            1 => {
                bad.retained.primary_view.as_mut().unwrap().color_profile =
                    noon_core::ViewColorProfile::Compatibility
            }
            2 => bad.retained.primary_view.as_mut().unwrap().camera = ObjectId::new(u64::MAX),
            3 => {
                let camera = bad.retained.primary_view.unwrap().camera;
                let index = bad
                    .retained
                    .objects
                    .iter()
                    .position(|row| row.object == camera)
                    .unwrap();
                let bloom = bad.retained.objects[index].effects.remove(0);
                bad.retained
                    .objects
                    .iter_mut()
                    .find(|row| row.object != camera)
                    .unwrap()
                    .effects
                    .push(bloom);
            }
            5 => {
                let source = noon::example_scenes::spatial_surface::session().unwrap();
                let mut spatial =
                    SemanticExecutionPlayer::from_scene_execution(source, 2.0, 73).unwrap();
                let wire: RetainedFamilyExecutionDeltaEnvelope =
                    serde_json::from_str(&spatial.initial_delta_json().unwrap()).unwrap();
                let row = wire
                    .retained
                    .objects
                    .iter()
                    .find(|row| row.spatial.is_some())
                    .unwrap();
                bad.retained.objects[0].spatial = row.spatial.clone();
            }
            _ => {
                let camera = bad.retained.primary_view.unwrap().camera;
                let row = bad
                    .retained
                    .objects
                    .iter_mut()
                    .find(|row| row.object == camera)
                    .unwrap();
                let TransportEffectDefinition::Bloom { threshold, .. } =
                    &mut row.effects[0].definition
                else {
                    unreachable!()
                };
                *threshold = -1.0;
            }
        }
        assert!(mirror.apply_family(bad).is_err(), "invalid case {invalid}");
        assert_eq!(mirror.frame().unwrap(), &before);
    }
    mirror.apply_family(next).unwrap();
    assert_eq!(mirror.frame().unwrap(), &before);
}
