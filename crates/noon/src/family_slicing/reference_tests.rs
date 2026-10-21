//! Pinned CE is an external fixture; all selection uses typed native operations.
use crate::{AuthoringError, CompositeEntryHandle, MobjectFamily, Scene};
use serde_json::{json, Value};
use std::collections::BTreeMap;

#[test]
fn slice_capture_protocol_timing_and_root_mapping_match_pinned_ce() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../fixtures/manim-v0.21/family-slices.json"
    ))
    .unwrap();
    assert_eq!(fixture["manimVersion"], "0.21.0");
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 432);
    let points = [(0., 0.), (0.1, 0.), (0.2, 0.3), (0.3, 0.4)];
    for case in cases {
        let spec = &case["spec"];
        let mut scene = Scene::new();
        let a = scene.circle(0.2).unwrap();
        let b = scene.square(0.3).unwrap();
        let extra = scene.square(0.4).unwrap();
        let root = scene.family(&[(&a).into(), (&b).into()]).unwrap();
        root.set_points_f64(&points[..spec["ownPoints"].as_u64().unwrap() as usize])
            .unwrap();
        let names = BTreeMap::from([
            (root.node_id(), "self"),
            (a.node_id(), "a"),
            (b.node_id(), "b"),
            (extra.node_id(), "extra"),
        ]);
        let member_names = |members: Vec<CompositeEntryHandle>| {
            members
                .iter()
                .map(|value| names[&value.as_target().node_id()])
                .collect::<Vec<_>>()
        };
        let snapshot = |family: &MobjectFamily| {
            let children = (0..family.member_count().unwrap())
                .map(|i| names[&family.member_at(i).unwrap().unwrap().as_target().node_id()])
                .collect::<Vec<_>>();
            json!({"points":family.get_num_points().unwrap(),"length":family.split_count().unwrap(),"children":children})
        };
        let captured = root.member_view().unwrap();
        let before = snapshot(&root);
        let plan = root.slice_plan().unwrap();
        let mut events = Vec::new();
        for (phase, index) in [("step", 2), ("start", 0), ("stop", 1)] {
            if spec["slice"][index].is_null() {
                continue;
            }
            let mut event = snapshot(&root);
            event["phase"] = json!(phase);
            events.push(event);
            if spec["phase"] != phase {
                continue;
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
                "set_points" => root.set_points_f64(&points).unwrap(),
                "clear_points" => root.set_points_f64(&[]).unwrap(),
                other => panic!("unknown fixture action {other}"),
            }
        }
        let node_count = scene.integration_store().borrow().len();
        let revision = scene.revision();
        let selected = plan
            .select_slice(
                spec["slice"][0].as_i64(),
                spec["slice"][1].as_i64(),
                spec["slice"][2].as_i64().unwrap_or(1),
            )
            .unwrap();
        assert_eq!(scene.integration_store().borrow().len(), node_count);
        assert_eq!(scene.revision(), revision);
        let result = match selected {
            None => json!({"exception":"IndexError","message":"list index out of range"}),
            Some(members) => {
                let expected = member_names(members.clone());
                // The normal native constructor accepts the selected existing
                // semantic identities, including self plus direct children.
                let targets = members
                    .iter()
                    .map(CompositeEntryHandle::as_target)
                    .collect::<Vec<_>>();
                let sliced = scene.family(&targets).unwrap();
                assert_eq!(sliced.member_count().unwrap(), expected.len());
                json!({"type":spec["class"],"members":expected})
            }
        };
        for (key, observed) in [
            ("before", before),
            ("events", json!(events)),
            ("result", result),
            ("after", snapshot(&root)),
        ] {
            assert_eq!(observed, case[key], "{spec} {key}");
        }
        let old = (0..captured.len().unwrap())
            .map(|i| {
                names[&captured
                    .member_at(i)
                    .unwrap()
                    .unwrap()
                    .as_target()
                    .node_id()]
            })
            .collect::<Vec<_>>();
        assert_eq!(
            json!(old),
            case["capturedListAfter"],
            "{spec} native retained list"
        );
        // Public Python raw-list object identity is a separate qualification.
    }
}

#[test]
fn native_slice_signed_extremes_and_zero_step_are_checked() {
    let mut scene = Scene::new();
    let a = scene.circle(0.2).unwrap();
    let b = scene.square(0.3).unwrap();
    let root = scene.family(&[(&a).into(), (&b).into()]).unwrap();
    let plan = root.slice_plan().unwrap();
    assert!(matches!(
        plan.select_slice(None, None, 0),
        Err(AuthoringError::ZeroSliceStep)
    ));
    let forward = plan
        .select_slice(Some(i64::MIN), Some(i64::MAX), i64::MAX)
        .unwrap()
        .unwrap();
    assert_eq!(forward.len(), 1);
    assert_eq!(forward[0].as_target().node_id(), a.node_id());
    let reverse = plan.select_slice(None, None, i64::MIN).unwrap().unwrap();
    assert_eq!(reverse.len(), 1);
    assert_eq!(reverse[0].as_target().node_id(), b.node_id());
}
