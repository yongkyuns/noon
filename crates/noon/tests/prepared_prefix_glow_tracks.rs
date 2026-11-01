#![cfg(feature = "prepared-prefix")]
mod raster_support;
use noon::{
    effects::{EffectUpdate, Pixels},
    AnimationOptions, Color, ExecutionSession, PrefixReader, RateFunction, Scene,
};
use noon_runtime::ReplayLimits;

// Observe first execution, before any historical seek can conceal a bad capture.
fn history(mut observe: impl FnMut(&mut ExecutionSession)) -> PrefixReader {
    let mut scene = Scene::new();
    let mut shape = scene.circle(0.5).unwrap();
    shape.set_fill(1.0, 1.0, 1.0, 1.0).unwrap();
    shape.disable_stroke().unwrap();
    scene.add(&shape).unwrap();
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
    for (phase, color, radius, intensity) in [
        (0, Color::RED, 3.0, 0.7),
        (1, Color::BLUE, 7.0, 1.4),
        (2, Color::GREEN, 4.0, 1.1),
    ] {
        if phase == 2 {
            scene.live(&mut source).remove_glow(&shape).unwrap();
            let boundary = source.wait_segment(0.5).unwrap();
            source.advance_to(boundary.start_time()).unwrap();
            observe(&mut source);
            source.advance_to(boundary.end_time()).unwrap();
        }
        let target = scene.live(&mut source).target_editor(&shape).unwrap();
        scene
            .live(&mut source)
            .set_glow(
                &target,
                EffectUpdate::default()
                    .color(color)
                    .radius(Pixels(radius))
                    .intensity(intensity),
            )
            .unwrap();
        let segment = scene
            .live(&mut source)
            .declare_and_activate_transform_to(
                &shape,
                &target,
                AnimationOptions::new()
                    .run_time(1.0)
                    .rate_func(RateFunction::Linear),
            )
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
        // Keep the second edit, retirement and fresh generation in one suffix.
        if phase == 1 {
            continue;
        }
        let batch = source
            .publish_prepared_prefix(segment, &mut publisher)
            .unwrap();
        assert_eq!(batch.stats().resource_payloads, 0);
        reader.append(batch).unwrap();
        assert_eq!(
            reader.frame(),
            &baseline,
            "future tracks must not alter the displayed past"
        );
        assert_eq!(reader.publication_context(), publication);
    }
    let boundary = source.wait_segment(0.5).unwrap();
    source.advance_to(boundary.end_time()).unwrap();
    observe(&mut source);
    source.seal_replay().unwrap();
    reader
        .append(
            source
                .finish_prepared_prefix(boundary, &mut publisher)
                .unwrap(),
        )
        .unwrap();
    reader
}

#[test]
fn animated_glow_prefix_matches_first_execution_after_attachment_retirement_and_source_drop() {
    let mut expected = Vec::new();
    let mut reader = history(|source| expected.push(source.frame().clone()));
    let attachment = |index: usize| {
        expected[index].objects[0]
            .effects
            .as_ref()
            .unwrap()
            .iter()
            .next()
            .unwrap()
            .attachment
    };
    let glow = |index: usize| {
        expected[index].objects[0]
            .effects
            .as_ref()
            .unwrap()
            .definitions()[0]
            .as_glow()
            .unwrap()
            .to_owned()
    };
    assert!((glow(2).intensity() - 0.175).abs() < 1e-12);
    assert!((glow(5).intensity() - 0.875).abs() < 1e-12);
    assert_eq!(glow(5).radius(), noon::effects::EffectRadius::Pixels(4.0));
    let tint = glow(5).color();
    assert!((tint.red - (0.75 * Color::RED.red + 0.25 * Color::BLUE.red)).abs() < 1e-6);
    assert!((tint.blue - (0.75 * Color::RED.blue + 0.25 * Color::BLUE.blue)).abs() < 1e-6);
    assert!(expected[0].objects[0].effects.is_none());
    assert!(
        expected[7].objects[0].effects.is_none(),
        "retired attachment must disappear"
    );
    assert_ne!(
        attachment(2),
        attachment(9),
        "re-enrollment needs a new generation"
    );
    for frame in expected.iter().rev().chain(expected.iter()) {
        reader.seek(frame.time).unwrap();
        assert_eq!(reader.frame(), frame, "first execution at {}", frame.time);
    }
}

#[test]
#[ignore = "requires a native GPU; explicitly invoked by focused qualification"]
fn animated_glow_prefix_pixels_match_first_execution_after_source_drop() {
    pollster::block_on(async {
        let mut raster = raster_support::Raster::with_primary_dimensions(128, 128).await;
        let mut expected = Vec::new();
        let mut reader = history(|source| {
            let time = source.frame().time;
            let publication = source.take_renderer_publication();
            let pixels = raster.capture_effects(&publication);
            if expected.is_empty() {
                assert_eq!(raster.capture(&publication), pixels, "no-effect baseline");
            }
            expected.push((time, pixels));
        });
        assert!(
            expected[2].1 != expected[3].1,
            "enrolled intensity must visibly animate"
        );
        assert!(
            expected[4].1 != expected[6].1,
            "color and radius must visibly animate"
        );
        assert_eq!(
            expected[0].1, expected[7].1,
            "retirement restores plain pixels"
        );
        assert!(
            expected[9].1 != expected[10].1,
            "fresh generation must visibly animate"
        );
        drop(raster);
        let mut historical = raster_support::Raster::with_primary_dimensions(128, 128).await;
        for (time, pixels) in expected.iter().rev().chain(expected.iter()) {
            reader.seek(*time).unwrap();
            assert!(
                &historical.capture_effects(&reader.take_renderer_publication()) == pixels,
                "first-execution pixels at {time}"
            );
        }
    });
}
