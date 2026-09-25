use super::*;
use crate::{RetainedFamilyExecutionDeltaEnvelope, RetainedTransientPresentationOccurrence};
use noon_runtime::{
    TransientAnchorSide, TransientPresentationOccurrence, TransientPresentationState,
};

fn frame() -> FrameState {
    // Execution storage order differs from painter order. A transported anchor
    // must resolve by ObjectId, not by copying an execution-local row index.
    let objects = [2.0, -3.5]
        .into_iter()
        .enumerate()
        .map(|(index, z_index)| FrameObjectState {
            id: ObjectId::new(40 + index as u64),
            z_index,
            content: ObjectContentRef::Geometry(GeometryRef::circle(0.5)),
            transform: Transform2D::IDENTITY,
            style: Style::default(),
            appearance: 1.0,
            text_bounds: None,
        })
        .collect();
    FrameState {
        time: 0.0,
        objects,
        presences: vec![true; 2],
        reveals: vec![1.0; 2],
        morphs: vec![0.0; 2],
        render_geometries: vec![None; 2],
        render_transforms: vec![None; 2],
        family_animations: vec![None; 2],
        family_animation_plan_indices: vec![None; 2],
    }
}

fn wire_round_trip(delta: &RetainedExecutionDeltaEnvelope) -> RetainedExecutionDeltaEnvelope {
    serde_json::from_str(&serde_json::to_string(delta).unwrap()).unwrap()
}

fn assert_layer(mirror: &RetainedExecutionFrameMirror, object: ObjectId, z_index: f64) {
    let index = mirror.frame_index_for_object(object).unwrap();
    assert_eq!(
        mirror.frame().unwrap().objects[index].z_index.to_bits(),
        z_index.to_bits()
    );
}

#[test]
fn nonzero_layers_survive_snapshot_storage_remapping_and_sparse_updates() {
    let mut frame = frame();
    let mut encoder = RetainedExecutionDeltaEncoder::new(71);
    let snapshot = encoder
        .encode_snapshot_indices(&frame, Camera2DState::default(), [1, 0])
        .unwrap();
    let mut mirror = RetainedExecutionFrameMirror::default();
    mirror.apply(wire_round_trip(&snapshot)).unwrap();
    assert_eq!(mirror.frame_index_for_object(ObjectId::new(40)), Some(1));
    assert_layer(&mirror, ObjectId::new(40), 2.0);
    assert_layer(&mirror, ObjectId::new(41), -3.5);

    // Property-only updates must not reset an unchanged anchor layer.
    frame.time = 0.5;
    frame.objects[0].appearance = 0.5;
    let delta = encoder
        .encode_incremental(
            &frame,
            &FrameChanges::objects(vec![0]),
            Camera2DState::default(),
        )
        .unwrap()
        .unwrap();
    assert_eq!(delta.objects.len(), 1);
    let (_, changes) = mirror.apply(wire_round_trip(&delta)).unwrap();
    assert_eq!(changes.object_indices(), &[1]);
    assert_layer(&mirror, ObjectId::new(40), 2.0);
    assert_layer(&mirror, ObjectId::new(41), -3.5);

    // A layer change and its painter splice share the same publication.
    frame.objects[0].z_index = -4.0;
    let delta = encoder
        .encode_incremental_with_painter_order(
            &frame,
            &FrameChanges::objects(vec![0]).with_painter_order(0..2),
            Camera2DState::default(),
            &[0, 1],
        )
        .unwrap()
        .unwrap();
    assert_eq!(delta.objects.len(), 1);
    mirror.apply(wire_round_trip(&delta)).unwrap();
    assert_eq!(mirror.painter_order(), &[1, 0]);
    assert_layer(&mirror, ObjectId::new(40), -4.0);
    assert_layer(&mirror, ObjectId::new(41), -3.5);
}

#[test]
fn signed_zero_layer_bits_are_not_replaced_by_a_decoder_default() {
    let mut frame = frame();
    frame.objects[0].z_index = -0.0;
    frame.objects[1].z_index = 0.0;
    let snapshot = RetainedExecutionDeltaEncoder::new(72)
        .encode_snapshot(&frame, Camera2DState::default())
        .unwrap();
    let mut mirror = RetainedExecutionFrameMirror::default();
    mirror.apply(wire_round_trip(&snapshot)).unwrap();
    assert_layer(&mirror, ObjectId::new(40), -0.0);
    assert_layer(&mirror, ObjectId::new(41), 0.0);
}

fn transient(frame: &FrameState, side: TransientAnchorSide) -> TransientPresentationOccurrence {
    let anchor = &frame.objects[0];
    TransientPresentationOccurrence::new(
        0,
        9,
        TransientPresentationState {
            z_index: anchor.z_index,
            content: anchor.content.clone(),
            text_bounds: None,
            transform: anchor.transform,
            style: anchor.style,
            appearance: anchor.appearance,
            presence: true,
            reveal: 1.0,
            morph: 0.0,
            render_geometry: None,
            render_transform: None,
        },
    )
    .with_anchor_side(side)
}

fn install(
    mirror: &RetainedExecutionFrameMirror,
    occurrence: &RetainedTransientPresentationOccurrence,
) -> TransientPresentationOccurrence {
    occurrence.install(mirror.frame_index_for_object(occurrence.anchor).unwrap() as u32)
}

#[test]
fn real_transient_packing_preserves_nonzero_anchor_layers_on_both_sides() {
    let frame = frame();
    for side in [TransientAnchorSide::Before, TransientAnchorSide::After] {
        let retained = RetainedExecutionDeltaEncoder::new(73)
            .encode_snapshot_indices(&frame, Camera2DState::default(), [1, 0])
            .unwrap();
        let mut envelope = RetainedFamilyExecutionDeltaEnvelope {
            retained,
            family_states: Vec::new(),
            family_plans: Vec::new(),
            resource_additions: None,
            transient_presentations: Vec::new(),
            selection_overlay: None,
            pointer_view: None,
        };
        envelope
            .replace_transient_presentations(&frame, &[transient(&frame, side)])
            .unwrap();
        let mut decoded: RetainedFamilyExecutionDeltaEnvelope =
            serde_json::from_str(&serde_json::to_string(&envelope).unwrap()).unwrap();
        let mut mirror = RetainedExecutionFrameMirror::default();
        mirror.apply(decoded.retained).unwrap();
        let installed = install(&mirror, &decoded.transient_presentations[0]);
        assert_eq!(installed.anchor_object_index(), 1);
        let packed = noon_render_wgpu::prepare_transient_presentation_rows(
            mirror.frame().unwrap(),
            mirror.painter_order(),
            &[installed],
        )
        .unwrap();
        assert_eq!(packed.stats.occurrences_packed, 1);
        assert_eq!(packed.stats.painter_positions_visited, 0);
        assert_eq!(packed.slots[0].anchor_side, side);

        // Do not relax the renderer's same-layer contract to fix the transport.
        decoded.transient_presentations[0].z_index = 3.0;
        let mismatched = install(&mirror, &decoded.transient_presentations[0]);
        assert!(matches!(
            noon_render_wgpu::prepare_transient_presentation_rows(
                mirror.frame().unwrap(),
                mirror.painter_order(),
                &[mismatched],
            ),
            Err(noon_render_wgpu::DerivedDisplayRenderError::ZIndexDiffersFromAnchor(9))
        ));
    }
}

#[test]
fn wire_requires_explicit_layers_and_rejects_the_previous_protocol() {
    let snapshot = RetainedExecutionDeltaEncoder::new(74)
        .encode_snapshot(&frame(), Camera2DState::default())
        .unwrap();
    let mut json = serde_json::to_value(&snapshot).unwrap();
    json["objects"][0]
        .as_object_mut()
        .unwrap()
        .remove("z_index");
    assert!(serde_json::from_value::<RetainedExecutionDeltaEnvelope>(json).is_err());
    let mut old = snapshot;
    old.protocol_version = 5;
    let mut mirror = RetainedExecutionFrameMirror::default();
    assert!(matches!(
        mirror.apply(old),
        Err(RetainedExecutionTransportError::UnsupportedVersion(5))
    ));
    assert!(mirror.frame().is_none());
}

#[test]
fn nonfinite_layers_fail_encoding_without_consuming_a_sequence() {
    for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut frame = frame();
        frame.objects[1].z_index = invalid;
        let mut encoder = RetainedExecutionDeltaEncoder::new(75);
        assert!(matches!(
            encoder.encode_snapshot(&frame, Camera2DState::default()),
            Err(RetainedExecutionTransportError::InvalidZIndex(_))
        ));
        frame.objects[1].z_index = -3.5;
        let valid = encoder
            .encode_snapshot(&frame, Camera2DState::default())
            .unwrap();
        assert_eq!(valid.sequence, 0);
        frame.objects[1].z_index = invalid;
        assert!(matches!(
            encoder.encode_incremental(
                &frame,
                &FrameChanges::objects(vec![1]),
                Camera2DState::default()
            ),
            Err(RetainedExecutionTransportError::InvalidZIndex(_))
        ));
        frame.objects[1].z_index = -3.5;
        let valid = encoder
            .encode_incremental(
                &frame,
                &FrameChanges::objects(vec![1]),
                Camera2DState::default(),
            )
            .unwrap()
            .unwrap();
        assert_eq!(valid.sequence, 1);
    }
}

#[test]
fn invalid_layers_reject_snapshot_and_incremental_publications_atomically() {
    for snapshot in [false, true] {
        for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let frame = frame();
            let mut encoder = RetainedExecutionDeltaEncoder::new(76);
            let initial = encoder
                .encode_snapshot(&frame, Camera2DState::default())
                .unwrap();
            let mut mirror = RetainedExecutionFrameMirror::default();
            mirror.apply(initial).unwrap();
            let before = mirror.frame().unwrap().clone();
            let order = mirror.painter_order().to_vec();
            let camera = mirror.camera();
            let sequence = mirror.applied_sequence();
            let mut delta = if snapshot {
                encoder
                    .encode_snapshot(&frame, Camera2DState::default())
                    .unwrap()
            } else {
                encoder
                    .encode_incremental(
                        &frame,
                        &FrameChanges::objects(vec![0, 1]),
                        Camera2DState::default(),
                    )
                    .unwrap()
                    .unwrap()
            };
            delta.time = 1.0;
            delta.objects[0].z_index = 4.0;
            delta.objects[0].appearance = 0.25;
            delta.objects[1].z_index = invalid;
            assert!(matches!(
                mirror.apply(delta),
                Err(RetainedExecutionTransportError::InvalidZIndex(_))
            ));
            assert_eq!(mirror.frame().unwrap(), &before);
            assert_eq!(mirror.painter_order(), order);
            assert_eq!(mirror.camera(), camera);
            assert_eq!(mirror.applied_sequence(), sequence);
        }
    }
}
