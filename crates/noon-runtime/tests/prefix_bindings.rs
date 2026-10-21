#![cfg(feature = "prepared-prefix")]

use noon_compile::{
    CompilePatchError, CompiledObject, CompiledScene, ExecutionMutationTransaction, ExecutionPatch,
};
use noon_core::{Color, GeometryRef, ObjectId, SemanticClickIndicate, Style, Transform2D, Vec2};
use noon_runtime::{PrefixError, ReplayLimits, RuntimePatchStats, SceneInstance};
use std::sync::Arc;

fn object(id: u64, binding: Option<SemanticClickIndicate>) -> CompiledObject {
    let mut object = CompiledObject::new(
        ObjectId::new(id),
        GeometryRef::circle(0.25),
        Transform2D::IDENTITY,
        Style {
            fill: Some(Color::BLUE),
            ..Style::default()
        },
    );
    object.click_indicate = binding.map(Arc::new);
    object
}

fn binding(scale: f64, color: Color) -> SemanticClickIndicate {
    SemanticClickIndicate::new(scale, color, 2.0)
}

fn source(objects: Vec<CompiledObject>) -> SceneInstance {
    let mut source = SceneInstance::new(CompiledScene::compile_objects(objects, &[]).unwrap());
    source
        .begin_replay_retention(ReplayLimits {
            revisions: 64,
            payloads: 256,
        })
        .unwrap();
    source
}

fn set_binding(source: &mut SceneInstance, id: u64, binding: Option<SemanticClickIndicate>) {
    source
        .apply_execution_patch(&ExecutionPatch::SetClickIndicate {
            object: ObjectId::new(id),
            binding,
        })
        .unwrap();
}

#[test]
fn binding_only_publication_is_sparse_and_does_not_dirty_rendered_rows() {
    let mut costs = Vec::new();
    for count in [1, 1_001] {
        let mut source = source((0..count).map(|id| object(id, None)).collect());
        source.advance_to(1.0).unwrap();
        let (mut publisher, mut reader) = source.create_prefix_reader().unwrap();
        reader.seek(0.5).unwrap();
        let displayed = reader.frame().clone();
        let reader_context = reader.take_renderer_publication().context();
        let source_frame = source.frame().clone();
        let context = source.take_renderer_publication().context();
        let action = binding(1.5, Color::YELLOW);

        set_binding(&mut source, 0, Some(action));
        assert!(source.has_click_indicate_bindings());
        assert_eq!(
            source.click_indicate_binding(ObjectId::new(0)),
            Some(action)
        );
        assert_eq!(source.frame(), &source_frame);
        assert_eq!(source.last_patch_stats(), RuntimePatchStats::default());
        let publication = source.take_renderer_publication();
        assert!(publication.changes().is_empty());
        assert_eq!(
            publication.context().scene_revision(),
            context.scene_revision()
        );
        assert_ne!(
            publication.context().execution_revision(),
            context.execution_revision()
        );
        let changed_context = publication.context();
        let history = source.replay_stats();
        set_binding(&mut source, 0, Some(action));
        assert_eq!(source.publication_context(), changed_context);
        assert_eq!(source.replay_stats(), history);

        source.advance_to(2.0).unwrap();
        let batch = source.prepare_prefix_update(&mut publisher).unwrap();
        costs.push(batch.stats());
        assert_eq!(batch.stats().revisions, 1);
        assert_eq!(batch.stats().row_payloads, 1);
        assert_eq!(batch.stats().new_object_slots, 0);
        reader.append(batch).unwrap();
        assert_eq!(reader.frame(), &displayed);
        assert_eq!(reader.take_renderer_publication().context(), reader_context);
        assert!(reader
            .prepare_bound_click_indicate(ObjectId::new(0))
            .unwrap()
            .is_none());

        reader.advance_to(1.5).unwrap();
        let prepared = reader
            .prepare_bound_click_indicate(ObjectId::new(0))
            .unwrap()
            .unwrap();
        reader.start_transient_animation(prepared).unwrap();
        reader.advance_interactions(10.0).unwrap();
        reader.advance_interactions(11.0).unwrap();
        assert_eq!(
            reader.frame().objects[0].transform.scale,
            Vec2::new(1.5, 1.5)
        );
        assert_eq!(reader.frame().objects[0].style.fill, Some(Color::YELLOW));
    }
    assert_eq!(
        costs[0], costs[1],
        "binding transfer cost must not depend on unrelated rows"
    );
}

#[test]
fn input_interest_tracks_binding_revisions_removal_and_reentry_in_both_directions() {
    let a = binding(1.25, Color::YELLOW);
    let b = binding(1.75, Color::RED);
    let mut source = source(vec![object(0, None), object(1, None)]);
    source.advance_to(1.0).unwrap();
    set_binding(&mut source, 0, Some(a));
    set_binding(&mut source, 1, Some(b));
    source.advance_to(2.0).unwrap();
    set_binding(&mut source, 0, None);
    source.advance_to(3.0).unwrap();
    source
        .apply_execution_patch(&ExecutionPatch::RemoveObject(ObjectId::new(1)))
        .unwrap();
    source.advance_to(4.0).unwrap();
    source
        .apply_execution_patch(&ExecutionPatch::CreateObject(object(1, Some(a))))
        .unwrap();
    source.advance_to(5.0).unwrap();
    set_binding(&mut source, 1, None);
    source.advance_to(6.0).unwrap();
    source.seal_replay().unwrap();

    for (time, first, second) in [
        (4.5, None, Some(a)),
        (1.5, Some(a), Some(b)),
        (0.5, None, None),
        (2.5, None, Some(b)),
        (5.5, None, None),
        (3.5, None, None),
        (4.5, None, Some(a)),
        (0.5, None, None),
    ] {
        source.seek(time).unwrap();
        assert_eq!(source.click_indicate_binding(ObjectId::new(0)), first);
        assert_eq!(source.click_indicate_binding(ObjectId::new(1)), second);
        assert_eq!(
            source.has_click_indicate_bindings(),
            first.is_some() || second.is_some()
        );
    }
}

#[test]
fn appended_bound_slot_is_unavailable_until_its_creation_revision_is_displayed() {
    let mut source = source(vec![object(0, None)]);
    source.advance_to(1.0).unwrap();
    let (mut publisher, mut reader) = source.create_prefix_reader().unwrap();
    reader.seek(0.5).unwrap();
    reader.reserve_object_capacity(2).unwrap();
    let displayed = reader.frame().clone();
    let context = reader.take_renderer_publication().context();
    source
        .apply_execution_patch(&ExecutionPatch::CreateObject(object(
            1,
            Some(binding(1.75, Color::RED)),
        )))
        .unwrap();
    source.advance_to(2.0).unwrap();
    reader
        .append(source.prepare_prefix_update(&mut publisher).unwrap())
        .unwrap();
    drop(source);
    drop(publisher);

    assert_eq!(reader.frame(), &displayed);
    assert!(matches!(
        reader.prepare_bound_click_indicate(ObjectId::new(1)),
        Err(PrefixError::Interaction(_))
    ));
    assert!(matches!(
        reader.prepare_bound_click_indicate(ObjectId::new(99)),
        Err(PrefixError::Interaction(_))
    ));
    assert_eq!(reader.frame(), &displayed);
    assert_eq!(reader.take_renderer_publication().context(), context);
    reader.advance_to(1.5).unwrap();
    let prepared = reader
        .prepare_bound_click_indicate(ObjectId::new(1))
        .unwrap()
        .unwrap();
    reader.start_transient_animation(prepared).unwrap();
    reader.advance_interactions(10.0).unwrap();
    reader.advance_interactions(11.0).unwrap();
    assert_eq!(
        reader.frame().objects[1].transform.scale,
        Vec2::new(1.75, 1.75)
    );
    assert_eq!(reader.frame().objects[1].style.fill, Some(Color::RED));
    reader.seek(0.5).unwrap();
    assert!(!reader.interactions_active());
    assert!(matches!(
        reader.prepare_bound_click_indicate(ObjectId::new(1)),
        Err(PrefixError::Interaction(_))
    ));
    assert!(reader
        .prepare_bound_click_indicate(ObjectId::new(0))
        .unwrap()
        .is_none());
}

#[test]
fn rejected_binding_edits_preserve_history_and_an_active_effect() {
    let action = binding(1.5, Color::YELLOW);
    let mut source = source(vec![object(0, Some(action))]);
    source.advance_to(1.0).unwrap();
    let baseline = source.frame().clone();
    let prepared = source
        .prepare_click_indicate(ObjectId::new(0), action)
        .unwrap()
        .unwrap();
    source.start_transient_animation(prepared).unwrap();
    source.advance_interactions(10.0).unwrap();
    source.advance_interactions(11.0).unwrap();
    let highlighted = source.frame().clone();
    let context = source.take_renderer_publication().context();
    let history = source.replay_stats();
    assert!(matches!(
        source.apply_execution_patch(&ExecutionPatch::SetClickIndicate {
            object: ObjectId::new(0),
            binding: Some(binding(f64::NAN, Color::RED)),
        }),
        Err(CompilePatchError::InvalidClickIndicate(_))
    ));
    let transaction = ExecutionMutationTransaction::from_mutations([
        ExecutionPatch::SetClickIndicate {
            object: ObjectId::new(0),
            binding: None,
        },
        ExecutionPatch::SetStyle {
            object: ObjectId::new(99),
            style: Style::default(),
        },
    ]);
    assert!(source.apply_execution_transaction(&transaction).is_err());
    assert_eq!(source.frame(), &highlighted);
    assert_eq!(source.replay_stats(), history);
    assert_eq!(
        source.click_indicate_binding(ObjectId::new(0)),
        Some(action)
    );
    assert!(source.has_click_indicate_bindings());
    assert!(source.interactions_active());
    let publication = source.take_renderer_publication();
    assert_eq!(publication.context(), context);
    assert!(publication.changes().is_empty());
    source.advance_interactions(12.0).unwrap();
    assert_eq!(source.frame(), &baseline);
}
