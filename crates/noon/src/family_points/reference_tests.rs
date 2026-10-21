//! Pinned CE observations are an explicit golden boundary, not an engine protocol.
use super::*;
use crate::{CompositeEntryHandle, Scene};
use serde_json::{json, Value};
use std::collections::BTreeMap;

const POINTS: [(f64, f64); 5] = [(0., 0.), (0.1, 0.), (0.2, 0.3), (0.3, 0.4), (0.5, 0.6)];

fn identity(entry: &CompositeEntryHandle) -> noon_core::SemanticNodeId {
    entry.as_target().node_id()
}

fn snapshot(group: &MobjectFamily, names: &BTreeMap<noon_core::SemanticNodeId, &str>) -> Value {
    let split = group
        .split_members()
        .unwrap()
        .iter()
        .map(|entry| names[&identity(entry)])
        .collect::<Vec<_>>();
    let children = group
        .integration_store()
        .borrow()
        .semantic_family_checked(group.node_id())
        .unwrap()
        .members()
        .into_iter()
        .map(|id| names[&id])
        .collect::<Vec<_>>();
    json!({"points":group.get_num_points().unwrap(),"length":group.split_count().unwrap(),"iteration":split,"split":split,"children":children})
}

#[test]
fn pinned_own_points_and_scalar_index_callbacks_match_for_both_family_classes() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../fixtures/manim-v0.21/family-points.json"
    ))
    .unwrap();
    assert_eq!(fixture["manimVersion"], "0.21.0");
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 336);
    for case in cases {
        let spec = &case["spec"];
        let mut scene = Scene::new();
        let a = scene.circle(0.2).unwrap();
        let b = scene.square(0.3).unwrap();
        let extra = scene.square(0.4).unwrap();
        let group = scene.family(&[(&a).into(), (&b).into()]).unwrap();
        let names = BTreeMap::from([
            (group.node_id(), "self"),
            (a.node_id(), "a"),
            (b.node_id(), "b"),
            (extra.node_id(), "extra"),
        ]);
        let semantic_count = scene.integration_store().borrow().len();
        group
            .set_points_f64(&POINTS[..spec["ownPoints"].as_u64().unwrap() as usize])
            .unwrap();
        let before = snapshot(&group, &names);
        let old_length = group.split_count().unwrap() as i64;
        let events = json!([{"length":old_length,"points":group.get_num_points().unwrap()}]);
        // This models the intervening authoring operation, not Python dispatch.
        // Python invokes __index__ and normalizes range(old_length); Rust reads
        // the current own-point predicate and current membership afterward.
        match spec["action"].as_str().unwrap() {
            "none" => {}
            "set_points" => group.set_points_f64(&POINTS[..4]).unwrap(),
            "clear_points" => group.set_points_f64(&[]).unwrap(),
            "remove_first" => {
                group.remove((&a).into()).unwrap();
            }
            "clear_children" => {
                group.remove_many(&[(&a).into(), (&b).into()]).unwrap();
            }
            "append_extra" => {
                group.add((&extra).into()).unwrap();
            }
            action => panic!("unknown reference action {action}"),
        }
        let requested = spec["index"].as_i64().unwrap();
        let normalized = if requested < 0 {
            old_length + requested
        } else {
            requested
        };
        let result = if !(0..old_length).contains(&normalized) {
            json!({"exception":"IndexError","message":"range object index out of range"})
        } else {
            match group.split_at(normalized as usize).unwrap() {
                Some(entry) => json!(names[&identity(&entry)]),
                None => json!({"exception":"IndexError","message":"list index out of range"}),
            }
        };
        let actual = json!({"spec":spec,"before":before,"result":result,"events":events,"after":snapshot(&group,&names)});
        assert_eq!(&actual, case, "spec: {spec}");
        assert_eq!(
            scene.integration_store().borrow().len(),
            semantic_count,
            "point/index operations allocated semantic nodes: {spec}"
        );
    }
}
