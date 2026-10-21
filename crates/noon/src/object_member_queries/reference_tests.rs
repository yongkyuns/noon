//! Ordinary object queries run through the same native engine as family queries.
use crate::{AuthoringError, CompositeEntryHandle, Mobject, Scene};
use serde_json::{json, Value};

fn snapshot(root: &Mobject) -> Value {
    let names = |members: Vec<CompositeEntryHandle>| {
        members
            .iter()
            .map(|entry| {
                assert_eq!(entry.as_target().node_id(), root.node_id());
                "self"
            })
            .collect::<Vec<_>>()
    };
    json!({"points":root.get_num_points().unwrap(),"length":root.split_count().unwrap(),
        "iteration":names(root.split_iter().unwrap().collect::<Result<Vec<_>,AuthoringError>>().unwrap()),
        "split":names(root.split_members().unwrap())})
}

#[test]
fn ordinary_object_queries_and_point_callbacks_match_pinned_ce() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../fixtures/manim-v0.21/object-queries.json"
    ))
    .unwrap();
    assert_eq!(fixture["manimVersion"], "0.21.0");
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 1020);
    let points = [(0., 0.), (0.1, 0.), (0.2, 0.3), (0.3, 0.4), (0.5, 0.6)];
    for case in cases {
        let spec = &case["spec"];
        let mut scene = Scene::new();
        let mut root = scene.square(0.3).unwrap();
        root.set_points_f64(&points[..spec["ownPoints"].as_u64().unwrap() as usize])
            .unwrap();
        let before = snapshot(&root);
        let mut events = Vec::new();
        let mut callback = |root: &mut Mobject, phase: &str| {
            let mut event = snapshot(root);
            event["phase"] = json!(phase);
            events.push(event);
            if spec["phase"] == phase {
                match spec["action"].as_str().unwrap() {
                    "none" => {}
                    "set_points" => root.set_points_f64(&points[..4]).unwrap(),
                    "clear_points" => root.set_points_f64(&[]).unwrap(),
                    other => panic!("unknown action {other}"),
                }
            }
        };
        let result = if spec["query"] == "scalar" {
            let count = root.split_count().unwrap() as i64;
            callback(&mut root, "index");
            let index = spec["index"].as_i64().unwrap();
            let index = if index < 0 { count + index } else { index };
            if index < 0 || index >= count {
                json!({"exception":"IndexError","message":"range object index out of range"})
            } else {
                match root.split_at(index as usize).unwrap() {
                    Some(entry) => {
                        assert_eq!(entry.as_target().node_id(), root.node_id());
                        assert!(matches!(entry, CompositeEntryHandle::Mobject(_)));
                        json!("self")
                    }
                    None => json!({"exception":"IndexError","message":"list index out of range"}),
                }
            }
        } else {
            let plan = root.slice_plan().unwrap();
            for (phase, index) in [("step", 2), ("start", 0), ("stop", 1)] {
                if !spec["slice"][index].is_null() {
                    callback(&mut root, phase);
                }
            }
            let nodes = scene.integration_store().borrow().len();
            let revision = scene.revision();
            let selected = plan
                .select_slice(
                    spec["slice"][0].as_i64(),
                    spec["slice"][1].as_i64(),
                    spec["slice"][2].as_i64().unwrap_or(1),
                )
                .unwrap();
            assert_eq!(scene.revision(), revision);
            assert_eq!(scene.integration_store().borrow().len(), nodes);
            match selected {
                None => json!({"exception":"IndexError","message":"list index out of range"}),
                Some(entries) => {
                    let names = entries
                        .iter()
                        .map(|entry| {
                            assert_eq!(entry.as_target().node_id(), root.node_id());
                            "self"
                        })
                        .collect::<Vec<_>>();
                    let targets = entries
                        .iter()
                        .map(CompositeEntryHandle::as_target)
                        .collect::<Vec<_>>();
                    scene.family(&targets).unwrap();
                    json!({"type":if spec["class"]=="Mobject"{"Group"}else{"VGroup"},"members":names})
                }
            }
        };
        let observed = json!({"spec":spec,"before":before,"events":events,"result":result,"after":snapshot(&root)});
        assert_eq!(observed, *case, "{spec}");
    }
}

#[test]
fn object_owned_members_use_existing_identity_and_native_member_storage() {
    use noon_core::SemanticMutationTransaction;
    let mut scene = Scene::new();
    let mut root = scene.square(0.3).unwrap();
    let a = scene.circle(0.2).unwrap();
    let b = scene.circle(0.4).unwrap();
    let id = root.node_id();
    let mut transaction = SemanticMutationTransaction::new();
    transaction.add_member(id, a.node_id());
    transaction
        .apply(&mut scene.integration_store().borrow_mut())
        .unwrap();
    let old = root.member_view().unwrap();
    let iterator = root.split_iter().unwrap();
    root.set_points_f64(&[]).unwrap();
    assert_eq!(
        iterator
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
            .iter()
            .map(|entry| entry.as_target().node_id())
            .collect::<Vec<_>>(),
        vec![id, a.node_id()]
    );
    let mut transaction = SemanticMutationTransaction::new();
    transaction.replace_member_container(id);
    transaction.add_member(id, b.node_id());
    transaction
        .apply(&mut scene.integration_store().borrow_mut())
        .unwrap();
    let mut transaction = SemanticMutationTransaction::new();
    transaction.remove_member(id, a.node_id());
    transaction
        .apply(&mut scene.integration_store().borrow_mut())
        .unwrap();
    assert_eq!(old.len().unwrap(), 1);
    assert_eq!(
        old.member_at(0).unwrap().unwrap().as_target().node_id(),
        a.node_id()
    );
    assert_eq!(root.member_count().unwrap(), 1);
    assert_eq!(
        root.member_at(0).unwrap().unwrap().as_target().node_id(),
        b.node_id()
    );
    assert_eq!(root.node_id(), id);
    scene.add_many(&[(&root).into()]).unwrap();
    scene.execution_session().unwrap();
}
