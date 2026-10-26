use super::*;
use crate::Scene;
use noon_runtime::ReplayLimits;

#[test]
fn public_prefix_record_budget_preserves_source_admission_and_exhausted_history() {
    let mut scene = Scene::new();
    let target = scene.circle(0.5).unwrap();
    scene.add(&target).unwrap();
    let mut source = scene.execution_session().unwrap();
    source
        .begin_replay_retention(ReplayLimits {
            revisions: 16,
            payloads: 512,
        })
        .unwrap();
    let boundary = source.wait_segment(1.0).unwrap();
    assert!(source
        .create_prepared_prefix_with_budgets(boundary, 0, usize::MAX)
        .is_err());
    source.advance_to(1.0).unwrap();
    let (_, probe) = source.create_prepared_prefix(boundary).unwrap();
    let table = probe.retained_history_record_bytes();
    assert!(table > 0);
    drop(probe);
    assert!(matches!(
        source.create_prepared_prefix_with_budgets(boundary, 0, table - 1),
        Err(PrefixError::HistoryRecordBudgetExceeded { .. })
    ));
    let (mut publisher, mut reader) = source
        .create_prepared_prefix_with_budgets(boundary, 0, table)
        .unwrap();
    assert_eq!(reader.history_record_budget_bytes(), table);
    assert_eq!(publisher.history_record_budget_bytes(), table);
    let frame = reader.frame().clone();
    scene
        .live(&mut source)
        .shift(&target, 1.0, 0.0)
        .unwrap();
    let next = source.wait_segment(1.0).unwrap();
    source.advance_to(next.end_time()).unwrap();
    assert!(matches!(
        source.publish_prepared_prefix(next, &mut publisher),
        Err(PrefixError::HistoryRecordBudgetExceeded { .. })
    ));
    assert_eq!(publisher.frontier(), 1.0);
    assert_eq!(reader.frame(), &frame);
    assert_eq!(reader.retained_history_record_bytes(), table);
    assert_eq!(reader.retained_resource_bytes(), 0);
    assert_eq!(reader.end_time(), None);
    reader.seek(0.5).unwrap();
    assert_eq!(reader.retained_history_record_bytes(), table);
}
