//! Image admission and failed late worker publication use the same atomic boundary.
use super::*;
use crate::{
    RetainedFamilyPlanTransport, RetainedResourceRetirements, SemanticExecutionPlayer,
    TransportImageResourceHandle, TransportImageSampling, TransportTextResourceHandle,
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
        SemanticExecutionPlayer::from_scene_execution(scene.into_execution().unwrap(), 4.0, 17)
            .unwrap();
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

fn family_delta(retained: RetainedExecutionDeltaEnvelope) -> RetainedFamilyExecutionDeltaEnvelope {
    RetainedFamilyExecutionDeltaEnvelope {
        retained,
        family_states: Vec::new(),
        family_plans: Vec::new(),
        resource_additions: None,
        resource_retirements: RetainedResourceRetirements::default(),
        transient_presentations: Vec::new(),
        selection_overlay: None,
        pointer_view: None,
    }
}

#[test]
fn rejected_family_delta_does_not_run_resident_preparation() {
    let (bytes, initial) = initial();
    let mut mirror = InstalledRetainedExecutionMirror::from_bundle_bytes(&bytes).unwrap();
    mirror.apply_family(family_delta(initial.clone())).unwrap();

    let mut rejected = initial.clone();
    rejected.snapshot = false;
    rejected.sequence = 1;
    rejected.objects.push(rejected.objects[0].clone());
    let before = mirror.frame().unwrap().clone();
    let mut resident_meshes = vec![0_u32];
    let result = crate::retained_execution_canvas::apply_family_then_resident_preparation(
        &mut mirror,
        family_delta(rejected),
        |_| {
            resident_meshes.push(1);
            Ok::<_, &'static str>(())
        },
    );

    assert!(result.is_err());
    assert_eq!(resident_meshes, [0]);
    assert_eq!(mirror.frame().unwrap(), &before);
}

#[test]
fn stale_family_delta_does_not_advance_resident_mesh_history() {
    let (bytes, initial) = initial();
    let mut mirror = InstalledRetainedExecutionMirror::from_bundle_bytes(&bytes).unwrap();
    mirror.apply_family(family_delta(initial.clone())).unwrap();
    let (bundle, handle) = addition();
    let stale = RetainedFamilyExecutionDeltaEnvelope {
        retained: initial,
        family_states: Vec::new(),
        family_plans: Vec::new(),
        resource_additions: Some(bundle),
        resource_retirements: RetainedResourceRetirements::default(),
        transient_presentations: Vec::new(),
        selection_overlay: None,
        pointer_view: None,
    };
    let mut resident_mesh_history = 7;
    let admission = crate::retained_execution_canvas::apply_family_then_resident_preparation(
        &mut mirror,
        stale,
        |_| {
            resident_mesh_history += 1;
            Ok::<_, &'static str>(())
        },
    )
    .unwrap();

    assert_eq!(
        admission.outcome,
        RetainedTransportApplyOutcome::DroppedStale
    );
    assert!(admission.resident_preparation.is_none());
    assert_eq!(resident_mesh_history, 7);
    assert_eq!(mirror.resources().image_count(), 1);
    assert!(mirror
        .resources()
        .resolve_image_handle(handle, TransportImageSampling::Nearest)
        .is_none());
}

#[test]
fn resident_preload_failure_does_not_reject_an_admitted_delta() {
    let (bytes, initial) = initial();
    let mut mirror = InstalledRetainedExecutionMirror::from_bundle_bytes(&bytes).unwrap();
    mirror.apply_family(family_delta(initial.clone())).unwrap();

    let mut update = initial;
    update.snapshot = false;
    update.sequence = 1;
    update.objects[0].transform.translation = noon_core::Vec2::new(3.0, 4.0);
    let admission = crate::retained_execution_canvas::apply_family_then_resident_preparation(
        &mut mirror,
        family_delta(update),
        |_| Err::<(), _>("forced preload failure"),
    )
    .unwrap();

    assert_eq!(admission.outcome, RetainedTransportApplyOutcome::Applied);
    assert_eq!(admission.changes.object_indices(), &[0]);
    assert_eq!(
        admission.resident_preparation,
        Some(Err("forced preload failure"))
    );
    assert_eq!(
        mirror.frame().unwrap().objects[0].transform.translation,
        noon_core::Vec2::new(3.0, 4.0)
    );
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
            resource_retirements: crate::RetainedResourceRetirements::default(),
            transient_presentations: vec![],
            selection_overlay: None,
            pointer_view: None,
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
        resource_retirements: crate::RetainedResourceRetirements::default(),
        transient_presentations: vec![],
        selection_overlay: None,
        pointer_view: None,
    };
    let (outcome, _) = mirror.apply_family(stale).unwrap();
    assert_eq!(outcome, RetainedTransportApplyOutcome::DroppedStale);
    assert_eq!(mirror.resources().image_count(), 1);
    assert!(mirror
        .resources()
        .resolve_image_handle(handle, TransportImageSampling::Nearest)
        .is_none());
}
#[test]
fn mixed_resource_churn_reclaims_retired_entries_without_dropping_shared_text_dependencies() {
    let (bytes, initial) = initial();
    let mut mirror = InstalledRetainedExecutionMirror::from_bundle_bytes(&bytes).unwrap();
    mirror
        .apply_family(RetainedFamilyExecutionDeltaEnvelope {
            retained: initial.clone(),
            family_states: Vec::new(),
            family_plans: Vec::new(),
            resource_additions: None,
            resource_retirements: RetainedResourceRetirements::default(),
            transient_presentations: Vec::new(),
            selection_overlay: None,
            pointer_view: None,
        })
        .unwrap();
    let initial_image = image_handle(&initial);

    let mut texts = noon::Scene::new();
    let first = texts
        .typst(noon::Typst::new("#line(length: 10pt) A"))
        .unwrap();
    let second = texts
        .typst(noon::Typst::new("#line(length: 10pt) B"))
        .unwrap();
    let first_text = first.state().unwrap().content.text().unwrap();
    let second_text = second.state().unwrap().content.text().unwrap();
    let source = texts.integration_store().borrow();

    let first_bundle = RetainedResourceBundle::capture(
        [first_text],
        source.text_resources(),
        source.geometry_resources(),
        source.font_resources(),
    )
    .unwrap();
    let first_transport = TransportTextResourceHandle::from_source_handle(first_text);
    let mut first_delta = initial.clone();
    first_delta.snapshot = false;
    first_delta.sequence = 1;
    first_delta.painter_order = None;
    first_delta.objects[0].content = TransportObjectContent::Text {
        text: first_transport,
    };
    mirror
        .apply_family(RetainedFamilyExecutionDeltaEnvelope {
            retained: first_delta,
            family_states: Vec::new(),
            family_plans: Vec::new(),
            resource_additions: Some(first_bundle),
            resource_retirements: RetainedResourceRetirements {
                images: vec![initial_image],
                texts: Vec::new(),
                geometries: Vec::new(),
            },
            transient_presentations: Vec::new(),
            selection_overlay: None,
            pointer_view: None,
        })
        .unwrap();
    assert_eq!(mirror.resources().image_count(), 0);
    let first_local = mirror
        .resources()
        .resolve_text_handle(first_transport)
        .unwrap();
    let first_resource = mirror.resources().texts().get(first_local).unwrap();
    let retired_vector = first_resource
        .vector_items
        .first()
        .map(|item| item.geometry);

    let second_bundle = RetainedResourceBundle::capture_additions_with_geometries(
        [second_text],
        [],
        source.text_resources(),
        source.geometry_resources(),
        source.font_resources(),
        &mirror.resources().inventory(),
    )
    .unwrap();
    // This mirrors the producer's resource inventory update after the first
    // successful publication; it makes the second text borrow the shared font.
    assert_eq!(second_bundle.font_count(), 0);
    let second_transport = TransportTextResourceHandle::from_source_handle(second_text);
    let mut second_delta = initial.clone();
    second_delta.snapshot = false;
    second_delta.sequence = 2;
    second_delta.painter_order = None;
    second_delta.objects[0].content = TransportObjectContent::Text {
        text: second_transport,
    };
    mirror
        .apply_family(RetainedFamilyExecutionDeltaEnvelope {
            retained: second_delta,
            family_states: Vec::new(),
            family_plans: Vec::new(),
            resource_additions: Some(second_bundle),
            resource_retirements: RetainedResourceRetirements {
                images: Vec::new(),
                texts: vec![first_transport],
                geometries: Vec::new(),
            },
            transient_presentations: Vec::new(),
            selection_overlay: None,
            pointer_view: None,
        })
        .unwrap();

    assert!(mirror
        .resources()
        .resolve_text_handle(first_transport)
        .is_none());
    let second_local = mirror
        .resources()
        .resolve_text_handle(second_transport)
        .unwrap();
    let second_resource = mirror.resources().texts().get(second_local).unwrap();
    for run in second_resource.runs.iter() {
        assert!(mirror.resources().fonts().get_for_face(&run.font).is_some());
    }
    if let Some(vector) = retired_vector {
        assert!(mirror.resources().geometries().get(vector).is_none());
    }

    let mut previous_image = None;
    for sequence in 3..67 {
        let (bundle, next_image) = addition();
        let mut delta = initial.clone();
        delta.snapshot = false;
        delta.sequence = sequence;
        delta.time = sequence as f64;
        delta.painter_order = None;
        delta.objects[0].content = TransportObjectContent::Image {
            image: next_image,
            sampling: TransportImageSampling::Linear,
        };
        mirror
            .apply_family(RetainedFamilyExecutionDeltaEnvelope {
                retained: delta,
                family_states: Vec::new(),
                family_plans: Vec::new(),
                resource_additions: Some(bundle),
                resource_retirements: RetainedResourceRetirements {
                    images: previous_image.into_iter().collect(),
                    texts: (sequence == 3)
                        .then_some(second_transport)
                        .into_iter()
                        .collect(),
                    geometries: Vec::new(),
                },
                transient_presentations: Vec::new(),
                selection_overlay: None,
                pointer_view: None,
            })
            .unwrap();
        assert_eq!(mirror.resources().image_count(), 1);
        assert!(mirror
            .resources()
            .resolve_text_handle(second_transport)
            .is_none());
        previous_image = Some(next_image);
    }
}

#[test]
fn live_image_retirement_is_rejected_before_mutating_the_mirror() {
    let (bytes, initial) = initial();
    let mut mirror = InstalledRetainedExecutionMirror::from_bundle_bytes(&bytes).unwrap();
    mirror.apply(initial.clone()).unwrap();
    let before = mirror.frame().unwrap().clone();
    let image = image_handle(&initial);
    let mut delta = initial.clone();
    delta.snapshot = false;
    delta.sequence = 1;
    delta.painter_order = None;
    let result = mirror.apply_family(RetainedFamilyExecutionDeltaEnvelope {
        retained: delta,
        family_states: Vec::new(),
        family_plans: Vec::new(),
        resource_additions: None,
        resource_retirements: RetainedResourceRetirements {
            images: vec![image],
            texts: Vec::new(),
            geometries: Vec::new(),
        },
        transient_presentations: Vec::new(),
        selection_overlay: None,
        pointer_view: None,
    });
    assert!(result.is_err());
    assert_eq!(mirror.frame().unwrap(), &before);
    assert_eq!(mirror.resources().image_count(), 1);
}

#[test]
fn snapshot_retirement_replaces_the_root_index() {
    let (bytes, initial) = initial();
    let mut mirror = InstalledRetainedExecutionMirror::from_bundle_bytes(&bytes).unwrap();
    mirror.apply(initial.clone()).unwrap();
    let old = image_handle(&initial);
    let (bundle, next) = addition();
    let mut replacement = initial;
    replacement.session += 1;
    replacement.sequence = 0;
    replacement.snapshot = true;
    replacement.objects[0].content = TransportObjectContent::Image {
        image: next,
        sampling: TransportImageSampling::Linear,
    };
    mirror
        .apply_family(RetainedFamilyExecutionDeltaEnvelope {
            retained: replacement,
            family_states: Vec::new(),
            family_plans: Vec::new(),
            resource_additions: Some(bundle),
            resource_retirements: RetainedResourceRetirements {
                images: vec![old],
                texts: Vec::new(),
                geometries: Vec::new(),
            },
            transient_presentations: Vec::new(),
            selection_overlay: None,
            pointer_view: None,
        })
        .unwrap();
    assert_eq!(mirror.resources().image_count(), 1);
    assert!(mirror
        .resources()
        .resolve_image_handle(old, TransportImageSampling::Linear)
        .is_none());
}

#[test]
fn shared_image_survives_retirement_of_only_one_snapshot_row() {
    let (bytes, initial) = initial();
    let mut mirror = InstalledRetainedExecutionMirror::from_bundle_bytes(&bytes).unwrap();
    mirror.apply(initial.clone()).unwrap();
    let image = image_handle(&initial);
    let mut two = initial.clone();
    two.session += 1;
    two.sequence = 0;
    two.snapshot = true;
    let mut second = two.objects[0].clone();
    second.slot = crate::TransportSlotId {
        slot: second.slot.slot + 1,
        generation: 0,
    };
    second.object = noon_core::ObjectId::new(u64::MAX - 1);
    second.order = 1;
    two.objects.push(second);
    mirror.apply(two.clone()).unwrap();
    let mut invalid = two;
    invalid.sequence = 1;
    invalid.objects.truncate(1);
    let before = mirror.frame().unwrap().clone();
    assert!(mirror
        .apply_family(RetainedFamilyExecutionDeltaEnvelope {
            retained: invalid,
            family_states: Vec::new(),
            family_plans: Vec::new(),
            resource_additions: None,
            resource_retirements: RetainedResourceRetirements {
                images: vec![image],
                texts: Vec::new(),
                geometries: Vec::new(),
            },
            transient_presentations: Vec::new(),
            selection_overlay: None,
            pointer_view: None,
        })
        .is_err());
    assert_eq!(mirror.frame().unwrap(), &before);
    assert_eq!(mirror.resources().image_count(), 1);
}

#[test]
fn stale_retirement_is_dropped_without_releasing_the_resource() {
    let (bytes, initial) = initial();
    let mut mirror = InstalledRetainedExecutionMirror::from_bundle_bytes(&bytes).unwrap();
    mirror.apply(initial.clone()).unwrap();
    let image = image_handle(&initial);
    let stale = RetainedFamilyExecutionDeltaEnvelope {
        retained: initial,
        family_states: Vec::new(),
        family_plans: Vec::new(),
        resource_additions: None,
        resource_retirements: RetainedResourceRetirements {
            images: vec![image],
            texts: Vec::new(),
            geometries: Vec::new(),
        },
        transient_presentations: Vec::new(),
        selection_overlay: None,
        pointer_view: None,
    };
    assert_eq!(
        mirror.apply_family(stale).unwrap().0,
        RetainedTransportApplyOutcome::DroppedStale
    );
    assert_eq!(mirror.resources().image_count(), 1);
}

fn image_fill_initial() -> (Vec<u8>, RetainedExecutionDeltaEnvelope) {
    let mut scene = noon::Scene::new();
    let source = scene
        .image_rgba8(2, 1, vec![255, 0, 0, 255, 0, 0, 255, 0])
        .unwrap();
    let paint = source
        .image_paint(
            noon::ImageMapping::default(),
            Some(noon::RasterImageSampling::Linear),
        )
        .unwrap();
    for x in [-1.5, 1.5] {
        let mut shape = scene.rectangle(2.0, 2.0).unwrap();
        shape.disable_stroke().unwrap();
        shape.set_translation(x, 0.0).unwrap();
        shape.set_fill_image(paint, Some(0.75)).unwrap();
        scene.add(&shape).unwrap();
    }
    let mut player =
        SemanticExecutionPlayer::from_scene_execution(scene.into_execution().unwrap(), 4.0, 17)
            .unwrap();
    let bytes = player.resource_bundle_bytes();
    let delta = serde_json::from_str(&player.initial_delta_json().unwrap()).unwrap();
    (bytes, delta)
}

#[test]
fn image_fill_bundle_remaps_shared_pixels_and_mapping_edits_do_not_retain_new_pixels() {
    let (bytes, initial) = image_fill_initial();
    let bundle = RetainedResourceBundle::decode_binary(&bytes).unwrap();
    assert_eq!((bundle.image_count(), bundle.image_bytes()), (1, 8));
    let mut mirror = InstalledRetainedExecutionMirror::from_bundle_bytes(&bytes).unwrap();
    mirror.apply_family(family_delta(initial.clone())).unwrap();
    let wire = initial.objects[0].fill_image.unwrap();
    let local = *mirror.frame().unwrap().objects[0]
        .fill_image
        .as_deref()
        .unwrap();
    assert_ne!(local.resource().arena, wire.image.arena);
    assert_eq!(
        mirror.frame().unwrap().objects[1]
            .fill_image
            .as_ref()
            .unwrap()
            .resource(),
        local.resource()
    );
    let pixels = mirror
        .resources()
        .images()
        .get(local.resource())
        .unwrap()
        .rgba8()
        .as_ptr();
    for sequence in 1..=64 {
        let mut update = initial.clone();
        update.snapshot = false;
        update.sequence = sequence;
        update.painter_order = None;
        update.objects.truncate(1);
        let paint = update.objects[0].fill_image.as_mut().unwrap();
        paint.mapping = noon::ImageMapping::new(
            noon::ImageFit::Stretch,
            [0.5; 2],
            noon::ImageAddressMode::Repeat,
            noon::ImageUvTransform::new([sequence as f64 / 64.0, -0.25], [0.5, 0.75], 0.3).unwrap(),
        )
        .unwrap();
        let json = serde_json::to_string(&update).unwrap();
        assert!(!json.contains("rgba8") && !json.contains("pixels"));
        let (_, changes) = mirror.apply_family(family_delta(update)).unwrap();
        assert_eq!(changes.object_indices(), &[0]);
        assert_eq!(mirror.resources().image_count(), 1);
        assert_eq!(
            mirror
                .resources()
                .images()
                .get(local.resource())
                .unwrap()
                .rgba8()
                .as_ptr(),
            pixels
        );
    }
}

#[test]
fn image_fill_unknown_resource_and_unsupported_profile_reject_before_publication() {
    let (bytes, initial) = image_fill_initial();
    let mut mirror = InstalledRetainedExecutionMirror::from_bundle_bytes(&bytes).unwrap();
    mirror.apply_family(family_delta(initial.clone())).unwrap();
    let before = mirror.frame().unwrap().clone();
    let mut valid = initial.clone();
    valid.snapshot = false;
    valid.sequence = 1;
    valid.painter_order = None;
    valid.objects[0].transform.translation.x += 0.125;
    for failure in 0..5 {
        let mut bad = valid.clone();
        match failure {
            0 => bad.objects[1].fill_image.as_mut().unwrap().image.arena += 1,
            1 => bad.objects[1].reveal = 0.5,
            2 => bad.objects[1].morph = 0.5,
            3 => {
                bad.objects[1].content = TransportObjectContent::Geometry {
                    geometry: noon::GeometryRef::line(Vec2::ZERO, Vec2::new(1.0, 1.0)),
                    resource: None,
                }
            }
            _ => {
                bad.objects[1].fill_image.as_mut().unwrap().mapping = noon::ImageMapping::new(
                    noon::ImageFit::Cover,
                    [0.5; 2],
                    noon::ImageAddressMode::Clamp,
                    noon::ImageUvTransform::new([f64::MAX, 0.0], [1.0; 2], 0.0).unwrap(),
                )
                .unwrap()
            }
        }
        assert!(
            mirror.apply_family(family_delta(bad)).is_err(),
            "case {failure}"
        );
        assert_eq!(mirror.frame().unwrap(), &before);
        assert_eq!(mirror.wire.applied_sequence(), Some(0));
        assert_eq!(mirror.resources().image_count(), 1);
    }
    assert_eq!(
        mirror.apply_family(family_delta(valid)).unwrap().0,
        RetainedTransportApplyOutcome::Applied
    );
    // Checked mapping decoding rejects invalid values before a receiver exists.
    let mut json = serde_json::to_value(initial).unwrap();
    json["objects"][0]["fill_image"]["mapping"]["transform"]["scale"][0] = serde_json::json!(0.0);
    assert!(serde_json::from_value::<RetainedExecutionDeltaEnvelope>(json).is_err());
}

#[test]
fn image_fill_retirement_waits_for_last_referring_shape_and_rejected_retirement_is_atomic() {
    let (bytes, initial) = image_fill_initial();
    let mut mirror = InstalledRetainedExecutionMirror::from_bundle_bytes(&bytes).unwrap();
    mirror.apply_family(family_delta(initial.clone())).unwrap();
    let image = initial.objects[0].fill_image.unwrap().image;
    let mut first = initial.clone();
    first.snapshot = false;
    first.sequence = 1;
    first.painter_order = None;
    first.objects.truncate(1);
    first.objects[0].fill_image = None;
    let mut invalid = family_delta(first.clone());
    invalid.resource_retirements.images.push(image);
    let before = mirror.frame().unwrap().clone();
    assert!(mirror.apply_family(invalid).is_err());
    assert_eq!(mirror.frame().unwrap(), &before);
    assert_eq!(mirror.wire.applied_sequence(), Some(0));
    mirror.apply_family(family_delta(first)).unwrap();
    assert_eq!(mirror.resources().image_count(), 1);
    let mut last = initial;
    last.snapshot = false;
    last.sequence = 2;
    last.painter_order = None;
    last.objects.remove(0);
    last.objects[0].fill_image = None;
    let mut last = family_delta(last);
    last.resource_retirements.images.push(image);
    mirror.apply_family(last).unwrap();
    assert_eq!(mirror.resources().image_count(), 0);
    assert!(mirror
        .frame()
        .unwrap()
        .objects
        .iter()
        .all(|row| row.fill_image.is_none()));
}
