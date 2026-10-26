use crate::effects::GlowUpdate;
use crate::{
    AnimationOptions, ExecutionSession, ImageAddressMode, ImageFit, ImageMapping, ImagePaint,
    ImageUvTransform, Mobject, RasterImageSampling, RateFunction, Scene, SemanticPaint,
};

fn paint(scene: &mut Scene) -> (Mobject, ImagePaint) {
    let image = scene
        .image_rgba8(2, 1, vec![255, 0, 0, 255, 0, 0, 255, 128])
        .unwrap();
    let paint = image
        .image_paint(ImageMapping::default(), Some(RasterImageSampling::Linear))
        .unwrap();
    (image, paint)
}

fn mapped(paint: ImagePaint, translation: [f64; 2], scale: [f64; 2]) -> ImagePaint {
    ImagePaint::new(
        paint.resource(),
        ImageMapping::new(
            ImageFit::Cover,
            [0.5; 2],
            ImageAddressMode::Clamp,
            ImageUvTransform::new(translation, scale, 0.0).unwrap(),
        )
        .unwrap(),
        paint.sampling(),
    )
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

fn options() -> AnimationOptions {
    AnimationOptions::new()
        .run_time(1.0)
        .rate_func(RateFunction::Linear)
}

#[test]
fn image_fill_mapping_and_opacity_animate_seek_and_complete_exactly() {
    let mut scene = Scene::new();
    let (_image, paint) = paint(&mut scene);
    let mut object = scene.square(1.0).unwrap();
    object.set_fill_image(paint, Some(0.25)).unwrap();
    scene.add(&object).unwrap();
    let end = mapped(paint, [0.4, -0.2], [0.5, 1.5]);
    let mut target = object.target_editor().unwrap();
    target.set_fill_image(end, Some(0.75)).unwrap();
    target.set_translation(2.0, 0.0).unwrap();
    let animation = scene
        .declare_transform_to(&object, &target, options())
        .unwrap();
    let mut session = scene.execution_session().unwrap();
    let segment = scene.live(&mut session).play_animation(&animation).unwrap();
    scene
        .live(&mut session)
        .advance_segment_to(segment, 0.5)
        .unwrap();
    let middle = row(&session, &object);
    assert_eq!(
        middle
            .fill_image
            .as_deref()
            .unwrap()
            .mapping()
            .transform()
            .translation(),
        [0.2, -0.1]
    );
    assert_eq!(
        middle
            .fill_image
            .as_deref()
            .unwrap()
            .mapping()
            .transform()
            .scale(),
        [0.75, 1.25]
    );
    assert_eq!(middle.style.fill.unwrap().alpha, 0.5);
    assert_eq!(middle.transform.translation.x, 1.0);
    let captured = scene.live(&mut session).effective(&object).unwrap();
    assert_eq!(
        captured.fill_image.as_deref().copied(),
        Some(paint.interpolate(end, 0.5).unwrap())
    );
    assert!(scene
        .live(&mut session)
        .effective_fill_color(&object)
        .is_err());
    scene
        .live(&mut session)
        .advance_segment_to(segment, 1.0)
        .unwrap();
    scene.live(&mut session).complete_segment(segment).unwrap();
    assert_eq!(
        scene
            .live(&mut session)
            .authored(&object)
            .unwrap()
            .style
            .fill,
        Some(SemanticPaint::Image(std::sync::Arc::new(end)))
    );
    session.seek(0.5).unwrap();
    assert_eq!(
        row(&session, &object).fill_image.as_deref().copied(),
        Some(paint.interpolate(end, 0.5).unwrap())
    );
    session.seek(0.0).unwrap();
    assert_eq!(
        row(&session, &object).fill_image.as_deref().copied(),
        Some(paint)
    );
    session.seek(1.0).unwrap();
    assert_eq!(
        row(&session, &object).fill_image.as_deref().copied(),
        Some(end)
    );
}

#[test]
fn historical_image_fill_tracks_keep_effect_rejection_atomic_after_solid_replacement() {
    let mut scene = Scene::new();
    let (_image, paint) = paint(&mut scene);
    let mut object = scene.square(1.0).unwrap();
    object.set_fill_image(paint, Some(0.5)).unwrap();
    scene.add(&object).unwrap();
    let mut target = object.target_editor().unwrap();
    let end = mapped(paint, [0.4, 0.0], [1.0, 1.0]);
    target.set_fill_image(end, None).unwrap();
    let animation = scene
        .declare_transform_to(&object, &target, options())
        .unwrap();
    let mut session = scene.execution_session().unwrap();
    let segment = scene.live(&mut session).play_animation(&animation).unwrap();
    scene
        .live(&mut session)
        .advance_segment_to(segment, 1.0)
        .unwrap();
    scene.live(&mut session).complete_segment(segment).unwrap();
    scene
        .live(&mut session)
        .set_fill(&object, 0.0, 1.0, 0.0, 1.0)
        .unwrap();
    let before = row(&session, &object).clone();
    let authored = object.state().unwrap();
    let publication = session.publication_context();
    assert!(scene
        .live(&mut session)
        .set_glow(&object, GlowUpdate::default().intensity(1.0))
        .is_err());
    assert_eq!(row(&session, &object), &before);
    assert_eq!(object.state().unwrap(), authored);
    assert_eq!(session.publication_context(), publication);
    assert!(object
        .integration_store()
        .borrow()
        .animation_effect_snapshot(object.node_id())
        .unwrap()
        .is_empty());
    session.seek(0.5).unwrap();
    assert_eq!(
        row(&session, &object).fill_image.as_deref().copied(),
        Some(paint.interpolate(end, 0.5).unwrap())
    );
    assert!(row(&session, &object).glow.is_none());
}

#[test]
fn image_fill_live_publication_captures_new_resource_and_clears_back_to_solid() {
    let mut scene = Scene::new();
    let (_image, paint) = paint(&mut scene);
    let object = scene.circle(0.4).unwrap();
    let unrelated = scene.circle(0.3).unwrap();
    scene.add(&object).unwrap();
    scene.add(&unrelated).unwrap();
    let mut session = scene.execution_session().unwrap();
    let previous = row(&session, &unrelated).clone();
    scene
        .live(&mut session)
        .set_fill_image(&object, paint, Some(0.6))
        .unwrap();
    assert_eq!(
        row(&session, &object).fill_image.as_deref().copied(),
        Some(paint)
    );
    assert_eq!(row(&session, &unrelated), &previous);
    assert!(session
        .raster_image_resources()
        .get(paint.resource())
        .is_some());
    scene
        .live(&mut session)
        .set_fill(&object, 0.0, 1.0, 0.0, 0.8)
        .unwrap();
    assert_eq!(row(&session, &object).fill_image.as_deref().copied(), None);
    assert_eq!(
        row(&session, &object).style.fill.unwrap(),
        crate::Color::rgba(0.0, 1.0, 0.0, 0.8)
    );
}

#[test]
fn foreign_image_and_effect_combinations_reject_without_authored_changes() {
    let mut scene = Scene::new();
    let (_image, paint) = paint(&mut scene);
    let mut foreign = Scene::new();
    let mut foreign_object = foreign.square(1.0).unwrap();
    let before = foreign_object.state().unwrap();
    assert!(foreign_object.set_fill_image(paint, Some(0.3)).is_err());
    assert_eq!(foreign_object.state().unwrap(), before);
    let mut object = scene.square(1.0).unwrap();
    object.set_fill_image(paint, None).unwrap();
    let before = object.state().unwrap();
    assert!(object
        .set_glow(GlowUpdate::default().intensity(1.0))
        .is_err());
    assert_eq!(object.state().unwrap(), before);
    assert!(object
        .integration_store()
        .borrow()
        .animation_effect_snapshot(object.node_id())
        .unwrap()
        .is_empty());
    let mut glowing = scene.circle(0.5).unwrap();
    glowing
        .set_glow(GlowUpdate::default().intensity(1.0))
        .unwrap();
    let before = glowing.state().unwrap();
    assert!(glowing.set_fill_image(paint, None).is_err());
    assert_eq!(glowing.state().unwrap(), before);
}

#[test]
fn image_fill_family_transform_rejects_before_admitting_padding_or_animation() {
    let mut scene = Scene::new();
    let (_image, paint) = paint(&mut scene);
    let mut object = scene.square(1.0).unwrap();
    object.set_fill_image(paint, Some(0.5)).unwrap();
    let first = object.target_editor().unwrap();
    let second = object.target_editor().unwrap();
    let source = scene.family(&[(&object).into()]).unwrap();
    let target = scene.family(&[(&first).into(), (&second).into()]).unwrap();
    scene.add_many(&[(&source).into()]).unwrap();
    let mut session = scene.execution_session().unwrap();
    let before = session.frame().clone();
    let publication = session.publication_context();
    let request = crate::AnimationCompositionRequest::FamilyTransformTo {
        source: &source,
        target_state: &target,
        options: options(),
    };
    assert!(scene
        .live(&mut session)
        .declare_and_activate_composition(&request, options())
        .is_err());
    assert_eq!(session.frame(), &before);
    assert_eq!(session.publication_context(), publication);
    assert_eq!(
        row(&session, &object).fill_image.as_deref().copied(),
        Some(paint)
    );
}

#[test]
fn incompatible_mapping_animation_rejects_without_a_partial_fill_or_motion() {
    let mut scene = Scene::new();
    let (_image, paint) = paint(&mut scene);
    let mut object = scene.square(1.0).unwrap();
    object.set_fill_image(paint, Some(0.7)).unwrap();
    scene.add(&object).unwrap();
    let mut target = object.target_editor().unwrap();
    target
        .set_fill_image(mapped(paint, [0.4, 0.0], [-1.0, 1.0]), Some(0.2))
        .unwrap();
    target.set_translation(3.0, 0.0).unwrap();
    let animation = scene
        .declare_transform_to(&object, &target, options())
        .unwrap();
    let mut session = scene.execution_session().unwrap();
    let before = row(&session, &object).clone();
    assert!(scene.live(&mut session).play_animation(&animation).is_err());
    assert_eq!(row(&session, &object), &before);
    assert_eq!(
        object.state().unwrap().style.fill,
        Some(SemanticPaint::Image(std::sync::Arc::new(paint)))
    );
}

#[test]
fn copied_image_fill_and_cloned_store_keep_local_resource_provenance() {
    let mut scene = Scene::new();
    let (_image, paint) = paint(&mut scene);
    let mut object = scene.square(1.0).unwrap();
    object.set_fill_image(paint, Some(0.625)).unwrap();
    let target = object.target_editor().unwrap();
    assert_eq!(
        target.state().unwrap().style.fill,
        Some(SemanticPaint::Image(std::sync::Arc::new(paint)))
    );
    let store = object.integration_store().borrow();
    let cloned = store.clone();
    let Some(SemanticPaint::Image(copy)) = cloned
        .semantic_object_state_checked(object.node_id())
        .unwrap()
        .style
        .fill
        .as_ref()
    else {
        panic!("cloned image fill");
    };
    assert_ne!(copy.resource().arena, paint.resource().arena);
    assert!(cloned
        .raster_image_resources()
        .get(paint.resource())
        .is_none());
    assert!(cloned
        .raster_image_resources()
        .get(copy.resource())
        .is_some());
    assert_eq!(copy.mapping(), paint.mapping());
}
