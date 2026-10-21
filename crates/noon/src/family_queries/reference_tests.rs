//! Actual pinned CE controls exercised through direct typed Rust authoring.
use crate::Scene;
use serde_json::{json, Value};
use std::collections::BTreeMap;

#[test]
fn pinned_iterator_prefix_container_mutation_and_exhaustion_match() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../fixtures/manim-v0.21/family-iteration.json"
    ))
    .unwrap();
    assert_eq!(fixture["manimVersion"], "0.21.0");
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 320);
    let points = [(0., 0.), (1., 1.), (2., 0.), (3., 1.), (4., 0.)];
    for case in cases {
        let spec = &case["spec"];
        let mut scene = Scene::new();
        let a = scene.circle(0.2).unwrap();
        let b = scene.square(0.3).unwrap();
        let extra = scene.circle(0.4).unwrap();
        let post = scene.circle(0.5).unwrap();
        let root = scene.family(&[(&a).into(), (&b).into()]).unwrap();
        root.set_points_f64(&points[..spec["ownPoints"].as_u64().unwrap() as usize])
            .unwrap();
        let names = BTreeMap::from([
            (root.node_id(), "self"),
            (a.node_id(), "a"),
            (b.node_id(), "b"),
            (extra.node_id(), "extra"),
            (post.node_id(), "post"),
        ]);
        let ids_before = scene.integration_store().borrow().len();
        let mut iterator = root.split_iter().unwrap();
        let mut consumed = Vec::new();
        for _ in 0..spec["consumed"].as_u64().unwrap() {
            if let Some(value) = iterator.next() {
                consumed.push(names[&value.unwrap().as_target().node_id()]);
            } else {
                break;
            }
        }
        match spec["action"].as_str().unwrap() {
            "none" => {}
            "remove_first" => {
                root.remove((&a).into()).unwrap();
            }
            "clear_children" => {
                root.remove_many(&[(&a).into(), (&b).into()]).unwrap();
            }
            "append_extra" => {
                root.add((&extra).into()).unwrap();
            }
            "add_empty" => {
                root.add_many(&[]).unwrap();
            }
            "prepend_extra" => {
                root.prepend_many(&[(&extra).into()]).unwrap();
            }
            "insert_extra" => {
                root.insert_member(0, (&extra).into()).unwrap();
            }
            "reverse" => {
                root.invert(false).unwrap();
            }
            "set_points" => root.set_points_f64(&points[..4]).unwrap(),
            "clear_points" => root.set_points_f64(&[]).unwrap(),
            other => panic!("unknown reference action {other}"),
        }
        let remaining = iterator
            .by_ref()
            .map(|value| names[&value.unwrap().as_target().node_id()])
            .collect::<Vec<_>>();
        let split = root
            .split_members()
            .unwrap()
            .iter()
            .map(|value| names[&value.as_target().node_id()])
            .collect::<Vec<_>>();
        let children = (0..root.member_count().unwrap())
            .map(|i| names[&root.member_at(i).unwrap().unwrap().as_target().node_id()])
            .collect::<Vec<_>>();
        let after = json!({"points":root.get_num_points().unwrap(),"length":root.split_count().unwrap(),"split":split,"children":children});
        root.insert_member(0, (&post).into()).unwrap();
        let observed = json!({"spec":spec,"consumed":consumed,"remaining":remaining,"after":after,"staysExhausted":iterator.next().is_none()});
        assert_eq!(observed, *case, "{spec}");
        assert_eq!(scene.integration_store().borrow().len(), ids_before);
    }
}
