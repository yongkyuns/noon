from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    file = Path(path)
    text = file.read_text()
    count = text.count(old)
    if count != 1:
        raise SystemExit(
            f"expected exactly one match in {path}, found {count}: {old[:80]!r}"
        )
    file.write_text(text.replace(old, new, 1))


rust = "crates/noon-web/src/canonical_authoring_scene.rs"
replace_once(
    rust,
    """enum SceneMembershipBatchKind {\n    Add,\n    Remove,\n    Clear,\n    Replace,\n}\n""",
    """enum SceneMembershipBatchKind {\n    Add,\n    Remove,\n    Clear,\n    Replace,\n    BringToBack,\n}\n""",
)
replace_once(
    rust,
    """        let request = match batch.kind {\n            SceneMembershipBatchKind::Add => noon::SceneMembershipRequest::Add(&borrowed),\n            SceneMembershipBatchKind::Remove => noon::SceneMembershipRequest::Remove(&borrowed),\n            SceneMembershipBatchKind::Clear => {\n""",
    """        let request = match batch.kind {\n            SceneMembershipBatchKind::Add => noon::SceneMembershipRequest::Add(&borrowed),\n            SceneMembershipBatchKind::Remove => noon::SceneMembershipRequest::Remove(&borrowed),\n            SceneMembershipBatchKind::BringToBack => {\n                noon::SceneMembershipRequest::BringToBack(&borrowed)\n            }\n            SceneMembershipBatchKind::Clear => {\n""",
)
replace_once(
    rust,
    """                \"add\" => SceneMembershipBatchKind::Add,\n                \"remove\" => SceneMembershipBatchKind::Remove,\n                \"clear\" => SceneMembershipBatchKind::Clear,\n                \"replace\" => SceneMembershipBatchKind::Replace,\n                _ => {\n                    return Err(js_error(format!(\n                        \"membership batch kind must be add, remove, clear, or replace; got {kind:?}\"\n                    )))\n                }\n""",
    """                \"add\" => SceneMembershipBatchKind::Add,\n                \"remove\" => SceneMembershipBatchKind::Remove,\n                \"clear\" => SceneMembershipBatchKind::Clear,\n                \"replace\" => SceneMembershipBatchKind::Replace,\n                \"bring_to_back\" => SceneMembershipBatchKind::BringToBack,\n                _ => {\n                    return Err(js_error(format!(\n                        \"membership batch kind must be add, remove, clear, replace, or bring_to_back; got {kind:?}\"\n                    )))\n                }\n""",
)

test = r'''
    #[test]
    fn canonical_membership_bring_to_back_routes_through_shared_authority() {
        let mut context = CanonicalAuthoringScene::default();
        let first = context.scene.circle(0.4).unwrap();
        let second = context.scene.square(0.8).unwrap();
        let third = context.scene.rectangle(0.6, 1.0).unwrap();
        context.bind_mobject(ObjectId::new(0), &first).unwrap();
        context.bind_mobject(ObjectId::new(1), &second).unwrap();
        context.bind_mobject(ObjectId::new(2), &third).unwrap();
        let key = |handle: &noon::Mobject| {
            format!(
                "{}:{}",
                handle.node_id().slot(),
                handle.node_id().generation()
            )
        };

        let revision = context.scene.integration_store().borrow().scene_revision();
        context
            .edit_membership(SceneMembershipBatch {
                kind: SceneMembershipBatchKind::BringToBack,
                members: vec![membership_mobject(2, &third), membership_mobject(0, &first)],
                bindings: Vec::new(),
            })
            .unwrap();
        assert_eq!(
            context.root_membership_keys().unwrap(),
            vec![key(&third), key(&first), key(&second)]
        );
        assert_eq!(
            context.scene.integration_store().borrow().scene_revision(),
            revision.checked_next().unwrap()
        );

        let foreign = CanonicalAuthoringScene::default();
        let foreign_object = foreign.scene.circle(0.2).unwrap();
        let before = context.root_membership_keys().unwrap();
        let revision = context.scene.integration_store().borrow().scene_revision();
        assert!(context
            .edit_membership(SceneMembershipBatch {
                kind: SceneMembershipBatchKind::BringToBack,
                members: vec![
                    membership_mobject(0, &first),
                    membership_mobject(9, &foreign_object),
                ],
                bindings: Vec::new(),
            })
            .is_err());
        assert_eq!(context.root_membership_keys().unwrap(), before);
        assert_eq!(
            context.scene.integration_store().borrow().scene_revision(),
            revision
        );
    }

'''
replace_once(
    rust,
    """    #[test]\n    fn typed_binding_shares_state_and_root_without_snapshot_synchronization() {\n""",
    test
    + """    #[test]\n    fn typed_binding_shares_state_and_root_without_snapshot_synchronization() {\n""",
)

replace_once(
    "web/python/noon.py",
    """        return leaves[0] if len(leaves) == 1 else self\n\n    def remove(self, *mobjects: object) -> Scene:\n""",
    """        return leaves[0] if len(leaves) == 1 else self\n\n    def bring_to_front(self, *mobjects: object) -> Scene:\n        self.add(*mobjects)\n        return self\n\n    def bring_to_back(self, *mobjects: object) -> Scene:\n        self._edit_membership(\"bring_to_back\", mobjects)\n        return self\n\n    def remove(self, *mobjects: object) -> Scene:\n""",
)
