use crate::{
    AnimationOptions, Color, ExecutionSession, ImageAddressMode, ImageFit, ImageMapping,
    ImagePaintOptions, ImageUvTransform, Mobject, RasterImageSampling, RateFunction, Scene,
};

fn pixels() -> Vec<u8> {
    vec![
        255, 0, 0, 255, 0, 255, 0, 0, 0, 0, 255, 128, 255, 255, 255, 64,
    ]
}
fn paint() -> ImagePaintOptions {
    ImagePaintOptions::rgba8(2, 2, pixels()).unwrap()
}
fn mapping() -> ImageMapping {
    ImageMapping::new(
        ImageFit::Cover,
        [0.5; 2],
        ImageAddressMode::Clamp,
        ImageUvTransform::new([0.25, -0.5], [2.0, 0.5], 0.75).unwrap(),
    )
    .unwrap()
}
fn row<'a>(session: &'a ExecutionSession, source: &Mobject) -> &'a noon_runtime::FrameObjectState {
    let id = session.execution_object_id(source.node_id()).unwrap();
    session
        .frame()
        .objects
        .iter()
        .find(|row| row.id == id)
        .unwrap()
}
fn image(row: &noon_runtime::FrameObjectState) -> noon_core::ImagePaint {
    *row.fill_paint.as_ref().unwrap().as_image().unwrap()
}
fn options() -> AnimationOptions {
    AnimationOptions::new()
        .run_time(1.0)
        .rate_func(RateFunction::Linear)
}

#[test]
fn native_image_paint_options_are_inert_and_share_the_existing_bitmap_resource() {
    let mut scene = Scene::new();
    let before = scene
        .integration_store()
        .borrow()
        .raster_image_resources()
        .stats();
    let options = paint();
    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .raster_image_resources()
            .stats(),
        before
    );
    let bitmap = scene
        .image(crate::ImageMobjectOptions::rgba8(2, 2, pixels()).unwrap())
        .unwrap();
    let bitmap_handle = bitmap.state().unwrap().content.image().unwrap().resource();
    let mut shape = scene.circle(1.0).unwrap();
    let id = shape.node_id();
    shape.set_fill_image(options, Some(0.375)).unwrap();
    let state = shape.state().unwrap();
    assert_eq!(shape.node_id(), id);
    assert_eq!(
        state
            .style
            .fill
            .as_ref()
            .unwrap()
            .as_image()
            .unwrap()
            .resource(),
        bitmap_handle
    );
    assert_eq!(state.style.fill_opacity, 0.375);
    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .raster_image_resources()
            .len(),
        1
    );
    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .raster_image_resources()
            .stats()
            .pixel_bytes,
        16
    );
    assert!(shape.fill_color().is_err());
    assert_eq!(shape.fill_opacity().unwrap(), 0.375);
    let reused = ImagePaintOptions::from_image(&bitmap).unwrap();
    assert_eq!(reused.sampling(), RasterImageSampling::Bicubic);
    let mut other = scene.square(1.0).unwrap();
    other.set_fill_image(reused, None).unwrap();
    assert_eq!(
        other
            .state()
            .unwrap()
            .style
            .fill
            .as_ref()
            .unwrap()
            .as_image()
            .unwrap()
            .resource(),
        bitmap_handle
    );
    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .raster_image_resources()
            .len(),
        1
    );
}

#[test]
fn image_mapping_animation_uses_the_fill_channel_and_preserves_exact_completion_and_seek() {
    let mut scene = Scene::new();
    let mut source = scene.circle(0.5).unwrap();
    source.disable_stroke().unwrap();
    source.set_fill_image(paint(), Some(0.25)).unwrap();
    source.set_object_opacity(0.6).unwrap();
    scene.add(&source).unwrap();
    let resource = source
        .state()
        .unwrap()
        .style
        .fill
        .as_ref()
        .unwrap()
        .as_image()
        .unwrap()
        .resource();
    let mut target = source.target_editor().unwrap();
    target
        .set_fill_image_mapping(mapping(), Some(0.75))
        .unwrap();
    let animation = scene
        .declare_transform_to(&source, &target, options())
        .unwrap();
    let mut session = scene.execution_session().unwrap();
    let segment = scene.live(&mut session).play_animation(&animation).unwrap();
    let stats = scene
        .integration_store()
        .borrow()
        .raster_image_resources()
        .stats();
    scene
        .live(&mut session)
        .advance_segment_to(segment, 0.5)
        .unwrap();
    let middle = row(&session, &source).clone();
    let sampled = image(&middle);
    assert_eq!(sampled.resource(), resource);
    assert_eq!(sampled.mapping().transform().translation(), [0.125, -0.25]);
    assert_eq!(sampled.mapping().transform().scale(), [1.5, 0.75]);
    assert_eq!(sampled.mapping().transform().rotation(), 0.375);
    assert_eq!(middle.style.fill, Some(Color::rgba(1.0, 1.0, 1.0, 0.5)));
    assert_eq!(middle.style.opacity, 0.6);
    let captured = crate::effective_capture::target_style_from_effective(
        &source.state().unwrap().style,
        middle.style,
        middle.fill_paint.as_ref(),
    )
    .unwrap();
    assert_eq!(
        captured.fill,
        Some(middle.fill_paint.as_ref().unwrap().as_ref().clone())
    );
    scene
        .live(&mut session)
        .advance_segment_to(segment, 1.0)
        .unwrap();
    scene.live(&mut session).complete_segment(segment).unwrap();
    let endpoint = source.state().unwrap();
    assert_eq!(
        endpoint
            .style
            .fill
            .as_ref()
            .unwrap()
            .as_image()
            .unwrap()
            .mapping(),
        mapping()
    );
    assert_eq!(endpoint.style.fill_opacity, 0.75);
    for time in [0.5, 0.0, 0.75, 1.0, 0.25, 0.5] {
        session.seek(time).unwrap();
        if time == 0.5 {
            assert_eq!(row(&session, &source).fill_paint, middle.fill_paint);
        }
        assert_eq!(image(row(&session, &source)).resource(), resource);
        assert_eq!(
            scene
                .integration_store()
                .borrow()
                .raster_image_resources()
                .stats(),
            stats
        );
    }
}

#[test]
fn live_image_mapping_is_local_and_does_not_readmit_pixels_or_touch_spatial_state() {
    let mut scene = Scene::new();
    let mut source = scene.square(1.0).unwrap();
    source.disable_stroke().unwrap();
    source.set_fill_image(paint(), Some(1.0)).unwrap();
    scene.add(&source).unwrap();
    for _ in 0..1024 {
        let other = scene.circle(0.1).unwrap();
        scene.add(&other).unwrap();
    }
    let mut session = scene.execution_session().unwrap();
    session.take_frame_changes();
    let original = image(row(&session, &source));
    let stats = scene
        .integration_store()
        .borrow()
        .raster_image_resources()
        .stats();
    for turn in 0..32 {
        let next = if turn % 2 == 0 {
            mapping()
        } else {
            ImageMapping::default()
        };
        scene
            .live(&mut session)
            .set_fill_image_mapping(&source, next, None)
            .unwrap();
        assert_eq!(session.take_frame_changes().object_indices(), &[0]);
        session.query_viewport(crate::Rect::new(
            crate::Vec2::new(-2.0, -2.0),
            crate::Vec2::new(2.0, 2.0),
        ));
        assert_eq!(session.last_spatial_update_stats().full_rebuilds, 0);
        assert_eq!(session.last_spatial_update_stats().leaves_upserted, 0);
        assert_eq!(
            image(row(&session, &source)).resource(),
            original.resource()
        );
        assert_eq!(
            scene
                .integration_store()
                .borrow()
                .raster_image_resources()
                .stats(),
            stats
        );
    }
    let context = session.publication_context();
    scene
        .live(&mut session)
        .set_fill_image_mapping(&source, ImageMapping::default(), None)
        .unwrap();
    assert!(session.take_frame_changes().is_empty());
    assert_eq!(
        session.publication_context().frame_epoch(),
        context.frame_epoch()
    );
}

#[test]
fn image_admission_and_compiler_profile_rejections_preserve_the_live_publication() {
    let mut scene = Scene::new();
    let source = scene.circle(1.0).unwrap();
    let line = scene.line((0.0, 0.0), (1.0, 0.0)).unwrap();
    scene.add(&source).unwrap();
    scene.add(&line).unwrap();
    let mut session = scene.execution_session().unwrap();
    let before = session.frame().clone();
    let context = session.publication_context();
    let resources = scene
        .integration_store()
        .borrow()
        .raster_image_resources()
        .stats();
    assert!(scene
        .live(&mut session)
        .set_fill_image(&source, paint(), Some(f64::NAN))
        .is_err());
    assert!(scene
        .live(&mut session)
        .set_fill_image(&line, paint(), None)
        .is_err());
    assert_eq!(session.frame(), &before);
    assert_eq!(session.publication_context(), context);
    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .raster_image_resources()
            .stats(),
        resources
    );
    scene
        .live(&mut session)
        .set_fill_image(&source, paint(), Some(0.625))
        .unwrap();
    let handle = image(row(&session, &source)).resource();
    assert_eq!(
        session
            .raster_image_resources()
            .get(handle)
            .unwrap()
            .rgba8(),
        pixels()
    );
    assert_eq!(row(&session, &source).style.fill.unwrap().alpha, 0.625);
}

#[test]
fn incompatible_image_mapping_or_pixel_changes_roll_back_animation_activation() {
    for case in 0..4 {
        let mut scene = Scene::new();
        let mut source = scene.circle(0.5).unwrap();
        source.set_fill_image(paint(), None).unwrap();
        scene.add(&source).unwrap();
        let mut target = source.target_editor().unwrap();
        match case {
            0 => {
                target
                    .set_fill_image_mapping(
                        ImageMapping::new(
                            ImageFit::Contain,
                            [0.5; 2],
                            ImageAddressMode::Clamp,
                            ImageUvTransform::default(),
                        )
                        .unwrap(),
                        None,
                    )
                    .unwrap();
            }
            1 => {
                target
                    .set_fill_image_mapping(
                        ImageMapping::new(
                            ImageFit::Cover,
                            [0.5; 2],
                            ImageAddressMode::Clamp,
                            ImageUvTransform::new([0.0; 2], [-1.0, 1.0], 0.0).unwrap(),
                        )
                        .unwrap(),
                        None,
                    )
                    .unwrap();
            }
            2 => {
                target
                    .set_fill_image(
                        ImagePaintOptions::rgba8(1, 1, vec![0, 255, 0, 255]).unwrap(),
                        None,
                    )
                    .unwrap();
            }
            _ => {
                let mut changed = paint();
                changed.set_sampling(RasterImageSampling::Bicubic);
                target.set_fill_image(changed, None).unwrap();
            }
        }
        let animation = scene
            .declare_transform_to(&source, &target, options())
            .unwrap();
        let mut session = scene.execution_session().unwrap();
        let before = session.frame().clone();
        let context = session.publication_context();
        assert!(
            scene.live(&mut session).play_animation(&animation).is_err(),
            "case {case}"
        );
        assert_eq!(session.frame(), &before);
        assert_eq!(session.publication_context(), context);
    }
}

#[test]
fn image_resource_replacement_keeps_an_older_execution_snapshot_usable() {
    let mut scene = Scene::new();
    let mut source = scene.circle(1.0).unwrap();
    source.set_fill_image(paint(), None).unwrap();
    scene.add(&source).unwrap();
    let old = scene.execution_session().unwrap();
    let old_handle = image(row(&old, &source)).resource();
    let mut current = scene.execution_session().unwrap();
    let next = ImagePaintOptions::rgba8(1, 1, vec![0, 255, 0, 128]).unwrap();
    scene
        .live(&mut current)
        .set_fill_image(&source, next, None)
        .unwrap();
    let next_handle = image(row(&current, &source)).resource();
    assert_ne!(old_handle, next_handle);
    assert!(scene
        .integration_store()
        .borrow()
        .raster_image_resources()
        .get(old_handle)
        .is_none());
    assert_eq!(
        old.raster_image_resources()
            .get(old_handle)
            .unwrap()
            .rgba8(),
        pixels()
    );
    assert_eq!(
        current
            .raster_image_resources()
            .get(next_handle)
            .unwrap()
            .rgba8(),
        &[0, 255, 0, 128]
    );
    let mut captured = scene.live(&mut current).target_editor(&source).unwrap();
    captured.set_fill_image_mapping(mapping(), None).unwrap();
    assert_eq!(
        captured
            .state()
            .unwrap()
            .style
            .fill
            .as_ref()
            .unwrap()
            .as_image()
            .unwrap()
            .resource(),
        next_handle
    );
}
