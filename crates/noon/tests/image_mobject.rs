use noon::{
    AnimationOptions, ImageMobjectOptions, LayoutAnchor, LayoutDimension, RasterImageSampling,
    RateFunction, Scene,
};
use noon_core::SemanticFadeDirection;
use std::{error::Error, sync::Arc};
type TestResult = Result<(), Box<dyn Error>>;

fn options() -> ImageMobjectOptions {
    ImageMobjectOptions::rgba8(
        2,
        2,
        vec![
            255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 0, 255, 255, 255, 255,
        ],
    )
    .unwrap()
}

#[test]
fn image_sizing_layout_copy_and_compilation_share_pixels() -> TestResult {
    let mut scene = Scene::new();
    let a = scene.image(options())?;
    assert!((a.height()? - 2.0 / 1080.0 * 8.0).abs() < 1e-12);
    assert_eq!(a.image_dimensions()?, (2, 2));
    assert_eq!(a.image_sampling()?, RasterImageSampling::Bicubic);
    let mut b = a.copy_handle()?;
    b.shift(3.0, 0.0)?;
    LayoutAnchor::from(&b).rescale_to_fit(4.0, LayoutDimension::Height, false)?;
    assert_eq!(
        (b.width()?, b.height()?, b.center()?),
        (4.0, 4.0, (3.0, 0.0))
    );
    assert_ne!(a.node_id(), b.node_id());
    let handle = a.state()?.content.image().unwrap().resource();
    assert_eq!(b.state()?.content.image().unwrap().resource(), handle);
    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .raster_image_resources()
            .len(),
        1
    );
    scene.add_many(&[(&a).into(), (&b).into()])?;
    let mut session = scene.execution_session()?;
    assert_eq!(session.frame().objects.len(), 2);
    assert_eq!(
        session.frame().objects[0]
            .content
            .image()
            .unwrap()
            .resource(),
        handle
    );
    let store = scene.integration_store().borrow();
    let authored = store.raster_image_resources().get(handle).unwrap();
    let compiled = session.raster_image_resources().get(handle).unwrap();
    assert_eq!(authored.rgba8().as_ptr(), compiled.rgba8().as_ptr());
    drop(store);
    assert_eq!(scene.live(&mut session).effective_layout(&b)?.width, 4.0);
    Ok(())
}

#[test]
fn invalid_image_configuration_does_not_allocate_semantic_identity_or_pixels() -> TestResult {
    let mut scene = Scene::new();
    let before = (scene.revision(), scene.integration_store().borrow().len());
    assert!(ImageMobjectOptions::rgba8(2, 2, vec![0; 15]).is_err());
    let mut input = options();
    assert!(input.set_height(f64::NAN).is_err());
    assert!(input.set_opacity(2.0).is_err());
    input.set_scale_to_resolution(f64::from(f32::from_bits(1)))?;
    // Display-size overflow is rejected before admitting pixels.
    let failed = scene.image(input);
    assert!(failed.is_err());
    assert_eq!(
        (scene.revision(), scene.integration_store().borrow().len()),
        before
    );
    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .raster_image_resources()
            .len(),
        0
    );
    Ok(())
}

#[test]
fn image_fades_transform_and_detached_live_creation() -> TestResult {
    let mut scene = Scene::new();
    let mut input = options();
    input.set_height(2.0)?;
    let object = scene.image(input)?;
    let mut session = scene.execution_session()?;
    let segment = scene.live(&mut session).declare_and_activate_fade(
        &object,
        SemanticFadeDirection::In,
        AnimationOptions::new()
            .run_time(2.0)
            .rate_func(RateFunction::Linear),
    )?;
    scene.live(&mut session).advance_segment_to(segment, 1.0)?;
    let index = session
        .frame()
        .objects
        .iter()
        .position(|row| Some(row.id) == session.execution_object_id(object.node_id()))
        .unwrap();
    assert!((session.frame().objects[index].appearance - 0.5).abs() < 1e-6);
    scene
        .live(&mut session)
        .advance_segment_to(segment, segment.end_time())?;
    scene.live(&mut session).complete_segment(segment)?;
    let resource = object.state()?.content.image().unwrap().resource();
    let target = scene.live(&mut session).target_editor(&object)?;
    scene
        .live(&mut session)
        .set_translation(&target, 4.0, 2.0)?;
    scene.live(&mut session).set_opacity(&target, 0.4)?;
    let segment = scene.live(&mut session).declare_and_activate_transform_to(
        &object,
        &target,
        AnimationOptions::new()
            .run_time(2.0)
            .rate_func(RateFunction::Linear),
    )?;
    scene
        .live(&mut session)
        .advance_segment_to(segment, segment.start_time() + 1.0)?;
    let middle = session.frame().clone();
    scene
        .live(&mut session)
        .advance_segment_to(segment, segment.end_time())?;
    scene.live(&mut session).complete_segment(segment)?;
    assert_eq!(
        object.state()?.content.image().unwrap().resource(),
        resource
    );
    assert_eq!(object.center()?, (4.0, 2.0));
    let count = scene
        .integration_store()
        .borrow()
        .raster_image_resources()
        .len();
    let created = scene.live(&mut session).create_image(options())?;
    assert!(session.execution_object_id(created.node_id()).is_none());
    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .raster_image_resources()
            .len(),
        count
    );
    assert!(middle.objects[index].transform.translation.x > 0.0);
    Ok(())
}

#[test]
fn clone_store_remaps_image_provenance_without_copying_pixels() -> TestResult {
    let mut scene = Scene::new();
    let object = scene.image(options())?;
    let store = scene.integration_store().borrow();
    let cloned = store.clone();
    let old = object.state()?.content.image().unwrap().resource();
    let new = cloned
        .semantic_object_state_checked(object.node_id())?
        .content
        .image()
        .unwrap()
        .resource();
    assert_ne!(old.arena, new.arena);
    assert!(cloned.raster_image_resources().get(old).is_none());
    let a = store.raster_image_resources().get_shared(old).unwrap();
    let b = cloned.raster_image_resources().get_shared(new).unwrap();
    assert_eq!(a.rgba8().as_ptr(), b.rgba8().as_ptr());
    assert!(Arc::strong_count(&a) >= 2);
    Ok(())
}

#[test]
fn unsupported_image_styles_and_create_are_rejected_before_publication() -> TestResult {
    let mut scene = Scene::new();
    let mut image = scene.image(options())?;
    let before = (
        scene.revision(),
        image.state()?,
        scene.integration_store().borrow().len(),
    );
    assert!(image.set_fill(1.0, 0.0, 0.0, 1.0).is_err());
    assert!(image.set_stroke_width(1.0).is_err());
    assert_eq!(
        (
            scene.revision(),
            image.state()?,
            scene.integration_store().borrow().len()
        ),
        before
    );
    let failed = scene
        .integration_store()
        .borrow_mut()
        .insert_semantic_create_animation(image.node_id(), AnimationOptions::new());
    assert!(matches!(
        failed,
        Err(noon_core::SemanticAnimationError::UnsupportedImageAnimation)
    ));
    assert_eq!(scene.revision(), before.0);
    let mut session = scene.execution_session()?;
    let frame = session.frame().clone();
    assert!(scene
        .live(&mut session)
        .declare_and_activate_create(&image, AnimationOptions::new())
        .is_err());
    assert_eq!(scene.revision(), before.0);
    assert_eq!(session.frame(), &frame);
    assert!(session.execution_object_id(image.node_id()).is_none());
    Ok(())
}

mod transform_correspondence {
    use noon::{AnimationOptions, ImageMobjectOptions, RateFunction, Scene};
    use noon_core::{Transform2D, Vec2};

    #[test]
    fn raster_transform_interpolates_corners_and_direct_seek_matches_forward() {
        let mut scene = Scene::new();
        let mut options = ImageMobjectOptions::rgba8(2, 2, vec![255; 16]).unwrap();
        options.set_height(2.0).unwrap();
        let mut image = scene.image(options).unwrap();
        image.set_translation(-2.0, 0.0).unwrap();
        scene.add(&image).unwrap();
        let mut target = image.target_editor().unwrap();
        target.set_translation(0.0, 0.0).unwrap();
        target.rotate(std::f64::consts::FRAC_PI_4).unwrap();
        target.scale(0.75, 0.75).unwrap();
        let mut session = scene.execution_session().unwrap();
        let segment = scene
            .live(&mut session)
            .declare_and_activate_transform_to(
                &image,
                &target,
                AnimationOptions::new()
                    .run_time(1.0)
                    .rate_func(RateFunction::Linear),
            )
            .unwrap();
        let id = session.execution_object_id(image.node_id()).unwrap();
        let from = Transform2D {
            translation: Vec2::new(-2.0, 0.0),
            ..Transform2D::IDENTITY
        };
        let to = Transform2D {
            translation: Vec2::ZERO,
            rotation: std::f32::consts::FRAC_PI_4,
            scale: Vec2::new(0.75, 0.75),
        };
        for alpha in [0.125, 0.25, 0.5, 0.75, 0.875] {
            scene
                .live(&mut session)
                .advance_segment_to(segment, alpha)
                .unwrap();
            let row = session
                .frame()
                .objects
                .iter()
                .find(|row| row.id == id)
                .unwrap();
            for corner in [
                Vec2::new(-1.0, -1.0),
                Vec2::new(-1.0, 1.0),
                Vec2::new(1.0, -1.0),
                Vec2::new(1.0, 1.0),
            ] {
                let a = from.transform_point(corner);
                let b = to.transform_point(corner);
                let expected = a + (b - a) * alpha as f32;
                let actual = row.transform.transform_point(corner);
                assert!(
                    (actual - expected).length() < 2e-6,
                    "alpha={alpha}, corner={corner:?}: {actual:?} != {expected:?}"
                );
            }
            let forward = session.frame().clone();
            session.seek(alpha).unwrap();
            assert_eq!(session.frame(), &forward);
        }
        scene
            .live(&mut session)
            .advance_segment_to(segment, 1.0)
            .unwrap();
        scene.live(&mut session).complete_segment(segment).unwrap();
        assert_eq!(
            image.state().unwrap().transform,
            target.state().unwrap().transform
        );
    }
}

#[test]
fn unrepresentable_image_shear_is_rejected_without_publishing() -> TestResult {
    let mut scene = Scene::new();
    let image = scene.image(options())?;
    scene.add(&image)?;
    let mut target = image.target_editor()?;
    target.rotate(0.8)?;
    target.scale(0.75, 0.25)?;
    let mut session = scene.execution_session()?;
    session.take_frame_changes();
    let revision = scene.revision();
    let nodes = scene.integration_store().borrow().len();
    let resource_stats = scene
        .integration_store()
        .borrow()
        .raster_image_resources()
        .stats();
    let original = image.state()?;
    let frame = session.frame().clone();
    let context = session.publication_context();
    let failed = scene.live(&mut session).declare_and_activate_transform_to(
        &image,
        &target,
        AnimationOptions::new(),
    );
    assert!(
        failed.is_err(),
        "TRS must not silently approximate sheared corner interpolation"
    );
    assert_eq!(scene.revision(), revision);
    assert_eq!(scene.integration_store().borrow().len(), nodes);
    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .raster_image_resources()
            .stats(),
        resource_stats
    );
    assert_eq!(image.state()?, original);
    assert_eq!(session.frame(), &frame);
    assert_eq!(session.publication_context(), context);
    assert!(session.take_frame_changes().is_empty());
    Ok(())
}
