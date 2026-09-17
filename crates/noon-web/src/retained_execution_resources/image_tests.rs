//! Image admission and failed late worker publication use the same atomic boundary.
use super::*;
use crate::{
    RetainedFamilyPlanTransport, SemanticExecutionPlayer, TransportImageResourceHandle,
    TransportImageSampling,
};
use noon_core::{
    FamilyAnimationLeafBinding, FontResourceArena, GeometryResourceArena, ObjectId,
    RasterImageResourceArena, SemanticNodeId, TextResourceArena, Vec2,
};

fn initial() -> (Vec<u8>, RetainedExecutionDeltaEnvelope) {
    let mut scene = noon::Scene::new();
    let image = scene
        .image(
            noon::ImageMobjectOptions::rgba8(2, 1, vec![255, 0, 0, 255, 0, 255, 0, 128]).unwrap(),
        )
        .unwrap();
    scene.add(&image).unwrap();
    let mut player =
        SemanticExecutionPlayer::from_session(scene.execution_session().unwrap(), 4.0, 17).unwrap();
    let bytes = player.resource_bundle_bytes();
    let frame = serde_json::from_str(&player.initial_delta_json().unwrap()).unwrap();
    (bytes, frame)
}
fn addition() -> (RetainedResourceBundle, TransportImageResourceHandle) {
    let mut images = RasterImageResourceArena::new();
    let handle = images.intern_rgba8(1, 1, vec![41, 73, 127, 255]).unwrap();
    let bundle = RetainedResourceBundle::capture(
        [],
        &TextResourceArena::new(),
        &GeometryResourceArena::new(),
        &FontResourceArena::new(),
    )
    .unwrap()
    .with_images([handle], &images)
    .unwrap();
    (bundle, handle.into())
}
fn image_handle(delta: &RetainedExecutionDeltaEnvelope) -> TransportImageResourceHandle {
    match delta.objects[0].content {
        TransportObjectContent::Image { image, .. } => image,
        _ => panic!("image snapshot"),
    }
}
#[test]
fn image_bundle_round_trip_remaps_provenance_and_keeps_pixels_out_of_deltas() {
    let (bytes, initial) = initial();
    let bundle = RetainedResourceBundle::decode_binary(&bytes).unwrap();
    assert_eq!(bundle.image_count(), 1);
    assert_eq!(bundle.image_bytes(), 8);
    let mut mirror = InstalledRetainedExecutionMirror::from_bundle_bytes(&bytes).unwrap();
    mirror.apply(initial.clone()).unwrap();
    let wire = image_handle(&initial);
    let local = mirror.frame().unwrap().objects[0].content.image().unwrap();
    assert_ne!(local.resource().arena, wire.arena);
    assert_eq!((local.width(), local.height()), (2, 1));
    assert_eq!(
        mirror
            .resources()
            .images()
            .get(local.resource())
            .unwrap()
            .rgba8(),
        &[255, 0, 0, 255, 0, 255, 0, 128]
    );
    let ptr = mirror
        .resources()
        .images()
        .get(local.resource())
        .unwrap()
        .rgba8()
        .as_ptr();
    for sequence in 1..=128 {
        let mut delta = initial.clone();
        delta.snapshot = false;
        delta.sequence = sequence;
        delta.painter_order = None;
        delta.objects[0].transform.translation = Vec2::new(sequence as f32, 1.0);
        delta.objects[0].content = TransportObjectContent::Image {
            image: wire,
            sampling: TransportImageSampling::Nearest,
        };
        let json = serde_json::to_string(&delta).unwrap();
        assert!(!json.contains("rgba8"));
        assert!(!json.contains("pixels"));
        let (_, changes) = mirror.apply(delta).unwrap();
        assert_eq!(changes.object_indices(), &[0]);
        let current = mirror.frame().unwrap().objects[0].content.image().unwrap();
        assert_eq!(current.resource(), local.resource());
        assert_eq!(current.sampling(), noon_core::RasterImageSampling::Nearest);
        assert_eq!(
            mirror
                .resources()
                .images()
                .get(current.resource())
                .unwrap()
                .rgba8()
                .as_ptr(),
            ptr
        );
    }
}

#[test]
fn image_addition_is_rolled_back_after_late_family_or_base_publication_failure() {
    for failure in ["family", "base"] {
        let (bytes, initial) = initial();
        let mut mirror = InstalledRetainedExecutionMirror::from_bundle_bytes(&bytes).unwrap();
        mirror.apply(initial.clone()).unwrap();
        let before = mirror.frame().unwrap().clone();
        let (bundle, handle) = addition();
        let mut replacement = initial;
        replacement.session += 1;
        replacement.sequence = 0;
        replacement.time = 1.0;
        replacement.objects[0].content = TransportObjectContent::Image {
            image: handle,
            sampling: TransportImageSampling::Linear,
        };
        let valid = RetainedFamilyExecutionDeltaEnvelope {
            retained: replacement,
            family_states: vec![],
            family_plans: vec![],
            resource_additions: Some(bundle),
            transient_presentations: vec![],
        };
        let mut invalid = valid.clone();
        if failure == "family" {
            invalid.family_plans = vec![RetainedFamilyPlanTransport::new(
                SemanticNodeId::new(7, 2),
                vec![FamilyAnimationLeafBinding::new(
                    SemanticNodeId::new(7, 2),
                    ObjectId::new(u64::MAX),
                )],
            )
            .unwrap()];
        } else {
            invalid
                .retained
                .objects
                .push(invalid.retained.objects[0].clone());
        }
        assert!(mirror.apply_family(invalid).is_err(), "{failure}");
        assert_eq!(mirror.frame().unwrap(), &before);
        assert_eq!(mirror.resources().image_count(), 1);
        assert!(mirror
            .resources()
            .resolve_image_handle(handle, TransportImageSampling::Linear)
            .is_none());
        assert!(matches!(mirror.wire.apply(valid.retained.clone()),
            Err(RetainedExecutionTransportError::UnknownImageResource(h)) if h == handle));
        // Exact retry is accepted, demonstrating neither wire identity nor
        // resource ownership/sequence advanced on the rejected publication.
        let (outcome, _) = mirror.apply_family(valid).unwrap();
        assert_eq!(outcome, RetainedTransportApplyOutcome::Applied);
        assert_eq!(mirror.resources().image_count(), 2);
        let local = mirror.frame().unwrap().objects[0].content.image().unwrap();
        assert_eq!((local.width(), local.height()), (1, 1));
        assert_eq!(
            mirror
                .resources()
                .images()
                .get(local.resource())
                .unwrap()
                .rgba8(),
            &[41, 73, 127, 255]
        );
    }
}

#[test]
fn stale_image_additions_are_dropped_without_retaining_pixels() {
    let (bytes, initial) = initial();
    let mut mirror = InstalledRetainedExecutionMirror::from_bundle_bytes(&bytes).unwrap();
    mirror.apply(initial.clone()).unwrap();
    let (bundle, handle) = addition();
    let stale = RetainedFamilyExecutionDeltaEnvelope {
        retained: initial,
        family_states: vec![],
        family_plans: vec![],
        resource_additions: Some(bundle),
        transient_presentations: vec![],
    };
    let (outcome, _) = mirror.apply_family(stale).unwrap();
    assert_eq!(outcome, RetainedTransportApplyOutcome::DroppedStale);
    assert_eq!(mirror.resources().image_count(), 1);
    assert!(mirror
        .resources()
        .resolve_image_handle(handle, TransportImageSampling::Nearest)
        .is_none());
}
