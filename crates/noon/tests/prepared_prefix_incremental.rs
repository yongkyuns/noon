#![cfg(feature = "prepared-prefix")]
mod raster_support;
use noon::{PrefixError, Scene, SceneExecution, SemanticVec3};
use noon_runtime::ReplayLimits;

fn recorded_scene() -> (noon::Mobject, SceneExecution) {
    let mut scene = Scene::new();
    let mut object = scene.circle(0.35).unwrap();
    object.set_fill(0.2, 0.6, 1.0, 1.0).unwrap();
    scene.add(&object).unwrap();
    let mut session = scene.into_execution().unwrap();
    session
        .session_mut()
        .begin_replay_retention(ReplayLimits {
            revisions: 128,
            payloads: 2048,
        })
        .unwrap();
    (object, session)
}

#[test]
fn original_source_wait_must_complete_before_initial_or_incremental_publication() {
    let (object, mut source) = recorded_scene();
    let initial = source.session().wait_segment(1.0).unwrap();
    assert!(matches!(
        source.session_mut().create_prepared_prefix(initial),
        Err(PrefixError::NotReady)
    ));
    source.session_mut().advance_to(1.0).unwrap();
    let (mut publisher, mut reader) = source
        .session_mut()
        .create_prepared_prefix(initial)
        .unwrap();
    let next = source.session().wait_segment(2.0).unwrap();
    source
        .scene_mut()
        .shift_3d(&object, SemanticVec3::new(0.5, 0.0, 0.0))
        .unwrap();
    source.session_mut().advance_to(1.5).unwrap();
    let expected = source.session().frame().clone();
    assert!(matches!(
        source
            .session_mut()
            .publish_prepared_prefix(next, &mut publisher),
        Err(PrefixError::NotReady)
    ));
    source.session_mut().advance_to(3.0).unwrap();
    let revision = source.scene().revision();
    let batch = source
        .session_mut()
        .publish_prepared_prefix(next, &mut publisher)
        .unwrap();
    assert_eq!(batch.stats().row_payloads, 1);
    reader.append(batch).unwrap();
    assert_eq!(reader.seek(1.5).unwrap(), &expected);
    assert_eq!(source.scene().revision(), revision);
    assert_eq!(
        source.session().frame().time,
        3.0,
        "reader seek cannot rewind source"
    );
}

#[test]
fn expired_source_boundary_cannot_publish_a_later_frontier() {
    let (_, mut source) = recorded_scene();
    let first = source.session().wait_segment(1.0).unwrap();
    source.session_mut().advance_to(1.0).unwrap();
    let (mut publisher, mut reader) = source.session_mut().create_prepared_prefix(first).unwrap();
    source.session_mut().advance_to(2.0).unwrap();
    assert!(matches!(
        source
            .session_mut()
            .publish_prepared_prefix(first, &mut publisher),
        Err(PrefixError::NotReady)
    ));
    assert_eq!(reader.seek(0.5).unwrap().time, 0.5);
    assert_eq!(reader.frontier(), 1.0);
}

#[test]
#[ignore = "requires actual Vulkan fallback adapter; explicitly run in focused CI"]
fn incremental_reader_pixels_match_first_pass_after_later_batches_and_source_destruction() {
    pollster::block_on(async {
        let (object, mut source) = recorded_scene();
        let mut reference = raster_support::Raster::new().await;
        let initial = source.session().wait_segment(1.0).unwrap();
        source.session_mut().advance_to(0.5).unwrap();
        let original = reference.capture(&source.session_mut().take_renderer_publication());
        assert!(
            original
                .as_chunks::<4>()
                .0
                .iter()
                .any(|pixel| pixel[2] > 32),
            "nonempty oracle"
        );
        source.session_mut().advance_to(1.0).unwrap();
        let (mut publisher, mut reader) = source
            .session_mut()
            .create_prepared_prefix(initial)
            .unwrap();
        let mut expected = vec![(0.5, original)];
        let mut retained = raster_support::Raster::new().await;
        for step in 1..=4 {
            let next = source.session().wait_segment(1.0).unwrap();
            source
                .scene_mut()
                .shift_3d(&object, SemanticVec3::new(0.4, 0.0, 0.0))
                .unwrap();
            source.session_mut().advance_to(step as f64 + 0.5).unwrap();
            let pixels = reference.capture(&source.session_mut().take_renderer_publication());
            expected.push((source.session().frame().time, pixels));
            source.session_mut().advance_to(next.end_time()).unwrap();
            reader
                .append(
                    source
                        .session_mut()
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
