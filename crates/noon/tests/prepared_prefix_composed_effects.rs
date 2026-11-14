#![cfg(feature = "prepared-prefix")]

use noon::{
    effects::{Blur, DissolveDirection, DissolveStyle, EffectUpdate, Pixels},
    AnimationCompositionRequest, AnimationOptions, ExecutionSession, ImagePaintOptions,
    PrefixReader, RateFunction, Scene, SemanticAnimationCompositionKind,
};
use noon_core::{ComposedSource, EffectDefinition, ObjectId, SemanticPaint};
use noon_runtime::ReplayLimits;

fn image_paint(color: [u8; 4]) -> ImagePaintOptions {
    ImagePaintOptions::rgba8(1, 1, color.to_vec()).unwrap()
}

fn paint_pixels(
    reader: &mut PrefixReader,
    time: f64,
    object: ObjectId,
) -> (noon_core::RasterImageResourceHandle, Vec<u8>) {
    reader.seek(time).unwrap();
    let publication = reader.take_renderer_publication();
    let row = publication
        .frame()
        .objects
        .iter()
        .find(|row| row.id == object)
        .unwrap();
    let image = row
        .fill_paint
        .as_deref()
        .and_then(SemanticPaint::as_image)
        .unwrap();
    let handle = image.resource();
    let pixels = publication
        .raster_image_resources()
        .get(handle)
        .unwrap()
        .rgba8()
        .to_vec();
    (handle, pixels)
}

fn composed_source(reader: &PrefixReader, owner: ObjectId) -> &ComposedSource {
    reader
        .frame()
        .objects
        .iter()
        .find(|row| row.id == owner)
        .and_then(|row| row.content.composed_source())
        .unwrap()
}

fn append_wait(
    source: &mut ExecutionSession,
    publisher: &mut noon_runtime::PrefixPublisher,
    reader: &mut PrefixReader,
) {
    let boundary = source.wait_segment(0.5).unwrap();
    source.advance_to(boundary.end_time()).unwrap();
    reader
        .append(source.publish_prepared_prefix(boundary, publisher).unwrap())
        .unwrap();
}

#[test]
fn composed_family_prefix_keeps_members_resources_and_effect_tracks_historical() {
    let old_pixels = [240, 20, 10, 255];
    let new_pixels = [10, 30, 240, 255];
    let mut scene = Scene::new();
    let mut coral = scene.circle(0.78).unwrap();
    let mut teal = scene.rectangle(1.3, 1.5).unwrap();
    let mut gold = scene.circle(0.34).unwrap();
    coral.set_translation(-0.45, -0.2).unwrap();
    teal.set_translation(0.5, -0.1).unwrap();
    gold.set_translation(0.35, 0.75).unwrap();
    teal.set_fill_image(image_paint(old_pixels), Some(1.0))
        .unwrap();
    let mut treated = scene
        .family(&[(&coral).into(), (&teal).into(), (&gold).into()])
        .unwrap();
    treated
        .add_effect(
            Blur::new(EffectUpdate::default().radius(Pixels(0.0))).unwrap(),
            "soften",
        )
        .unwrap();
    treated
        .set_glow(
            EffectUpdate::default()
                .radius(Pixels(8.0))
                .intensity(0.0)
                .color(noon::Color::rgba(0.35, 0.65, 1.0, 1.0)),
        )
        .unwrap();
    scene.add_many(&[(&treated).into()]).unwrap();

    let mut execution = scene.into_execution().unwrap();
    execution
        .session_mut()
        .begin_replay_retention(ReplayLimits {
            revisions: 128,
            payloads: 512,
        })
        .unwrap();
    let initial = execution.session_mut().wait_segment(0.6).unwrap();
    execution
        .session_mut()
        .advance_to(initial.end_time())
        .unwrap();
    let (mut publisher, mut reader) = execution.session().create_prepared_prefix(initial).unwrap();
    reader.seek(0.3).unwrap();
    let paused = reader.frame().clone();
    let owner = execution
        .session()
        .execution_object_id(treated.node_id())
        .unwrap();
    let members = [
        execution
            .session()
            .execution_object_id(coral.node_id())
            .unwrap(),
        execution
            .session()
            .execution_object_id(teal.node_id())
            .unwrap(),
        execution
            .session()
            .execution_object_id(gold.node_id())
            .unwrap(),
    ];
    assert_eq!(composed_source(&reader, owner).members(), members);
    let (old_version, old_at_start) = paint_pixels(&mut reader, 0.3, members[1]);
    assert_eq!(old_at_start, old_pixels);
    reader.seek(0.3).unwrap();
    let publication = reader.publication_context();

    let target = execution.scene_mut().copy_family(&treated).unwrap();
    execution
        .live()
        .set_effect(
            target.root(),
            "soften",
            EffectUpdate::default().radius(Pixels(2.0)),
        )
        .unwrap();
    execution
        .live()
        .set_effect(
            target.root(),
            "glow",
            EffectUpdate::default().intensity(1.6),
        )
        .unwrap();
    let options = AnimationOptions::new()
        .run_time(1.0)
        .rate_func(RateFunction::Linear);
    let track_segment = execution
        .live()
        .declare_and_activate_animation_composition(
            SemanticAnimationCompositionKind::Parallel,
            &[AnimationCompositionRequest::FamilyMethodTargetTo {
                source: &treated,
                target_state: target.root(),
                options,
            }],
            AnimationOptions::new(),
            AnimationOptions::new(),
        )
        .unwrap();
    execution
        .session_mut()
        .advance_segment_to(track_segment, track_segment.start_time() + 0.5)
        .unwrap();
    let mid_track_time = execution.session().frame().time;
    execution
        .session_mut()
        .advance_segment_to(track_segment, track_segment.end_time())
        .unwrap();
    execution.live().complete_segment(track_segment).unwrap();
    reader
        .append(
            execution
                .session()
                .publish_prepared_prefix(track_segment, &mut publisher)
                .unwrap(),
        )
        .unwrap();
    assert_eq!(reader.frame(), &paused, "effect tracks stay in the future");
    assert_eq!(reader.publication_context(), publication);
    reader.seek(mid_track_time).unwrap();
    let effects = reader
        .frame()
        .objects
        .iter()
        .find(|row| row.id == owner)
        .unwrap()
        .effects
        .as_ref()
        .unwrap();
    let radius = effects.definitions()[0].as_blur().unwrap().radius();
    let noon::effects::EffectRadius::Pixels(radius) = radius else {
        panic!("expected pixel radius");
    };
    assert!((radius - 1.0).abs() < 1e-12);
    assert!((effects.definitions()[1].as_glow().unwrap().intensity() - 0.8).abs() < 1e-12);
    assert_eq!(composed_source(&reader, owner).members(), members);

    execution
        .scene_mut()
        .set_fill_image(&teal, image_paint(new_pixels), Some(1.0))
        .unwrap();
    append_wait(execution.session_mut(), &mut publisher, &mut reader);
    assert_eq!(reader.frame().time, mid_track_time);
    assert_eq!(
        reader
            .frame()
            .objects
            .iter()
            .find(|row| row.id == owner)
            .unwrap()
            .content
            .composed_source()
            .unwrap()
            .members(),
        members
    );
    let (new_version, new_at_later_time) =
        paint_pixels(&mut reader, track_segment.end_time() + 0.25, members[1]);
    assert_ne!(new_version, old_version);
    assert_eq!(new_at_later_time, new_pixels);
    let (old_version_again, old_at_earlier_time) = paint_pixels(&mut reader, 0.3, members[1]);
    assert_eq!(old_version_again, old_version);
    assert_eq!(old_at_earlier_time, old_pixels);

    for (direction, seed) in [(DissolveDirection::Out, 7), (DissolveDirection::In, 7)] {
        let request = AnimationCompositionRequest::Dissolve {
            target: (&treated).into(),
            direction,
            style: DissolveStyle::new(EffectUpdate::default().seed(seed).softness(0.2)).unwrap(),
            options: AnimationOptions::new()
                .run_time(0.6)
                .rate_func(RateFunction::Linear)
                .remover(false),
        };
        let segment = execution
            .live()
            .declare_and_activate_composition(&request, AnimationOptions::new())
            .unwrap();
        execution
            .session_mut()
            .advance_segment_to(segment, segment.start_time() + 0.3)
            .unwrap();
        let middle = execution.session().frame().time;
        execution
            .session_mut()
            .advance_segment_to(segment, segment.end_time())
            .unwrap();
        execution.live().complete_segment(segment).unwrap();
        let paused_frame = reader.frame().clone();
        reader
            .append(
                execution
                    .session()
                    .publish_prepared_prefix(segment, &mut publisher)
                    .unwrap(),
            )
            .unwrap();
        assert_eq!(
            reader.frame(),
            &paused_frame,
            "paused timeline must not follow dissolve"
        );
        reader.seek(middle).unwrap();
        let stack = reader
            .frame()
            .objects
            .iter()
            .find(|row| row.id == owner)
            .unwrap()
            .effects
            .as_ref()
            .unwrap();
        assert_eq!(stack.definitions().len(), 3);
        assert!(matches!(
            stack.definitions()[2],
            EffectDefinition::NoiseDissolve(_)
        ));
        assert!(
            (stack.definitions()[2]
                .as_noise_dissolve()
                .unwrap()
                .progress()
                - 0.5)
                .abs()
                < 1e-12
        );
        assert_eq!(composed_source(&reader, owner).members(), members);
    }

    let old_attachments = reader
        .frame()
        .objects
        .iter()
        .find(|row| row.id == owner)
        .unwrap()
        .effects
        .as_ref()
        .unwrap()
        .attachments()
        .to_vec();
    let removal_start = execution.session().frame().time;
    execution.live().remove_effect(&treated, "soften").unwrap();
    execution.live().remove_glow(&treated).unwrap();
    append_wait(execution.session_mut(), &mut publisher, &mut reader);
    let removal_time = execution.session().frame().time;
    reader.seek(removal_start + 0.25).unwrap();
    let row = reader
        .frame()
        .objects
        .iter()
        .find(|row| row.id == owner)
        .unwrap();
    assert!(
        row.effects.is_none(),
        "completed dissolve retires visibility"
    );

    execution
        .live()
        .add_effect(
            &treated,
            Blur::new(EffectUpdate::default().radius(Pixels(0.0))).unwrap(),
            "soften",
        )
        .unwrap();
    execution
        .live()
        .set_glow(
            &treated,
            EffectUpdate::default()
                .radius(Pixels(8.0))
                .intensity(0.0)
                .color(noon::Color::rgba(0.35, 0.65, 1.0, 1.0)),
        )
        .unwrap();
    append_wait(execution.session_mut(), &mut publisher, &mut reader);
    assert_eq!(reader.frame().time, removal_start + 0.25);
    reader.seek(removal_time + 0.25).unwrap();
    let stack = reader
        .frame()
        .objects
        .iter()
        .find(|row| row.id == owner)
        .unwrap()
        .effects
        .as_ref()
        .unwrap();
    assert_eq!(stack.definitions().len(), 2);
    assert!(stack.attachments()[0] != old_attachments[0]);
    assert!(stack.attachments()[1] != old_attachments[1]);
    assert_eq!(composed_source(&reader, owner).members(), members);

    let dissolve = AnimationCompositionRequest::Dissolve {
        target: (&treated).into(),
        direction: DissolveDirection::Out,
        style: DissolveStyle::new(EffectUpdate::default().seed(7).softness(0.2)).unwrap(),
        options: AnimationOptions::new()
            .run_time(0.6)
            .rate_func(RateFunction::Linear)
            .remover(false),
    };
    let segment = execution
        .live()
        .declare_and_activate_composition(&dissolve, AnimationOptions::new())
        .unwrap();
    execution
        .session_mut()
        .advance_segment_to(segment, segment.start_time() + 0.3)
        .unwrap();
    let visibility_time = execution.session().frame().time;
    execution
        .session_mut()
        .advance_segment_to(segment, segment.end_time())
        .unwrap();
    execution.live().complete_segment(segment).unwrap();
    let paused_frame = reader.frame().clone();
    reader
        .append(
            execution
                .session()
                .publish_prepared_prefix(segment, &mut publisher)
                .unwrap(),
        )
        .unwrap();
    assert_eq!(reader.frame(), &paused_frame);
    reader.seek(visibility_time).unwrap();
    let stack = reader
        .frame()
        .objects
        .iter()
        .find(|row| row.id == owner)
        .unwrap()
        .effects
        .as_ref()
        .unwrap();
    assert_eq!(stack.definitions().len(), 3);
    assert_ne!(stack.attachments()[2], old_attachments[2]);
    assert!(matches!(
        stack.definitions()[2],
        EffectDefinition::NoiseDissolve(_)
    ));
    assert_eq!(composed_source(&reader, owner).members(), members);
    reader.seek(track_segment.start_time()).unwrap();
    assert_eq!(composed_source(&reader, owner).members(), members);
    assert_eq!(
        paint_pixels(&mut reader, track_segment.start_time(), members[1]).1,
        old_pixels
    );
}
