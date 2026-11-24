use super::*;
fn spatial() -> Box<noon_compile::CompiledSpatialState> {
    Box::new(noon_compile::CompiledSpatialState {
        world: noon_core::SemanticWorldTransform3D::IDENTITY,
        camera_projection: None,
        camera_profile: None,
        camera_motions: None,
        material: noon_core::SemanticSpatialMaterial::Unlit,
        point_light: false,
        composition_domain: noon_core::SemanticSpatialCompositionDomain::World,
        draw_kind: noon_compile::CompiledSpatialDrawKind::Planar,
        spatial_anchor_family: None,
        fixed_orientation_center: None,
        cairo_path_appearance: None,
    })
}
fn glass() -> Arc<noon_compile::CompiledEffectStack> {
    effect_stack(&noon_compile::CompiledEffect {
        attachment: SemanticNodeId::new(97, 4),
        definition: noon_core::BackdropBlur::new(
            EffectUpdate::default().radius(noon_core::Pixels(2.0)),
        )
        .unwrap()
        .into(),
    })
}
#[test]
fn worker_rejects_an_earlier_spatial_prefix_atomically_and_accepts_a_valid_retry() {
    let mut frame = mixed_frame();
    frame.objects[0].effects = Some(glass());
    frame.objects[1].content = ObjectContentRef::Geometry(GeometryRef::circle(0.4));
    frame.objects[1].spatial = Some(spatial());
    let mut encoder = RetainedExecutionDeltaEncoder::new(291);
    let good = encoder
        .encode_snapshot_with_context(
            &frame,
            Camera2DState::default(),
            PublicationContext::default(),
        )
        .unwrap();
    let mut bad = good.clone();
    bad.objects.swap(0, 1);
    for (rank, row) in bad.objects.iter_mut().enumerate() {
        row.order = rank as u32;
    }
    let mut mirror = test_mirror();
    assert!(
        matches!(mirror.apply(bad),Err(RetainedExecutionTransportError::InvalidComposedSource(noon_compile::CompiledComposedSourceError {owner,kind:noon_compile::CompiledComposedSourceErrorKind::UnsupportedBackdropInput(input)})) if owner==frame.objects[0].id&&input==frame.objects[1].id)
    );
    assert!(mirror.frame().is_none());
    assert_eq!(mirror.applied_sequence(), None);
    mirror.apply(good).unwrap();
    assert_eq!(mirror.frame().unwrap(), &frame);
    let before = mirror.frame().unwrap().clone();
    let index = mirror.composed_dependencies.clone();
    frame.objects[1].z_index = -1.0;
    let bad = encoder
        .encode_incremental_with_painter_order_and_context(
            &frame,
            &FrameChanges::objects(vec![1]).with_painter_order(0..2),
            Camera2DState::default(),
            PublicationContext::default(),
            &[1, 0],
        )
        .unwrap()
        .unwrap();
    assert!(matches!(
        mirror.apply(bad.clone()),
        Err(RetainedExecutionTransportError::InvalidComposedSource(_))
    ));
    assert_eq!(mirror.frame().unwrap(), &before);
    assert_eq!(mirror.composed_dependencies, index);
    assert_eq!(mirror.applied_sequence(), Some(0));
    let mut valid = bad;
    let mut panel = encoder.transport_object(&frame, 0, false).unwrap();
    panel.effects.clear();
    panel.visibility = None;
    valid.objects.push(panel);
    mirror.apply(valid).unwrap();
    assert!(mirror.frame().unwrap().objects[0].effects.is_none());
    assert_eq!(mirror.applied_sequence(), Some(1));
}
#[test]
fn worker_capability_metadata_tracks_spatial_rows_before_later_backdrop_enrollment() {
    let mut frame = mixed_frame();
    frame.objects[0].spatial = Some(spatial());
    let mut encoder = RetainedExecutionDeltaEncoder::new(292);
    let snapshot = encoder
        .encode_snapshot_with_context(
            &frame,
            Camera2DState::default(),
            PublicationContext::default(),
        )
        .unwrap();
    let mut mirror = test_mirror();
    mirror.apply(snapshot).unwrap();
    assert!(mirror.composed_dependencies.is_empty());
    frame.objects[1].effects = Some(glass());
    let bad = encoder
        .encode_incremental_with_context(
            &frame,
            &FrameChanges::objects(vec![1]),
            Camera2DState::default(),
            PublicationContext::default(),
        )
        .unwrap()
        .unwrap();
    let before = mirror.frame().unwrap().clone();
    let index = mirror.composed_dependencies.clone();
    assert!(matches!(
        mirror.apply(bad.clone()),
        Err(RetainedExecutionTransportError::InvalidComposedSource(_))
    ));
    assert_eq!(mirror.frame().unwrap(), &before);
    assert_eq!(mirror.composed_dependencies, index);
    assert_eq!(mirror.applied_sequence(), Some(0));
    // One coherent correction removes the incompatible spatial role and enrolls
    // the backdrop, without publishing an intermediate invalid frame.
    frame.objects[0].spatial = None;
    let mut good = bad;
    good.objects
        .push(encoder.transport_object(&frame, 0, false).unwrap());
    mirror.apply(good).unwrap();
    assert_eq!(mirror.frame().unwrap(), &frame);
    assert_eq!(mirror.applied_sequence(), Some(1));
}
