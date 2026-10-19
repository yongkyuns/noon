#![cfg(feature = "prepared-prefix")]
mod raster_support;
use noon::{ExecutionSession, PrefixError, Scene};
use noon_runtime::ReplayLimits;

fn recorded_scene() -> (Scene, noon::Mobject, ExecutionSession) {
    let mut scene = Scene::new();
    let mut object = scene.circle(0.35).unwrap();
    object.set_fill(0.2, 0.6, 1.0, 1.0).unwrap();
    scene.add(&object).unwrap();
    let mut session = scene.execution_session().unwrap();
    session
        .begin_replay_retention(ReplayLimits {
            revisions: 128,
            payloads: 2048,
        })
        .unwrap();
    (scene, object, session)
}

#[test]
fn original_source_wait_must_complete_before_initial_or_incremental_publication() {
    let (scene, object, mut source) = recorded_scene();
    let initial = source.wait_segment(1.0).unwrap();
    assert!(matches!(
        source.create_prepared_prefix(initial),
        Err(PrefixError::NotReady)
    ));
    source.advance_to(1.0).unwrap();
    let (mut publisher, mut reader) = source.create_prepared_prefix(initial).unwrap();
    let next = source.wait_segment(2.0).unwrap();
    scene.live(&mut source).shift(&object, 0.5, 0.0).unwrap();
    source.advance_to(1.5).unwrap();
    let expected = source.frame().clone();
    assert!(matches!(
        source.publish_prepared_prefix(next, &mut publisher),
        Err(PrefixError::NotReady)
    ));
    source.advance_to(3.0).unwrap();
    let revision = scene.integration_store().borrow().scene_revision();
    let batch = source
        .publish_prepared_prefix(next, &mut publisher)
        .unwrap();
    assert_eq!(batch.stats().row_payloads, 1);
    reader.append(batch).unwrap();
    assert_eq!(reader.seek(1.5).unwrap(), &expected);
    assert_eq!(
        scene.integration_store().borrow().scene_revision(),
        revision
    );
    assert_eq!(source.frame().time, 3.0, "reader seek cannot rewind source");
}

#[test]
fn expired_source_boundary_cannot_publish_a_later_frontier() {
    let (_, _, mut source) = recorded_scene();
    let first = source.wait_segment(1.0).unwrap();
    source.advance_to(1.0).unwrap();
    let (mut publisher, mut reader) = source.create_prepared_prefix(first).unwrap();
    source.advance_to(2.0).unwrap();
    assert!(matches!(
        source.publish_prepared_prefix(first, &mut publisher),
        Err(PrefixError::NotReady)
    ));
    assert_eq!(reader.seek(0.5).unwrap().time, 0.5);
    assert_eq!(reader.frontier(), 1.0);
}

#[test]
#[ignore = "requires actual Vulkan fallback adapter; explicitly run in focused CI"]
fn incremental_reader_pixels_match_first_pass_after_later_batches_and_source_destruction() {
    pollster::block_on(async {
        let (scene, object, mut source) = recorded_scene();
        let mut reference = raster_support::Raster::new().await;
        let initial = source.wait_segment(1.0).unwrap();
        source.advance_to(0.5).unwrap();
        let original = reference.capture(&source.take_renderer_publication());
        assert!(
            original.chunks_exact(4).any(|pixel| pixel[2] > 32),
            "nonempty oracle"
        );
        source.advance_to(1.0).unwrap();
        let (mut publisher, mut reader) = source.create_prepared_prefix(initial).unwrap();
        let mut expected = vec![(0.5, original)];
        let mut retained = raster_support::Raster::new().await;
        for step in 1..=4 {
            let next = source.wait_segment(1.0).unwrap();
            scene.live(&mut source).shift(&object, 0.4, 0.0).unwrap();
            source.advance_to(step as f64 + 0.5).unwrap();
            let pixels = reference.capture(&source.take_renderer_publication());
            expected.push((source.frame().time, pixels));
            source.advance_to(next.end_time()).unwrap();
            reader
                .append(
                    source
                        .publish_prepared_prefix(next, &mut publisher)
                        .unwrap(),
                )
                .unwrap();
            for (time, pixels) in expected.iter().rev() {
                reader.seek(*time).unwrap();
                assert_eq!(
                    &retained.capture(&reader.take_renderer_publication()),
                    pixels
                );
            }
        }
        drop(source);
        drop(scene);
        for _ in 0..4 {
            for (time, pixels) in expected.iter().rev() {
                reader.seek(*time).unwrap();
                assert_eq!(
                    &retained.capture(&reader.take_renderer_publication()),
                    pixels
                );
            }
        }
        let mut fresh = raster_support::Raster::new().await;
        reader.seek(1.5).unwrap();
        assert_eq!(
            fresh.capture(&reader.take_renderer_publication()),
            expected[1].1
        );
        eprintln!("PREFIX_INCREMENTAL_RASTER: 35 comparisons passed (5 first-pass timestamps, 4 append batches, source destruction, fresh renderer)");
    });
}
