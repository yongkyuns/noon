from pathlib import Path
import sys

ROOT = Path.cwd()

def rewrite(path, old, new):
    p = ROOT / path
    text = p.read_text()
    count = text.count(old)
    if count != 1:
        raise RuntimeError(f'{path}: expected exactly one anchor, found {count}: {old[:100]!r}')
    p.write_text(text.replace(old, new, 1))

def append(path, text, marker):
    p = ROOT / path
    if marker in p.read_text():
        raise RuntimeError(f'{path}: test marker already present: {marker}')
    p.write_text(p.read_text().rstrip() + '\n\n' + text.strip() + '\n')

LIVE_TESTS = r'''
fn assert_matching_foreground_completion(source_is_foreground: bool) {
    use noon_core::{plan_semantic_scene_membership, SemanticSceneMembershipRequest};

    let mut store = SemanticStore::new();
    let back = object(&mut store, -4.0);
    let source_leaf = matching_path_object(&mut store, matching_triangle(), 0.0);
    let source = family(&mut store, &[source_leaf]);
    let front = object(&mut store, 4.0);
    let target_leaf = matching_path_object(&mut store, matching_triangle(), 0.0);
    let target = family(&mut store, &[target_leaf]);
    let later = object(&mut store, 6.0);
    let root = family(&mut store, &[back, source, front]);
    let foreground = if source_is_foreground {
        vec![source, front]
    } else {
        vec![front]
    };
    plan_semantic_scene_membership(
        &store, root, SemanticSceneMembershipRequest::AddForeground(&foreground),
    ).unwrap().apply(&mut store).unwrap();
    let mut session = ExecutionSession::from_semantic_root(&store, root).unwrap();
    let source_id = session.execution_index.execution_object_id(source_leaf).unwrap();
    let front_id = session.execution_index.execution_object_id(front).unwrap();
    let request = SemanticCompositionRequest::MatchingFamilyTransformTo {
        source,
        target_state: target,
        options: AnimationOptions::new().run_time(1.0).rate_func(RateFunction::Linear),
    };
    let segment = session.declare_and_activate_composition(
        &mut store, root, &request, AnimationOptions::new(),
    ).unwrap();
    session.advance_segment_to(segment, 0.5).unwrap();
    let publication = session.publication_context();
    assert!(session.complete_segment(&mut store, segment).is_err());
    assert_eq!(session.publication_context(), publication);
    assert_eq!(store.node(root).unwrap().foreground_members(), foreground);

    session.advance_segment_to(segment, segment.end_time()).unwrap();
    session.complete_segment(&mut store, segment).unwrap();
    assert_eq!(store.node(root).unwrap().members(), [back, target, front]);
    assert_eq!(store.node(root).unwrap().foreground_members(), [front]);
    assert_eq!(store.node(source).unwrap().members(), [source_leaf]);
    assert_eq!(store.node(target).unwrap().members(), [target_leaf]);
    assert!(session.runtime.frame_index_for_object(source_id).is_none());
    let ids = session.painter_order().iter()
        .map(|&index| session.frame().objects[index as usize].id)
        .collect::<Vec<_>>();
    assert_eq!(ids, vec![
        session.execution_index.execution_object_id(back).unwrap(),
        session.execution_index.execution_object_id(target_leaf).unwrap(),
        front_id,
    ]);
    assert!(session.frame().objects.iter().all(|row| row.z_index == 0.0));
    let completed = session.publication_context();
    session.complete_segment(&mut store, segment).unwrap();
    assert_eq!(session.publication_context(), completed);
    session.take_frame_changes();
    let add = plan_semantic_scene_membership(
        &store, root, SemanticSceneMembershipRequest::Add(&[later]),
    ).unwrap();
    session.apply_semantic_transaction(&mut store, add).unwrap();
    assert_eq!(store.node(root).unwrap().members(), [back, target, later, front]);
    assert_eq!(store.node(root).unwrap().foreground_members(), [front]);
    assert!(session.runtime.frame_index_for_object(source_id).is_none());
}

#[test]
fn matching_foreground_completion_keeps_surviving_foreground_after_target() {
    assert_matching_foreground_completion(false);
}

#[test]
fn matching_foreground_completion_retires_source_without_promoting_target() {
    assert_matching_foreground_completion(true);
}
'''

CORE_TESTS = r'''
#[test]
fn lifecycle_membership_combines_removals_and_foreground_aware_admission() {
    let mut store = SemanticStore::new();
    let back = object(&mut store);
    let retired_leaf = object(&mut store);
    let retired = family(&mut store, &[retired_leaf]);
    let sibling = object(&mut store);
    let group = family(&mut store, &[retired, sibling]);
    let front = object(&mut store);
    let added = object(&mut store);
    let root = family(&mut store, &[back]);
    edit(&mut store, root, SemanticSceneMembershipRequest::AddForeground(&[group, front]));
    let revision = store.scene_revision();
    let mut transaction = SemanticMutationTransaction::new();
    stage_semantic_scene_lifecycle_membership(
        &store, root, &[retired], &[added], &mut transaction,
    ).unwrap();
    assert_lists(&store, root, &[back, group, front], &[group, front]);
    // Preparing and dropping the complete change cannot demote persistence early.
    drop(transaction.prepare(&mut store).unwrap());
    assert_eq!(store.scene_revision(), revision);
    assert_lists(&store, root, &[back, group, front], &[group, front]);
    let mut transaction = SemanticMutationTransaction::new();
    stage_semantic_scene_lifecycle_membership(
        &store, root, &[retired], &[added], &mut transaction,
    ).unwrap();
    transaction.apply(&mut store).unwrap();
    assert_lists(&store, root, &[back, added, sibling, front], &[sibling, front]);
    assert_eq!(store.scene_revision(), revision.checked_next().unwrap());
    assert_eq!(store.node(group).unwrap().members(), &[retired, sibling]);
}

#[test]
fn lifecycle_membership_removal_without_admission_does_not_reorder_survivors() {
    let mut store = SemanticStore::new();
    let retired = object(&mut store);
    let front = object(&mut store);
    let ordinary = object(&mut store);
    let root = family(&mut store, &[retired, front, ordinary]);
    let mut declaration = SemanticMutationTransaction::new();
    declaration.set_foreground_members(root, [retired, front]);
    declaration.apply(&mut store).unwrap();
    let mut transaction = SemanticMutationTransaction::new();
    stage_semantic_scene_lifecycle_membership(
        &store, root, &[retired], &[], &mut transaction,
    ).unwrap();
    transaction.apply(&mut store).unwrap();
    assert_lists(&store, root, &[front, ordinary], &[front]);
}

#[test]
fn lifecycle_membership_admission_reuses_an_existing_target_edge() {
    let mut store = SemanticStore::new();
    let source = object(&mut store);
    let target = object(&mut store);
    let front = object(&mut store);
    let root = family(&mut store, &[source, target]);
    edit(&mut store, root, SemanticSceneMembershipRequest::AddForeground(&[front]));
    let mut transaction = SemanticMutationTransaction::new();
    stage_semantic_scene_lifecycle_membership(
        &store, root, &[source], &[target], &mut transaction,
    ).unwrap();
    transaction.apply(&mut store).unwrap();
    assert_lists(&store, root, &[target, front], &[front]);
}
'''

STAGE_HELPER = r'''
/// Stage one lifecycle boundary's removals followed by foreground-aware admission.
///
/// All targets for a scope are planned together so membership restructuring,
/// persistence cleanup and painter order publish in the caller's single transaction.
/// Removal-only boundaries preserve the order of surviving display members. An
/// admission uses the same family projection as Scene.add, with only the surviving
/// foreground declarations at its tail. No temporary store or second order is owned.
pub fn stage_semantic_scene_lifecycle_membership(
    store: &SemanticStore,
    scene_root: SemanticNodeId,
    removed: &[SemanticNodeId],
    added: &[SemanticNodeId],
    transaction: &mut SemanticMutationTransaction,
) -> Result<(), SemanticSceneOperationError> {
    let root = target_node_checked(store, scene_root)?;
    if !matches!(root.kind(), SemanticNodeKind::Family(_)) {
        return Err(SemanticSceneOperationError::NotSemanticFamily(scene_root));
    }
    let removed = validated_distinct_nodes(store, removed)?;
    let added = validated_distinct_nodes(store, added)?;
    if removed.is_empty() && added.is_empty() {
        return Ok(());
    }
    let members = if root.foreground_members().is_empty() || removed.is_empty() {
        root.foreground_members().to_vec()
    } else {
        let removal = downward_target_closure(store, &removed)?;
        foreground::project_members(store, scene_root, &removal, None)?
    };
    let explicit = if added.is_empty() {
        Vec::new()
    } else {
        foreground::add_order(&added, &members)
    };
    let mut remove_set = downward_target_closure(store, &explicit)?;
    remove_set.extend(removed);
    stage_explicit_root_projection(
        store, scene_root, &remove_set, None, &explicit, ExplicitPlacement::Tail, transaction,
    )?;
    if members != root.foreground_members() {
        transaction.set_foreground_members(scene_root, members);
    }
    Ok(())
}
'''

if sys.argv[1] == '--regressions':
    append('crates/noon/src/execution_session/family_transform_tests.rs', LIVE_TESTS, 'fn assert_matching_foreground_completion(')
elif sys.argv[1] == '--production':
    path = 'crates/noon-core/src/semantic_store/semantic_scene_restructure.rs'
    p = ROOT / path
    text = p.read_text()
    start = text.index('fn plan_explicit_root_projection(')
    end = text.index('\nfn first_projected_root_after_restructure(', start)
    body = text[start:end]
    signature, rest = body.split('    let (affected, affected_roots)', 1)
    staged_sig = signature.replace('fn plan_explicit_root_projection(', 'fn stage_explicit_root_projection(').replace(
        '    placement: ExplicitPlacement,\n) -> Result<SemanticMutationTransaction, SemanticSceneOperationError>',
        '    placement: ExplicitPlacement,\n    transaction: &mut SemanticMutationTransaction,\n) -> Result<(), SemanticSceneOperationError>'
    )
    if staged_sig == signature or rest.count('    let mut transaction = SemanticMutationTransaction::new();\n') != 1 or rest.count('    Ok(transaction)') != 1:
        raise RuntimeError('Unexpected root projection implementation')
    rest = rest.replace('    let mut transaction = SemanticMutationTransaction::new();\n', '', 1).replace('    Ok(transaction)', '    Ok(())', 1)
    wrapper = signature + '''    let mut transaction = SemanticMutationTransaction::new();
    stage_explicit_root_projection(
        store, scene_root, remove_set, replacement, explicit, placement, &mut transaction,
    )?;
    Ok(transaction)
}

'''
    p.write_text(text[:start] + STAGE_HELPER.strip() + '\n\n' + wrapper + staged_sig + '    let (affected, affected_roots)' + rest + text[end:])
    rewrite('crates/noon-core/src/semantic_store/semantic_scene_restructure/foreground.rs', 'fn project_members(\n', 'pub(super) fn project_members(\n')
    # Keep the existing source formatting independent of adjacent export additions.
    rewrite('crates/noon-core/src/semantic_store.rs',
            'stage_semantic_foreground_removal,',
            'stage_semantic_foreground_removal, stage_semantic_scene_lifecycle_membership,')

    path = 'crates/noon/src/execution_session/family_transform.rs'
    rewrite(path, 'pub(super) fn stage_matching_family_completion_swap(', 'pub(super) fn validate_matching_family_completion_swap(')
    rewrite(path, '    semantic: &mut SemanticMutationTransaction,\n) -> Result<(), MatchingFamilyCompletionSwapError>', ') -> Result<(), MatchingFamilyCompletionSwapError>')
    rewrite(path, '    semantic.remove_member(execution_root, source_root);\n    semantic.add_member(execution_root, target_root);\n    Ok(())', '    Ok(())')
    rewrite(path, 'use noon_core::{SemanticMutationTransaction, SemanticNodeId, SemanticStore};', 'use noon_core::{SemanticNodeId, SemanticStore};')
    rewrite(path, '/// Stage the exact-end source-family -> target-family replacement without publishing it.', '/// Validate the exact-end source-family -> target-family replacement topology.')
    rewrite(path, '/// target is appended after the surviving root members, matching Manim\'s cleanup-time\n/// remove-source / add-target scene ordering.', '/// membership and foreground-aware ordering are staged separately with all other\n/// removals at the same completion boundary.')
    rewrite(path, '    use noon_core::{SemanticObjectState, StoredGeometry};\n\n    use super::*;', '''    use noon_core::{SemanticMutationTransaction, SemanticObjectState, StoredGeometry};

    use super::*;

    fn stage_matching_family_completion_swap(
        store: &SemanticStore,
        root: SemanticNodeId,
        source: SemanticNodeId,
        target: SemanticNodeId,
        transaction: &mut SemanticMutationTransaction,
    ) -> Result<(), MatchingFamilyCompletionSwapError> {
        validate_matching_family_completion_swap(store, root, source, target)?;
        noon_core::stage_semantic_scene_lifecycle_membership(
            store, root, &[source], &[target], transaction,
        ).unwrap();
        Ok(())
    }''')

    path = 'crates/noon/src/execution_session.rs'
    rewrite(path, '                let mut completion = SemanticMutationTransaction::new();\n                family_transform::stage_matching_family_completion_swap(', '                family_transform::validate_matching_family_completion_swap(')
    rewrite(path, '                    *target_state,\n                    &mut completion,\n', '                    *target_state,\n')
    rewrite(path, '                // The scratch transaction is discarded; this validates only.\n', '                // This validates only; completion stages one combined membership edit.\n')

    path = 'crates/noon/src/execution_session/completion.rs'
    rewrite(path, 'stage_semantic_foreground_removal, ReactiveValue, SemanticFadeDirection,', 'stage_semantic_scene_lifecycle_membership, ReactiveValue, SemanticFadeDirection,')
    rewrite(path, '            semantic.remove_member(root, target);\n            foreground_removals.entry(root).or_default().insert(target);', '            foreground_removals.entry(root).or_default().insert(target);')
    rewrite(path, '            super::family_transform::stage_matching_family_completion_swap(', '            super::family_transform::validate_matching_family_completion_swap(')
    rewrite(path, '                replacement.target,\n                &mut semantic,\n', '                replacement.target,\n')
    rewrite(path, '                semantic.remove_member(root, entry.semantic_object);\n', '')
    rewrite(path, '''        // One declaration edit per scope, staged with endpoint release and display
        // removals. A failed completion never leaves a partially demoted foreground.
        for (root, removed) in foreground_removals {
            stage_semantic_foreground_removal(
                store,
                root,
                &removed.into_iter().collect::<Vec<_>>(),
                &mut semantic,
            )
            .map_err(ExecutionSegmentCompletionError::ForegroundMembership)?;
        }''', '''        // One combined display/declaration edit per scope. Matching cleanup's
        // authored target is admitted after removals but before surviving foreground.
        // Planning separate add/remove transactions could resurrect a retired source
        // declaration or write the same membership edge twice.
        for (root, removed) in foreground_removals {
            let added = segment.family_replacement()
                .filter(|replacement| replacement.root == root)
                .map(|replacement| replacement.target);
            stage_semantic_scene_lifecycle_membership(
                store,
                root,
                &removed.into_iter().collect::<Vec<_>>(),
                added.as_slice(),
                &mut semantic,
            )
            .map_err(ExecutionSegmentCompletionError::ForegroundMembership)?;
        }''')
    # The collection now owns all completion membership, not merely foreground metadata.
    p = ROOT / path
    text = p.read_text().replace('foreground_removals', 'membership_removals')
    p.write_text(text)
    append('crates/noon-core/src/semantic_store/semantic_scene_restructure/foreground_tests.rs', CORE_TESTS, 'fn lifecycle_membership_combines_removals_')
else:
    raise SystemExit('expected --regressions or --production')
