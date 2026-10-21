use crate::{AuthoringError, Mobject, Scene};

fn ids(root: &Mobject) -> Vec<noon_core::SemanticNodeId> {
    root.split_members_with_self(false)
        .unwrap()
        .iter()
        .map(|entry| entry.as_target().node_id())
        .collect()
}

#[test]
fn ordinary_members_edit_atomically_and_retain_old_containers() {
    let mut scene = Scene::new();
    let root = scene.square(0.3).unwrap();
    let a = scene.circle(0.1).unwrap();
    let b = scene.circle(0.2).unwrap();
    let c = scene.circle(0.4).unwrap();
    let root_id = root.node_id();
    root.add_many(&[(&a).into(), (&b).into(), (&a).into()])
        .unwrap();
    assert_eq!(ids(&root), vec![b.node_id(), a.node_id()]);
    let old = root.member_view().unwrap();
    root.add((&c).into()).unwrap();
    assert_eq!(old.len().unwrap(), 2);
    root.remove((&a).into()).unwrap();
    root.prepend_many(&[(&c).into(), (&b).into(), (&c).into()])
        .unwrap();
    assert_eq!(ids(&root), vec![c.node_id(), b.node_id()]);
    root.insert_member(1, (&a).into()).unwrap();
    assert_eq!(ids(&root), vec![c.node_id(), a.node_id(), b.node_id()]);
    let previous = root.member_view().unwrap();
    root.replace_members(&[(&b).into(), (&a).into()]).unwrap();
    assert_eq!(previous.len().unwrap(), 3);
    assert_eq!(ids(&root), vec![b.node_id(), a.node_id()]);
    let revision = scene.revision();
    let before = ids(&root);
    assert!(root.add_many(&[(&c).into(), (&root).into()]).is_err());
    assert_eq!(ids(&root), before);
    assert_eq!(scene.revision(), revision);
    let foreign = Scene::new().circle(0.2).unwrap();
    assert!(matches!(
        root.replace_members(&[(&c).into(), (&foreign).into()]),
        Err(AuthoringError::ForeignStore)
    ));
    assert_eq!(ids(&root), before);
    assert_eq!(scene.revision(), revision);
    assert_eq!(root.node_id(), root_id);
    let copy = root.copy_with_references(&[(&a).into()]).unwrap();
    let copied_a = copy.mobject(&a).unwrap();
    assert_eq!(
        ids(copy.root()),
        vec![copy.mobject(&b).unwrap().node_id(), copied_a.node_id()]
    );
    assert_ne!(copied_a.node_id(), a.node_id());
    scene.add_many(&[(&root).into()]).unwrap();
    let mut session = scene.execution_session().unwrap();
    let revision = scene.revision();
    scene
        .live(&mut session)
        .add_family_members(&root, &[(&c).into()])
        .unwrap();
    assert_eq!(scene.revision(), revision.checked_next().unwrap());
    scene
        .live(&mut session)
        .replace_members((&root).into(), &[(&a).into()])
        .unwrap();
    assert_eq!(ids(&root), vec![a.node_id()]);
    assert_eq!(root.node_id(), root_id);
}

#[cfg(feature = "diagnostics")]
#[test]
fn membership_sequences_match_pinned_ce() {
    use serde_json::{json, Value};
    use std::collections::BTreeMap;
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../fixtures/manim-v0.21/object-members.json"
    ))
    .unwrap();
    assert_eq!(fixture["manimVersion"], "0.21.0");
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 50);
    for case in cases {
        let mut scene = Scene::new();
        let mut root = scene.square(0.3).unwrap();
        let points = [(0., 0.), (0.1, 0.), (0.2, 0.3), (0.3, 0.4)];
        root.set_points_f64(&points[..case["spec"]["ownPoints"].as_u64().unwrap() as usize])
            .unwrap();
        let a = scene.circle(0.1).unwrap();
        let b = scene.square(0.2).unwrap();
        let c = scene.circle(0.3).unwrap();
        let d = scene.square(0.4).unwrap();
        let names = BTreeMap::from([
            (root.node_id(), "self"),
            (a.node_id(), "a"),
            (b.node_id(), "b"),
            (c.node_id(), "c"),
            (d.node_id(), "d"),
        ]);
        let labels = |entries: Vec<crate::CompositeEntryHandle>| {
            entries
                .iter()
                .map(|entry| names[&entry.as_target().node_id()])
                .collect::<Vec<_>>()
        };
        let snapshot = |operation: &str, captured: Option<crate::FamilyIterator>| {
            let mut row = json!({"operation":operation,"returnsSelf":true,"points":root.get_num_points().unwrap(),"length":root.split_count().unwrap(),"members":labels(root.split_members_with_self(false).unwrap()),"split":labels(root.split_members().unwrap()),"iteration":labels(root.split_iter().unwrap().collect::<Result<Vec<_>,_>>().unwrap())});
            if let Some(iterator) = captured {
                row["capturedIterator"] =
                    json!(labels(iterator.collect::<Result<Vec<_>, _>>().unwrap()));
            }
            row
        };
        let mut steps = Vec::new();
        root.add_many(&[(&a).into(), (&b).into(), (&a).into()])
            .unwrap();
        steps.push(snapshot("add_repeated", None));
        let captured = root.split_iter().unwrap();
        root.add((&c).into()).unwrap();
        steps.push(snapshot("add_replaces_list", Some(captured)));
        let captured = root.split_iter().unwrap();
        root.remove_many(&[(&a).into(), (&d).into()]).unwrap();
        steps.push(snapshot("remove_edits_list", Some(captured)));
        let captured = root.split_iter().unwrap();
        root.prepend_many(&[(&c).into(), (&b).into(), (&c).into()])
            .unwrap();
        steps.push(snapshot("prepend_repeated", Some(captured)));
        let captured = root.split_iter().unwrap();
        root.prepend_many(&[(&c).into(), (&b).into()]).unwrap();
        steps.push(snapshot("prepend_same_order", Some(captured)));
        let count = root.member_count().unwrap() as i64;
        let index = case["spec"]["index"].as_i64().unwrap();
        let position = if index < 0 {
            (count + index).max(0)
        } else {
            index.min(count)
        };
        root.insert_member(position as usize, (&d).into()).unwrap();
        steps.push(snapshot("insert", None));
        let captured = root.split_iter().unwrap();
        root.replace_members(&[(&a).into(), (&c).into()]).unwrap();
        steps.push(snapshot("replace", Some(captured)));
        assert_eq!(json!(steps), case["steps"], "{}", case["spec"]);
        let copy = root
            .copy_with_references(&[(&a).into(), (&c).into(), (&a).into()])
            .unwrap();
        let copied_a = copy.mobject(&a).unwrap();
        assert_eq!(
            copy.root()
                .member_at(0)
                .unwrap()
                .unwrap()
                .as_target()
                .node_id(),
            copied_a.node_id()
        );
        assert_eq!(
            copy.root()
                .member_at(1)
                .unwrap()
                .unwrap()
                .as_target()
                .node_id(),
            copy.mobject(&c).unwrap().node_id()
        );
        assert_ne!(copied_a.node_id(), a.node_id());
    }
}
