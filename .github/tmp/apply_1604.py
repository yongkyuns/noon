from pathlib import Path
import re


def replace_once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    assert count == 1, f"{label}: expected one match, got {count}"
    return text.replace(old, new, 1)


def regex_once(text: str, pattern: str, replacement: str, label: str) -> str:
    updated, count = re.subn(pattern, replacement, text, count=1, flags=re.DOTALL)
    assert count == 1, f"{label}: expected one match, got {count}"
    return updated


# Make visual-vs-receiver-owned state an explicit core contract. The exhaustive
# struct literal intentionally makes future SemanticObjectState fields require a
# deliberate classification instead of silently joining either copy domain.
core = Path("crates/noon-core/src/semantic_store/object_content.rs")
text = core.read_text()
if "with_visual_state_from" not in text:
    marker = """    pub const fn presentation(&self) -> SemanticPresentation {\n        self.presentation\n    }\n"""
    method = """    /// Return receiver-owned state with target visual state.\n    ///\n    /// Persistent `become` keeps painter provenance, role, bindings and identity\n    /// on the receiver while copying the target's content, transform and style.\n    /// Keep this as an exhaustive struct literal: adding a new authored-state field\n    /// must fail to compile until its ownership is explicitly classified here.\n    pub fn with_visual_state_from(&self, target: &Self) -> Self {\n        Self {\n            content: target.content,\n            transform: target.transform,\n            style: target.style.clone(),\n            presentation: self.presentation,\n            role: self.role,\n            signal_bindings: self.signal_bindings.clone(),\n        }\n    }\n\n""" + marker
    text = replace_once(text, marker, method, "core visual-state method")

    test_marker = """    #[test]\n    fn semantic_object_role_is_explicit_and_defaults_to_ordinary() {\n"""
    test = """    #[test]\n    fn visual_state_copy_preserves_receiver_owned_metadata() {\n        let mut receiver = SemanticObjectState::new(StoredGeometry::Square { side: 1.0 });\n        receiver.set_z_index(7.0);\n        receiver.assign_insertion_order(11);\n        receiver.set_role(SemanticObjectRole::Camera2D);\n        let mut target = SemanticObjectState::new(StoredGeometry::Circle { radius: 2.0 });\n        target.transform.translation = SemanticVec3::new(3.0, -2.0, 0.0);\n        target.style.object_opacity = 0.25;\n        target.set_z_index(-4.0);\n        target.assign_insertion_order(99);\n        target.set_role(SemanticObjectRole::ArrowEndTip);\n\n        let copied = receiver.with_visual_state_from(&target);\n\n        assert_eq!(copied.content, target.content);\n        assert_eq!(copied.transform, target.transform);\n        assert_eq!(copied.style, target.style);\n        assert_eq!(copied.presentation(), receiver.presentation());\n        assert_eq!(copied.role(), receiver.role());\n        assert_eq!(copied.signal_bindings(), receiver.signal_bindings());\n    }\n\n""" + test_marker
    text = replace_once(text, test_marker, test, "core ownership test")
    core.write_text(text)


state = Path("crates/noon/src/state_replacement.rs")
text = state.read_text()
if "aligned_source_prototype" not in text:
    text = replace_once(
        text,
        """#[derive(Clone)]\nenum PersistentTargetNode {\n    Object,\n    Family {\n        members: Vec<SemanticNodeId>,\n        z_index: f64,\n    },\n}\n""",
        """#[derive(Clone)]\nenum PersistentTargetNode {\n    Object,\n    Family { members: Vec<SemanticNodeId> },\n}\n\n/// Select receiver metadata for synthesized expansion nodes using the same repeat\n/// index rule as Manim's `add_n_more_submobjects`. This does not transfer target\n/// identity or painter metadata; it only chooses the receiver-side prototype.\nfn aligned_source_prototype(\n    source_members: &[SemanticNodeId],\n    target_len: usize,\n    target_index: usize,\n) -> Option<SemanticNodeId> {\n    if source_members.is_empty() {\n        return None;\n    }\n    if source_members.len() >= target_len {\n        return source_members.get(target_index).copied();\n    }\n    let source_index = target_index * source_members.len() / target_len;\n    source_members.get(source_index).copied()\n}\n""",
        "persistent target enum",
    )

    text = regex_once(
        text,
        r"SemanticNodeKind::Family\(presentation\) => Ok\(PersistentTargetNode::Family \{\s*members: node\.members_iter\(\)\.collect\(\),\s*z_index: presentation\.z_index,\s*\}\),",
        """SemanticNodeKind::Family(_) => Ok(PersistentTargetNode::Family {\n                members: node.members_iter().collect(),\n            }),""",
        "target family template",
    )

    text = replace_once(
        text,
        """    fn reconcile_node(\n        &mut self,\n        candidate: Option<SemanticNodeId>,\n        target: SemanticNodeId,\n    ) -> Result<SemanticTransactionNodeRef, AuthoringError> {\n""",
        """    fn reconcile_node(\n        &mut self,\n        candidate: Option<SemanticNodeId>,\n        prototype: Option<SemanticNodeId>,\n        target: SemanticNodeId,\n    ) -> Result<SemanticTransactionNodeRef, AuthoringError> {\n""",
        "reconcile signature",
    )

    text = regex_once(
        text,
        r"SemanticTransactionNodeRef::Pending\(\s*self\.transaction\s*\.create_node\(SemanticNodeCreation::object\(state\.clone\(\)\)\),\s*\)",
        """{\n                        let receiver_state = self\n                            .store\n                            .semantic_object_state_checked(prototype.unwrap_or(target))\n                            .ok()\n                            .cloned()\n                            .unwrap_or_else(|| SemanticObjectState::new(state.content))\n                            .with_visual_state_from(state);\n                        SemanticTransactionNodeRef::Pending(\n                            self.transaction\n                                .create_node(SemanticNodeCreation::object(receiver_state)),\n                        )\n                    }""",
        "pending object state",
    )

    text = replace_once(
        text,
        "            PersistentTargetNode::Family { members, z_index } => {\n",
        "            PersistentTargetNode::Family { members } => {\n",
        "family arm header",
    )

    text = regex_once(
        text,
        r"let \(receiver, reused\) = if let Some\(source\) = reusable \{\s*self\.source_to_target\.insert\(source, target\);\s*\(SemanticTransactionNodeRef::Existing\(source\), Some\(source\)\)\s*\} else \{\s*let pending = self\.transaction\.create_node\(SemanticNodeCreation::family\(\)\);\s*if z_index != 0\.0 \{\s*self\.transaction\.set_z_index\(pending, z_index\);\s*\}\s*\(SemanticTransactionNodeRef::Pending\(pending\), None\)\s*\};",
        """let (receiver, reused, prototype_members) = if let Some(source) = reusable {\n                    self.source_to_target.insert(source, target);\n                    (\n                        SemanticTransactionNodeRef::Existing(source),\n                        Some(source),\n                        Vec::new(),\n                    )\n                } else {\n                    let pending = self.transaction.create_node(SemanticNodeCreation::family());\n                    let (prototype_members, prototype_z_index) = prototype\n                        .and_then(|prototype| self.store.node(prototype))\n                        .and_then(|node| match node.kind() {\n                            SemanticNodeKind::Family(presentation) => Some((\n                                node.members_iter().collect::<Vec<_>>(),\n                                presentation.z_index,\n                            )),\n                            _ => None,\n                        })\n                        .unwrap_or_else(|| (Vec::new(), 0.0));\n                    if prototype_z_index != 0.0 {\n                        self.transaction.set_z_index(pending, prototype_z_index);\n                    }\n                    (\n                        SemanticTransactionNodeRef::Pending(pending),\n                        None,\n                        prototype_members,\n                    )\n                };""",
        "family receiver ownership",
    )

    text = regex_once(
        text,
        r"for target_member in members \{\s*let member = self\.reconcile_node\(None, target_member\)\?;\s*self\.transaction\.add_member\(receiver, member\);\s*\}",
        """for (index, target_member) in members.iter().copied().enumerate() {\n                        let member = self.reconcile_node(\n                            None,\n                            aligned_source_prototype(&prototype_members, members.len(), index),\n                            target_member,\n                        )?;\n                        self.transaction.add_member(receiver, member);\n                    }""",
        "pending family descendants",
    )

    text = regex_once(
        text,
        r"for \(index, &target_member\) in target_members\.iter\(\)\.enumerate\(\) \{\s*desired\.push\(self\.reconcile_node\(current\.get\(index\)\.copied\(\), target_member\)\?\);\s*\}",
        """for (index, &target_member) in target_members.iter().enumerate() {\n            desired.push(self.reconcile_node(\n                current.get(index).copied(),\n                aligned_source_prototype(current, target_members.len(), index),\n                target_member,\n            )?);\n        }""",
        "existing family descendants",
    )

    text = replace_once(
        text,
        "    let root = reconcile.reconcile_node(Some(source.node_id()), target.node_id())?;\n",
        """    let root = reconcile.reconcile_node(\n        Some(source.node_id()),\n        Some(source.node_id()),\n        target.node_id(),\n    )?;\n""",
        "root reconcile",
    )
    state.write_text(text)


tests = Path("crates/noon/tests/family_state.rs")
text = tests.read_text()
if "assert_become_visual_state" not in text:
    family_marker = """fn family(scene: &Scene) -> MobjectFamily {\n    let a = scene.square(1.0).unwrap();\n    let mut b = scene.rectangle(1.0, 2.0).unwrap();\n    b.shift(2.0, 0.0).unwrap();\n    let nested = scene.family(&[(&a).into(), (&b).into()]).unwrap();\n    scene.family(&[(&a).into(), (&nested).into()]).unwrap()\n}\n"""
    helpers = family_marker + """\nfn object_state(scene: &Scene, id: noon::SemanticNodeId) -> noon::SemanticObjectState {\n    scene\n        .integration_store()\n        .borrow()\n        .semantic_object_state_checked(id)\n        .unwrap()\n        .clone()\n}\n\nfn assert_become_visual_state(\n    receiver: &noon::SemanticObjectState,\n    target: &noon::SemanticObjectState,\n) {\n    assert_eq!(receiver.content, target.content);\n    assert_eq!(receiver.transform, target.transform);\n    assert_eq!(receiver.style, target.style);\n}\n"""
    text = replace_once(text, family_marker, helpers, "family-state helpers")

    start = text.index("#[test]\nfn unequal_family_become_reuses_receiver_identity_and_never_imports_target_ids()")
    end = text.index("\n#[test]\nfn dimension_matching_uses_aggregate_bounds", start)
    replacement = r'''#[test]
fn unequal_family_become_reuses_receiver_identity_and_never_imports_target_ids() {
    let scene = Scene::new();
    let source_leaf = scene.square(1.0).unwrap();
    source_leaf.set_z_index(7.0).unwrap();
    let source_state = object_state(&scene, source_leaf.node_id());
    let source = scene.family(&[(&source_leaf).into()]).unwrap();
    let source_root = source.node_id();
    let first_target = scene.circle(2.0).unwrap();
    first_target.set_z_index(-4.0).unwrap();
    let mut second_target = scene.rectangle(3.0, 1.0).unwrap();
    second_target.shift(4.0, 1.0).unwrap();
    second_target.set_z_index(11.0).unwrap();
    let target = scene
        .family(&[(&first_target).into(), (&second_target).into()])
        .unwrap();
    let target_ids = leaves(&target);
    let target_states = target_ids
        .iter()
        .map(|&id| object_state(&scene, id))
        .collect::<Vec<_>>();
    let revision = scene.revision();

    source.become_family(&target, Default::default()).unwrap();

    assert_eq!(source.node_id(), source_root);
    assert_eq!(scene.revision(), revision.checked_next().unwrap());
    let receiver_ids = leaves(&source);
    assert_eq!(receiver_ids.len(), 2);
    assert_eq!(receiver_ids[0], source_leaf.node_id());
    assert!(!target_ids.contains(&receiver_ids[0]));
    assert!(!target_ids.contains(&receiver_ids[1]));
    assert_ne!(receiver_ids[1], source_leaf.node_id());
    let receiver_states = receiver_ids
        .iter()
        .map(|&id| object_state(&scene, id))
        .collect::<Vec<_>>();
    for (receiver, target) in receiver_states.iter().zip(&target_states) {
        assert_become_visual_state(receiver, target);
        assert_eq!(receiver.z_index(), source_state.z_index());
        assert_eq!(receiver.role(), source_state.role());
        assert_eq!(receiver.signal_bindings(), source_state.signal_bindings());
    }
    assert_eq!(
        receiver_states[0].insertion_order(),
        source_state.insertion_order()
    );
    assert_ne!(
        receiver_states[1].insertion_order(),
        target_states[1].insertion_order()
    );
    assert!(receiver_states[1].insertion_order() > receiver_states[0].insertion_order());
    assert_eq!(
        target_ids
            .iter()
            .map(|&id| object_state(&scene, id))
            .collect::<Vec<_>>(),
        target_states
    );

    let empty = scene.family(&[]).unwrap();
    let expanded_ids = receiver_ids.clone();
    let revision = scene.revision();
    source.become_family(&empty, Default::default()).unwrap();
    assert_eq!(source.node_id(), source_root);
    assert!(leaves(&source).is_empty());
    assert_eq!(scene.revision(), revision.checked_next().unwrap());
    {
        let store = scene.integration_store().borrow();
        for &id in &expanded_ids {
            assert!(store.node(id).is_some());
        }
    }

    source.become_family(&target, Default::default()).unwrap();
    let reexpanded_ids = leaves(&source);
    assert_eq!(reexpanded_ids.len(), target_ids.len());
    assert!(reexpanded_ids.iter().all(|id| !target_ids.contains(id)));
    let store = scene.integration_store().borrow();
    for id in expanded_ids {
        assert!(store.node(id).is_some());
    }
}

#[test]
fn live_unequal_family_become_preserves_same_z_receiver_painter_order() {
    use noon_compile::semantic_execution_object_id;

    let mut scene = Scene::new();
    let source_leaf = scene.square(2.0).unwrap();
    source_leaf.set_z_index(5.0).unwrap();
    let source = scene.family(&[(&source_leaf).into()]).unwrap();
    let peer = scene.square(2.0).unwrap();
    peer.set_z_index(5.0).unwrap();

    let first_target = scene.circle(1.0).unwrap();
    first_target.set_z_index(-10.0).unwrap();
    let second_target = scene.circle(1.0).unwrap();
    second_target.set_z_index(20.0).unwrap();
    let target = scene
        .family(&[(&first_target).into(), (&second_target).into()])
        .unwrap();
    let target_ids = leaves(&target);

    scene.add_many(&[(&source).into(), (&peer).into()]).unwrap();
    let mut session = scene.execution_session().unwrap();
    scene
        .live(&mut session)
        .become_family(&source, &target, Default::default())
        .unwrap();

    let receiver_ids = leaves(&source);
    assert_eq!(receiver_ids.len(), 2);
    for &receiver in &receiver_ids {
        assert_eq!(object_state(&scene, receiver).z_index(), 5.0);
    }
    let actual = session
        .painter_order()
        .iter()
        .map(|&row| session.frame().objects[row as usize].id)
        .collect::<Vec<_>>();
    assert_eq!(
        actual,
        [
            semantic_execution_object_id(receiver_ids[0]),
            semantic_execution_object_id(receiver_ids[1]),
            semantic_execution_object_id(peer.node_id()),
        ]
    );
    assert!(target_ids
        .iter()
        .map(|&id| semantic_execution_object_id(id))
        .all(|id| !actual.contains(&id)));
}
'''
    text = text[:start] + replacement + text[end:]
    tests.write_text(text)
