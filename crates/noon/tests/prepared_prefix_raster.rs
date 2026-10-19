//! Test-only, one-snapshot isolation control for #2211. Not incremental delivery.
//! Compare actual first-pass pixels with a historical reader after the producer
//! changes geometry/membership and is destroyed. Reuse the existing GPU harness;
//! no mocked renderer, image substitution, or frontend execution path.

mod raster_support;

use noon::{
    AnimationOptions, ExecutionSession, FadeEndpoint, FadeTranslation, RateFunction, Scene,
};
use noon_core::{SemanticFadeDirection, SemanticVec3};
use noon_runtime::ReplayLimits;
use raster_support::Raster;

fn options(duration: f64) -> AnimationOptions {
    AnimationOptions::new()
        .run_time(duration)
        .rate_func(RateFunction::Linear)
}

fn capture(session: &mut ExecutionSession, raster: &mut Raster) -> Vec<u8> {
    let time = session.frame().time;
    let pixels = raster.capture(&session.take_renderer_publication());
    assert_eq!(
        session.frame().time,
        time,
        "rendering advanced authored time"
    );
    assert!(
        pixels
            .as_chunks::<4>().0.iter()
            .filter(|p| p[0] != 0 || p[1] != 0 || p[2] != 0)
            .count()
            > 128
    );
    pixels
}

fn verify(reader: &mut ExecutionSession, raster: &mut Raster, expected: &[(f64, Vec<u8>)]) {
    for index in [2, 0, 1, 0, 2] {
        let (time, pixels) = &expected[index];
        reader.seek(*time).unwrap();
        assert_eq!(
            capture(reader, raster),
            *pixels,
            "retained render at {time}"
        );
        assert_eq!(
            capture(reader, raster),
            *pixels,
            "same-time repeated render at {time}"
        );
    }
}

#[test]
#[ignore = "requires actual Vulkan raster; invoked explicitly by the prefix probe workflow"]
fn retained_and_fresh_renderers_preserve_completed_prefix_pixels() {
    pollster::block_on(async {
        let mut scene = Scene::new();
        let mut shape = scene.square(1.0).unwrap();
        shape.set_fill(0.2, 0.7, 1.0, 0.65).unwrap();
        let mut target = scene.circle(0.6).unwrap();
        target.set_fill(1.0, 0.2, 0.4, 0.65).unwrap();
        target.shift(2.0, 0.0).unwrap();
        let later = scene.rectangle(2.5, 0.3).unwrap();
        scene.add(&shape).unwrap();
        let mut producer = scene.execution_session().unwrap();
        producer
            .begin_replay_retention(ReplayLimits::default())
            .unwrap();
        let segment = scene
            .live(&mut producer)
            .declare_and_activate_transform_to(&shape, &target, options(1.0))
            .unwrap();
        let mut reference_renderer = Raster::new().await;
        let mut expected = Vec::new();
        for time in [0.25, 0.75] {
            producer.advance_to(time).unwrap();
            expected.push((time, capture(&mut producer, &mut reference_renderer)));
        }
        {
            let mut live = scene.live(&mut producer);
            live.advance_segment_to(segment, segment.end_time())
                .unwrap();
            live.complete_segment(segment).unwrap();
        }
        producer.advance_to(1.25).unwrap();
        expected.push((1.25, capture(&mut producer, &mut reference_renderer)));
        assert_ne!(
            expected[0].1, expected[1].1,
            "the reference must actually animate"
        );

        // Exactly one execution snapshot, only after explicit source completion.
        // The unit-level probe separately rejects pending/unsupported captures.
        // This is a control for sparse publication work, not a shippable copier.
        let mut reader = producer.clone();
        reader.seal_replay().unwrap();
        let mut retained_renderer = Raster::new().await;
        verify(&mut reader, &mut retained_renderer, &expected);

        let segment = scene
            .live(&mut producer)
            .declare_and_activate_transform_to(&shape, &later, options(1.0))
            .unwrap();
        {
            let mut live = scene.live(&mut producer);
            live.advance_segment_to(segment, segment.end_time())
                .unwrap();
            live.complete_segment(segment).unwrap();
        }
        let segment = scene
            .live(&mut producer)
            .declare_and_activate_fade_with_endpoint(
                &shape,
                SemanticFadeDirection::Out,
                FadeEndpoint::new(
                    0.25,
                    FadeTranslation::Shift(SemanticVec3::new(1.0, 0.0, 0.0)),
                ),
                options(0.5),
            )
            .unwrap();
        {
            let mut live = scene.live(&mut producer);
            live.advance_segment_to(segment, segment.end_time())
                .unwrap();
            live.complete_segment(segment).unwrap();
        }
        assert!(!scene.live(&mut producer).contains(&shape).unwrap());
        let frontier = producer.frame().time;
        verify(&mut reader, &mut retained_renderer, &expected);
        assert_eq!(producer.frame().time, frontier);

        producer.discard_replay_retention();
        drop(producer);
        drop(shape);
        drop(target);
        drop(later);
        drop(scene);
        drop(reference_renderer);
        verify(&mut reader, &mut retained_renderer, &expected);
        drop(retained_renderer);

        // Eviction/recreation must not make historical rendering depend on what
        // the earlier renderer happened to cache. These are existing retained
        // GPU resources, NOT qualification of #2094's future raster cache.
        let mut fresh_renderer = Raster::new().await;
        verify(&mut reader, &mut fresh_renderer, &expected);
        eprintln!(
            "prefix raster: 3 first-pass timestamps; 40 historical/repeated captures; real Vulkan"
        );
    });
}
