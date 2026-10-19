#![cfg(feature = "prepared-prefix")]
use noon_compile::{CompiledObject, CompiledScene, ExecutionMutationTransaction, ExecutionPatch};
use noon_core::{GeometryRef, ObjectId, Style, Transform2D, Vec2};
use noon_runtime::{FrameObjectState, PrefixError, PrefixReader, ReplayError, ReplayLimits, RendererPublication, SceneInstance};

fn object(id: u64) -> CompiledObject {
    CompiledObject::new(ObjectId::new(id), GeometryRef::circle(0.2), Transform2D::IDENTITY, Style::default())
}
fn source(count: usize) -> SceneInstance {
    let mut rt = SceneInstance::new(CompiledScene::compile_objects((0..count).map(|i| object(i as u64)).collect(), &[]).unwrap());
    rt.begin_replay_retention(ReplayLimits { revisions: 256, payloads: 2048 }).unwrap();
    rt.advance_to(1.0).unwrap();
    rt
}
fn visible(publication: RendererPublication<'_>) -> Vec<(FrameObjectState, bool, f32, f32)> {
    let frame = publication.frame();
    publication.painter_order().iter().map(|&i| {
        let i = i as usize;
        (frame.objects[i].clone(), frame.presences[i], frame.reveals[i], frame.morphs[i])
    }).collect()
}
fn remove(id: u64) -> ExecutionPatch { ExecutionPatch::RemoveObject(ObjectId::new(id)) }
fn order(id: u64, before: Option<u64>) -> ExecutionPatch {
    ExecutionPatch::ReorderObject { object: ObjectId::new(id), before: before.map(ObjectId::new) }
}
fn check(reader: &mut PrefixReader, time: f64, expected: &[(FrameObjectState, bool, f32, f32)]) {
    reader.seek(time).unwrap();
    assert_eq!(visible(reader.take_renderer_publication()), expected, "historical state at {time}");
}

#[test]
fn several_creates_removals_reorders_and_reentry_in_one_batch_preserve_every_interval() {
    let mut rt = source(3);
    let (mut publisher, mut reader) = rt.create_prefix_reader().unwrap();
    reader.reserve_object_capacity(12).unwrap();
    let old = visible(rt.take_renderer_publication());
    rt.advance_to(1.0).unwrap();
    rt.apply_execution_patch(&ExecutionPatch::CreateObject(object(3))).unwrap();
    rt.apply_execution_patch(&order(3, Some(1))).unwrap();
    rt.advance_to(1.5).unwrap();
    let first = visible(rt.take_renderer_publication());
    rt.advance_to(2.0).unwrap();
    rt.apply_execution_patch(&remove(1)).unwrap();
    rt.apply_execution_patch(&ExecutionPatch::CreateObject(object(4))).unwrap();
    rt.apply_execution_patch(&order(4, Some(0))).unwrap();
    rt.apply_execution_patch(&order(2, Some(3))).unwrap();
    rt.advance_to(2.5).unwrap();
    let second = visible(rt.take_renderer_publication());
    rt.advance_to(3.0).unwrap();
    rt.apply_execution_patch(&remove(3)).unwrap();
    rt.apply_execution_patch(&remove(4)).unwrap();
    rt.apply_execution_patch(&ExecutionPatch::CreateObject(object(1))).unwrap();
    rt.apply_execution_patch(&order(1, Some(2))).unwrap();
    rt.advance_to(3.5).unwrap();
    let third = visible(rt.take_renderer_publication());
    rt.advance_to(4.0).unwrap();
    let frame = reader.frame().clone();
    let previous = reader.take_renderer_publication().context();
    let batch = rt.prepare_prefix_update(&mut publisher).unwrap();
    assert_eq!(batch.stats().new_object_slots, 2);
    assert_eq!(batch.required_object_capacity(), 5);
    reader.append(batch).unwrap();
    assert_eq!(reader.frame(), &frame, "append cannot grow or rewrite displayed frame");
    let publication = reader.take_renderer_publication();
    assert_eq!(publication.context(), previous);
    assert!(publication.changes().is_empty());
    drop(rt);
    for _ in 0..10 {
        for (time, expected) in [(3.5, &third), (0.5, &old), (2.5, &second), (1.5, &first)] {
            check(&mut reader, time, expected);
        }
    }
}

#[test]
fn a_later_batch_reactivates_the_same_slot_without_replacing_old_identity() {
    let mut rt = source(2);
    let (mut publisher, mut reader) = rt.create_prefix_reader().unwrap();
    reader.reserve_object_capacity(3).unwrap();
    rt.apply_execution_patch(&remove(0)).unwrap();
    rt.advance_to(2.0).unwrap();
    reader.append(rt.prepare_prefix_update(&mut publisher).unwrap()).unwrap();
    reader.seek(1.5).unwrap();
    assert_eq!(reader.take_renderer_publication().painter_order(), &[1]);
    let mut reentry = object(0);
    reentry.content = GeometryRef::rectangle(0.3, 0.4).into();
    rt.apply_execution_patch(&ExecutionPatch::CreateObject(reentry)).unwrap();
    rt.apply_execution_patch(&order(0, Some(1))).unwrap();
    rt.advance_to(3.0).unwrap();
    let batch = rt.prepare_prefix_update(&mut publisher).unwrap();
    assert_eq!(batch.stats().new_object_slots, 0);
    reader.append(batch).unwrap();
    assert!(matches!(reader.seek(2.5).unwrap().objects[0].geometry(), Some(GeometryRef::Rectangle { .. })));
    assert_eq!(reader.take_renderer_publication().painter_order(), &[0,1]);
    assert!(matches!(reader.seek(0.5).unwrap().objects[0].geometry(), Some(GeometryRef::Circle { .. })));
}

#[test]
fn capacity_requires_an_explicit_bounded_maintenance_barrier() {
    let mut rt = source(1);
    let (mut publisher, mut reader) = rt.create_prefix_reader().unwrap();
    rt.apply_execution_patch(&ExecutionPatch::CreateObject(object(1))).unwrap();
    rt.advance_to(2.0).unwrap();
    let batch = rt.prepare_prefix_update(&mut publisher).unwrap();
    let frame = reader.frame().clone();
    assert_eq!(reader.append(batch), Err(PrefixError::CapacityRequired { object_slots: 2 }));
    assert_eq!(reader.frame(), &frame);
    assert_eq!(reader.frontier(), 1.0);
    assert!(matches!(reader.reserve_object_capacity(usize::MAX), Err(PrefixError::Replay(ReplayError::RetentionLimit))));
    // Transport retries are outside this slice; dropping a batch needs a new stream.
    let (mut publisher, mut reader) = rt.create_prefix_reader().unwrap();
    rt.apply_execution_patch(&ExecutionPatch::CreateObject(object(2))).unwrap();
    rt.advance_to(3.0).unwrap();
    let batch = rt.prepare_prefix_update(&mut publisher).unwrap();
    reader.reserve_object_capacity(batch.required_object_capacity()).unwrap();
    reader.append(batch).unwrap();
    reader.seek(2.5).unwrap();
    assert_eq!(reader.take_renderer_publication().painter_order().len(), 3);
}

#[test]
fn failed_creation_transaction_does_not_allocate_or_publish_a_historical_slot() {
    let mut rt = source(1);
    let (mut publisher, mut reader) = rt.create_prefix_reader().unwrap();
    let bad = ExecutionMutationTransaction::from_mutations([
        ExecutionPatch::CreateObject(object(1)),
        ExecutionPatch::SetTransform { object: ObjectId::new(1), transform: Transform2D { translation: Vec2::new(f32::NAN, 0.0), ..Transform2D::IDENTITY } },
    ]);
    assert!(rt.apply_execution_transaction(&bad).is_err());
    rt.advance_to(2.0).unwrap();
    let batch = rt.prepare_prefix_update(&mut publisher).unwrap();
    assert_eq!(batch.stats().new_object_slots, 0);
    assert_eq!(batch.stats().revisions, 0);
    reader.append(batch).unwrap();
    reader.seek(1.5).unwrap();
    assert_eq!(reader.take_renderer_publication().painter_order(), &[0]);
}

#[test]
fn content_replacements_do_not_collapse_intermediate_geometry() {
    let mut rt = source(1);
    let (mut publisher, mut reader) = rt.create_prefix_reader().unwrap();
    let mut expected = Vec::new();
    for step in 1..=4 {
        rt.apply_execution_patch(&ExecutionPatch::SetContent { object: ObjectId::new(0), content: GeometryRef::circle(step as f32).into(), text_bounds: None }).unwrap();
        rt.advance_to(step as f64 + 0.5).unwrap();
        expected.push((rt.frame().time, visible(rt.take_renderer_publication())));
        rt.advance_to(step as f64 + 1.0).unwrap();
    }
    reader.append(rt.prepare_prefix_update(&mut publisher).unwrap()).unwrap();
    for (time, expected) in expected.iter().rev() { check(&mut reader, *time, expected); }
}

#[test]
fn lifecycle_payload_scales_with_new_work_not_unrelated_objects() {
    let mut stats = Vec::new();
    for count in [10, 10_000] {
        let mut rt = source(count);
        let (mut publisher, mut reader) = rt.create_prefix_reader().unwrap();
        reader.reserve_object_capacity(count + 1).unwrap();
        rt.apply_execution_patch(&ExecutionPatch::CreateObject(object(count as u64))).unwrap();
        rt.apply_execution_patch(&remove(count as u64)).unwrap();
        rt.advance_to(2.0).unwrap();
        let batch = rt.prepare_prefix_update(&mut publisher).unwrap();
        assert_eq!(batch.stats().row_payloads, 2);
        assert_eq!(batch.stats().new_object_slots, 1);
        stats.push(batch.stats());
        reader.append(batch).unwrap();
        reader.seek(1.5).unwrap();
        assert_eq!(reader.take_renderer_publication().painter_order().len(), count);
    }
    assert_eq!(stats[0], stats[1]);
}
