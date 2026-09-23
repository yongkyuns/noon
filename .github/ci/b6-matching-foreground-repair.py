from pathlib import Path
import sys


if sys.argv[1] == '--regressions':
    path = Path('crates/noon/src/execution_session/family_transform_tests.rs')
    text = path.read_text()
    start = text.index('fn assert_matching_foreground_completion(')
    prefix, tests = text[:start], text[start:]
    old = 'session.apply_semantic_transaction(&mut store, add).unwrap();'
    assert tests.count(old) == 1
    tests = tests.replace(old, 'session.apply_semantic_transaction_at_root(&mut store, root, add).unwrap();')
    old = '    let publication = session.publication_context();\n    assert!(session.complete_segment(&mut store, segment).is_err());'
    new = '''    let publication = session.publication_context();
    let frame = session.frame().clone();
    let painter_order = session.painter_order().to_vec();
    let revision = store.scene_revision();
    session.take_frame_changes();
    assert!(matches!(
        session.complete_segment(&mut store, segment),
        Err(crate::ExecutionSegmentCompletionError::NotAtBoundary { .. })
    ));
    assert_eq!(store.scene_revision(), revision);
    assert_eq!(session.frame(), &frame);
    assert_eq!(session.painter_order(), painter_order);
    assert!(session.take_frame_changes().is_empty());'''
    assert tests.count(old) == 1
    tests = tests.replace(old, new, 1)
    old = '    let completed = session.publication_context();\n    session.complete_segment(&mut store, segment).unwrap();\n    assert_eq!(session.publication_context(), completed);\n    session.take_frame_changes();'
    new = '''    assert_eq!(store.scene_revision(), revision.checked_next().unwrap());
    let completed = session.publication_context();
    session.take_frame_changes();
    session.complete_segment(&mut store, segment).unwrap();
    assert_eq!(session.publication_context(), completed);
    assert!(session.take_frame_changes().is_empty());'''
    assert tests.count(old) == 1
    tests = tests.replace(old, new, 1)
    path.write_text(prefix + tests)
elif sys.argv[1] == '--production':
    path = Path('crates/noon/src/execution_session/completion.rs')
    text = path.read_text()
    start = text.index('        if family_transform.is_some() {\n            let root =')
    end = text.index('            self.apply_prepared_scalar_timeline_transaction_with_execution_at_root(', start)
    text = text[:start] + '''        // Completion can restructure display membership even without an unequal
        // Transform. Carry the declaration's validated root through the existing
        // publication boundary; explicit family replacement also supplies its root.
        // The publication layer still rejects unrooted or foreign-root reorders.
        let order_root = match family_transform {
            Some(completion) => Some((*lifecycle_root).ok_or(
                ExecutionSegmentCompletionError::MissingFamilyTransformRoot(completion.source),
            )?),
            None => (*lifecycle_root)
                .or_else(|| segment.family_replacement().map(|replacement| replacement.root)),
        };
        if let Some(root) = order_root {
''' + text[end:]
    path.write_text(text)
    path = Path('crates/noon-core/src/semantic_store/semantic_scene_restructure/foreground_tests.rs')
    path.write_text(path.read_text().rstrip() + r'''

#[test]
fn lifecycle_membership_keeps_unrelated_roots_out_of_the_plan() {
    let mut store = SemanticStore::new();
    let source = object(&mut store);
    let target = object(&mut store);
    let front = object(&mut store);
    let unrelated = (0..2_000).map(|_| object(&mut store)).collect::<Vec<_>>();
    let root = family(&mut store, &unrelated);
    edit(&mut store, root, SemanticSceneMembershipRequest::Add(&[source]));
    edit(&mut store, root, SemanticSceneMembershipRequest::AddForeground(&[front]));
    let mut transaction = SemanticMutationTransaction::new();
    stage_semantic_scene_lifecycle_membership(
        &store, root, &[source], &[target], &mut transaction,
    ).unwrap();
    // Remove source, add target, and place target/front; no unrelated root edit.
    assert_eq!(transaction.mutations().len(), 4);
    transaction.apply(&mut store).unwrap();
    let expected = unrelated.iter().copied().chain([target, front]).collect::<Vec<_>>();
    assert_lists(&store, root, &expected, &[front]);
}

#[test]
fn lifecycle_membership_rejects_stale_admission_without_staging_removal() {
    let mut store = SemanticStore::new();
    let source = object(&mut store);
    let stale = object(&mut store);
    let root = family(&mut store, &[source]);
    edit(&mut store, root, SemanticSceneMembershipRequest::AddForeground(&[source]));
    let mut deletion = SemanticMutationTransaction::new();
    deletion.remove_node(stale);
    deletion.apply(&mut store).unwrap();
    let revision = store.scene_revision();
    let counters = store.last_mutation_stats();
    let mut transaction = SemanticMutationTransaction::new();
    assert!(stage_semantic_scene_lifecycle_membership(
        &store, root, &[source], &[stale], &mut transaction,
    ).is_err());
    assert!(transaction.mutations().is_empty());
    assert_lists(&store, root, &[source], &[source]);
    assert_eq!(store.scene_revision(), revision);
    assert_eq!(store.last_mutation_stats(), counters);
}
''' )
else:
    raise SystemExit('expected --regressions or --production')
