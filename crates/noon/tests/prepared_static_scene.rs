#![cfg(feature = "prepared-prefix")]

use noon::Scene;
use noon_runtime::ReplayLimits;

#[test]
fn completed_static_reader_keeps_exact_zero_endpoint_after_source_drop() {
    let mut scene = Scene::new();
    let shape = scene.square(1.0).unwrap();
    scene.add(&shape).unwrap();
    let mut source = scene.execution_session().unwrap();
    // A closed stream must not reserve unused future-history capacity.
    source
        .begin_replay_retention(ReplayLimits {
            revisions: usize::MAX,
            payloads: 16,
        })
        .unwrap();
    assert!(
        source.create_finished_prepared_reader().is_err(),
        "source completion must be explicit"
    );
    source.seal_replay().unwrap();
    let expected = source.frame().clone();
    let mut reader = source.create_finished_prepared_reader().unwrap();
    assert_eq!(reader.frontier(), 0.0);
    assert_eq!(reader.end_time(), Some(0.0));
    assert_eq!(reader.frame(), &expected);
    drop(source);
    drop(shape);
    drop(scene);
    reader.seek(0.0).unwrap();
    assert_eq!(reader.frame(), &expected);
    let publication = reader.publication_context();
    for time in [-1.0, f64::EPSILON, f64::NAN] {
        assert!(reader.seek(time).is_err());
        assert_eq!(reader.frame(), &expected);
        assert_eq!(reader.publication_context(), publication);
    }
}

#[test]
fn completed_reader_requires_sealed_current_frontier() {
    let mut scene = Scene::new();
    let shape = scene.circle(0.5).unwrap();
    scene.add(&shape).unwrap();
    let mut source = scene.execution_session().unwrap();
    source
        .begin_replay_retention(ReplayLimits::default())
        .unwrap();
    let boundary = source.wait_segment(1.0).unwrap();
    assert!(source.create_finished_prepared_reader().is_err());
    source.advance_to(boundary.end_time()).unwrap();
    assert!(source.create_finished_prepared_reader().is_err());
    source.seal_replay().unwrap();
    source.seek(0.0).unwrap();
    assert!(source.create_finished_prepared_reader().is_err());
    source.seek(1.0).unwrap();
    let mut reader = source.create_finished_prepared_reader().unwrap();
    assert_eq!(reader.end_time(), Some(1.0));
    reader.seek(1.0).unwrap();
}
