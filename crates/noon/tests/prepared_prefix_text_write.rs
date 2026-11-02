#![cfg(all(feature = "prepared-prefix", feature = "typst"))]
mod raster_support;
use noon::{
    effects::{EffectUpdate, Pixels},
    AnimationOptions, Color, ExecutionSession, MathTypst, PrefixReader, RateFunction, Scene,
};
use noon_runtime::{FrameState, PrefixError, ReplayLimits};

fn history(mut observe: impl FnMut(&mut ExecutionSession)) -> PrefixReader {
    let mut scene = Scene::new();
    let mut anchor = scene.circle(0.2).unwrap();
    anchor.shift(-2.5, 0.0).unwrap();
    scene.add(&anchor).unwrap();
    let mut source = scene.execution_session().unwrap();
    source
        .begin_replay_retention(ReplayLimits::default())
        .unwrap();
    let boundary = source.wait_segment(1.0).unwrap();
    source.advance_to(0.5).unwrap();
    observe(&mut source);
    source.advance_to(1.0).unwrap();
    let (mut publisher, mut reader) = source.create_prepared_prefix(boundary).unwrap();
    reader.seek(0.5).unwrap();
    let baseline = reader.frame().clone();
    let publication = reader.publication_context();
    let text = scene
        .live(&mut source)
        .create_math_typst(
            MathTypst::new("integral_0^1 x^2 dif x = frac(1, 3)").with_font_size(36.0),
        )
        .unwrap();
    scene
        .live(&mut source)
        .set_glow(
            &text,
            EffectUpdate::default()
                .color(Color::BLUE)
                .radius(Pixels(3.0))
                .intensity(0.8),
        )
        .unwrap();
    for reverse in [false, true] {
        let options = AnimationOptions::new()
            .run_time(1.0)
            .rate_func(RateFunction::Smooth)
            .reverse_rate_function(reverse)
            .introducer(!reverse)
            .remover(reverse);
        let segment = scene
            .live(&mut source)
            .declare_and_activate_text_write(&text, reverse, options)
            .unwrap();
        for fraction in [0.0, 0.25, 0.75] {
            scene
                .live(&mut source)
                .advance_segment_to(segment, segment.start_time() + fraction)
                .unwrap();
            observe(&mut source);
        }
        {
            let mut live = scene.live(&mut source);
            live.advance_segment_to(segment, segment.end_time())
                .unwrap();
            live.complete_segment(segment).unwrap();
        }
        let batch = source
            .publish_prepared_prefix(segment, &mut publisher)
            .unwrap();
        assert_eq!(
            batch.required_family_animation_capacity(),
            if reverse { 2 } else { 1 }
        );
        assert!(matches!(
            reader.preflight_append(&batch),
            Err(PrefixError::CapacityRequired { .. })
                | Err(PrefixError::FamilyAnimationCapacityRequired { .. })
        ));
        reader.reserve_append_capacity(&batch).unwrap();
        reader.append(batch).unwrap();
        assert_eq!(
            reader.frame(),
            &baseline,
            "adoption must not evaluate a future glyph plan"
        );
        assert_eq!(reader.publication_context(), publication);
    }
    let boundary = source.wait_segment(0.5).unwrap();
    source.advance_to(boundary.end_time()).unwrap();
    observe(&mut source);
    source.seal_replay().unwrap();
    let batch = source
        .finish_prepared_prefix(boundary, &mut publisher)
        .unwrap();
    reader.reserve_append_capacity(&batch).unwrap();
    reader.append(batch).unwrap();
    reader
}

// New physical slots are dormant before their creation revision. Compare every
// original frame column while explicitly rejecting visible future membership.
fn historical_frame(mut frame: FrameState, expected_rows: usize) -> FrameState {
    assert!(frame.presences[expected_rows..]
        .iter()
        .all(|&present| !present));
    assert!(frame.family_animations[expected_rows..]
        .iter()
        .all(Option::is_none));
    frame.objects.truncate(expected_rows);
    frame.presences.truncate(expected_rows);
    frame.reveals.truncate(expected_rows);
    frame.morphs.truncate(expected_rows);
    frame.render_geometries.truncate(expected_rows);
    frame.render_transforms.truncate(expected_rows);
    frame.family_animations.truncate(expected_rows);
    frame.family_animation_plan_indices.truncate(expected_rows);
    frame
}

#[test]
fn late_glyph_plans_match_first_execution_after_source_drop_and_rewind() {
    let mut expected = Vec::new();
    let mut reader = history(|source| expected.push(source.frame().clone()));
    assert!(expected[2].family_animations[1].is_some());
    assert_ne!(expected[2].family_animations, expected[3].family_animations);
    assert_eq!(expected[2].family_animation_plan_indices[1], Some(0));
    assert_eq!(expected[5].family_animation_plan_indices[1], Some(1));
    assert!(
        expected[5].family_animations[1]
            .unwrap()
            .reverse_member_order
    );
    for frame in expected.iter().rev().chain(expected.iter()) {
        reader.seek(frame.time).unwrap();
        assert_eq!(
            historical_frame(reader.frame().clone(), frame.objects.len()),
            *frame,
            "first execution at {}",
            frame.time
        );
    }
}

#[test]
#[ignore = "requires a native GPU; explicitly invoked by focused qualification"]
fn late_glyph_plan_pixels_match_first_execution_after_source_drop() {
    pollster::block_on(async {
        let mut raster = raster_support::Raster::with_primary_dimensions(192, 128).await;
        let mut expected = Vec::new();
        let mut reader = history(|source| {
            let time = source.frame().time;
            let publication = source.take_renderer_publication();
            let pixels = raster.capture_effects(&publication);
            if expected.is_empty() {
                assert_eq!(raster.capture(&publication), pixels);
            }
            expected.push((time, pixels));
        });
        assert!(
            expected[2].1 != expected[3].1,
            "glyph reveal must visibly progress"
        );
        assert!(
            expected[5].1 != expected[6].1,
            "reverse reveal must visibly progress"
        );
        assert_eq!(
            expected[0].1,
            expected.last().unwrap().1,
            "removal restores the anchor"
        );
        drop(raster);
        let mut historical = raster_support::Raster::with_primary_dimensions(192, 128).await;
        for (time, pixels) in expected.iter().rev().chain(expected.iter()) {
            reader.seek(*time).unwrap();
            assert!(
                &historical.capture_effects(&reader.take_renderer_publication()) == pixels,
                "first-execution glyph pixels at {time}"
            );
        }
    });
}
