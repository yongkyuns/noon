use noon_compile::{CompiledObject, CompiledScene, ExecutionPatch};
use noon_core::{
    CompositionTimeMap, GeometryRef, ObjectId, Property, Style, TrackDefinition, TrackId,
    TrackTiming, TrackValues, Transform2D,
};
use noon_runtime::SceneInstance;

fn object(id: u64, priority: f64) -> CompiledObject {
    let mut object = CompiledObject::new(
        ObjectId::new(id),
        GeometryRef::circle(1.0),
        Transform2D::IDENTITY,
        Style::default(),
    );
    object.base_z_index = priority;
    object
}
fn event(id: u64, time: f64, from: f64, to: f64) -> TrackDefinition {
    TrackDefinition {
        id: TrackId::new(id),
        object: ObjectId::new(1),
        property: Property::ZIndex,
        values: TrackValues::ZIndex { from, to },
        timing: TrackTiming::instant(time),
        time_map: CompositionTimeMap::identity(),
    }
}
fn runtime() -> SceneInstance {
    SceneInstance::new(
        CompiledScene::compile_objects(
            vec![
                object(0, 0.0),
                object(1, 0.0),
                object(2, 0.0),
                object(3, 2.0),
            ],
            &[
                event(0, 1.0, 0.0, 1.0000000000000002),
                event(1, 2.0, 1.0000000000000002, -1.0),
            ],
        )
        .unwrap(),
    )
}
#[test]
fn exact_discrete_order_agrees_for_forward_seek_and_prepared_publications() {
    let mut forward = runtime();
    let mut seek = runtime();
    let mut staged = runtime();
    for time in [0.0, 0.5, 1.0, 1.5, 2.0, 4.0] {
        forward.advance_to(time).unwrap();
        seek.seek(time).unwrap();
        let prepared = staged.prepare_advance_to(time).unwrap();
        let effective = staged.prepare_effective_property_batch(&[]).unwrap();
        staged.commit_prepared_frame(prepared, effective).unwrap();
        assert_eq!(forward.frame(), seek.frame());
        assert_eq!(forward.frame(), staged.frame());
        assert_eq!(forward.painter_order(), seek.painter_order());
        assert_eq!(forward.painter_order(), staged.painter_order());
        let expected: &[u32] = if time < 1.0 {
            &[0, 1, 2, 3]
        } else if time < 2.0 {
            &[0, 2, 1, 3]
        } else {
            &[1, 0, 2, 3]
        };
        assert_eq!(forward.painter_order(), expected);
        for (rank, &slot) in expected.iter().enumerate() {
            assert_eq!(forward.painter_rank(slot as usize), Some(rank as u32));
        }
    }
    seek.seek(1.0).unwrap();
    assert_eq!(seek.frame().objects[1].z_index, 1.0000000000000002);
}
#[test]
fn priority_event_dirties_only_its_row_and_crossed_order_interval() {
    let mut runtime = runtime();
    runtime.take_frame_changes();
    runtime.advance_to(0.9).unwrap();
    assert!(runtime.take_frame_changes().is_empty());
    runtime.advance_to(1.0).unwrap();
    let changes = runtime.take_frame_changes();
    assert_eq!(changes.painter_order_range(), Some(1..3));
    assert_eq!(changes.object_indices(), &[1]);
    runtime.advance_to(1.5).unwrap();
    assert!(runtime.take_frame_changes().is_empty());
    assert_eq!(runtime.last_stats().groups_evaluated, 0);
}
#[test]
fn structure_and_authored_priority_edits_preserve_effective_sorting() {
    let mut runtime = runtime();
    runtime.advance_to(1.0).unwrap();
    runtime
        .apply_execution_patch(&ExecutionPatch::SetZIndex {
            object: ObjectId::new(0),
            value: 3.0,
        })
        .unwrap();
    assert_eq!(runtime.painter_order(), &[2, 1, 3, 0]);
    runtime
        .apply_execution_patch(&ExecutionPatch::RemoveObject(ObjectId::new(2)))
        .unwrap();
    assert_eq!(runtime.painter_order(), &[1, 3, 0]);
    assert_eq!(runtime.painter_rank(2), None);
    runtime
        .apply_execution_patch(&ExecutionPatch::CreateObject(object(4, 1.5)))
        .unwrap();
    assert_eq!(runtime.painter_order(), &[1, 4, 3, 0]);
}
#[test]
fn negative_zero_keeps_family_ties_and_non_finite_priority_is_rejected() {
    let runtime = SceneInstance::new(
        CompiledScene::compile_objects(vec![object(0, 0.0), object(1, -0.0)], &[]).unwrap(),
    );
    assert_eq!(runtime.painter_order(), &[0, 1]);
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(CompiledScene::compile_objects(
            vec![object(0, 0.0), object(1, 0.0)],
            &[event(0, 1.0, 0.0, value)]
        )
        .is_err());
    }
}
