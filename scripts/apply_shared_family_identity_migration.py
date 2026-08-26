from pathlib import Path


def replace_once(path: str, old: str, new: str, label: str) -> None:
    p = Path(path)
    text = p.read_text()
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected one match in {path}, found {count}")
    p.write_text(text.replace(old, new, 1))


# Shared semantic identities for detached authoring objects and families.
replace_once(
    "crates/noon-core/src/semantic_store.rs",
    """pub enum SemanticNodeKind {\n    /// Compatibility payload while `SceneDefinition` consumers migrate.\n    Object(ObjectDefinition),\n    /// A semantic family/collection with no implied transform ownership.\n    Family,\n}\n""",
    """pub enum SemanticNodeKind {\n    /// Compatibility payload while `SceneDefinition` consumers migrate.\n    Object(ObjectDefinition),\n    /// Stable frontend object identity whose mutable authoring payload lives in a\n    /// shared frontend handle rather than the legacy SceneDefinition object shape.\n    AuthoringObject,\n    /// A semantic family/collection with no implied transform ownership.\n    Family,\n}\n""",
    "semantic authoring object kind",
)
replace_once(
    "crates/noon-core/src/semantic_store.rs",
    """        match &self.kind {\n            SemanticNodeKind::Object(object) => Some(object),\n            SemanticNodeKind::Family => None,\n        }\n""",
    """        match &self.kind {\n            SemanticNodeKind::Object(object) => Some(object),\n            SemanticNodeKind::AuthoringObject | SemanticNodeKind::Family => None,\n        }\n""",
    "semantic object accessor",
)
replace_once(
    "crates/noon-core/src/semantic_store.rs",
    """        match &mut self.kind {\n            SemanticNodeKind::Object(object) => Some(object),\n            SemanticNodeKind::Family => None,\n        }\n""",
    """        match &mut self.kind {\n            SemanticNodeKind::Object(object) => Some(object),\n            SemanticNodeKind::AuthoringObject | SemanticNodeKind::Family => None,\n        }\n""",
    "semantic object mutable accessor",
)
replace_once(
    "crates/noon-core/src/semantic_store.rs",
    """    pub fn insert_family(&mut self) -> SemanticNodeId {\n        self.insert_kind(SemanticNodeKind::Family)\n    }\n""",
    """    pub fn insert_authoring_object(&mut self) -> SemanticNodeId {\n        self.insert_kind(SemanticNodeKind::AuthoringObject)\n    }\n\n    pub fn insert_family(&mut self) -> SemanticNodeId {\n        self.insert_kind(SemanticNodeKind::Family)\n    }\n""",
    "semantic authoring object insertion",
)
replace_once(
    "crates/noon-core/src/semantic_store.rs",
    """    #[test]\n    fn deletion_does_not_renumber_unrelated_semantic_handles() {\n""",
    """    #[test]\n    fn authoring_objects_have_stable_shared_family_identity() {\n        let mut store = SemanticStore::new();\n        let first = store.insert_authoring_object();\n        let second = store.insert_authoring_object();\n        let family = store.insert_family();\n        let alias = store.insert_family();\n\n        store.add_member(family, first).unwrap();\n        store.add_member(family, second).unwrap();\n        store.add_member(alias, first).unwrap();\n        assert_eq!(store.node(family).unwrap().members(), &[first, second]);\n        assert_eq!(store.node(first).unwrap().parents(), &[family, alias]);\n\n        // SemanticStore owns duplicate suppression and direct-member ordering.\n        store.add_member(family, first).unwrap();\n        assert_eq!(store.node(family).unwrap().members(), &[first, second]);\n        assert_eq!(store.node(first).unwrap().parents(), &[family, alias]);\n\n        assert!(store.remove_member(family, first).unwrap());\n        assert_eq!(store.node(family).unwrap().members(), &[second]);\n        assert_eq!(store.node(first).unwrap().parents(), &[alias]);\n        assert!(!store.remove_member(family, first).unwrap());\n    }\n\n    #[test]\n    fn deletion_does_not_renumber_unrelated_semantic_handles() {\n""",
    "semantic authoring family test",
)

# One Rust/WASM authoring store now owns object and family semantic identity.
replace_once(
    "crates/noon-web/src/authoring_mobject.rs",
    """mod wasm {\n    use wasm_bindgen::prelude::*;\n\n    use super::FrontendMobjectHandle;\n""",
    """mod wasm {\n    use std::{cell::RefCell, rc::Rc};\n\n    use noon_core::{SemanticNodeId, SemanticStore};\n    use wasm_bindgen::prelude::*;\n\n    use super::FrontendMobjectHandle;\n""",
    "wasm shared store imports",
)
replace_once(
    "crates/noon-web/src/authoring_mobject.rs",
    """    #[wasm_bindgen]\n    pub struct WasmAuthoringMobjectHandle(FrontendMobjectHandle);\n""",
    """    type SharedSemanticStore = Rc<RefCell<SemanticStore>>;\n\n    #[wasm_bindgen]\n    pub struct WasmAuthoringStore {\n        semantics: SharedSemanticStore,\n    }\n\n    #[wasm_bindgen]\n    impl WasmAuthoringStore {\n        #[wasm_bindgen(constructor)]\n        pub fn new() -> Self {\n            Self {\n                semantics: Rc::new(RefCell::new(SemanticStore::new())),\n            }\n        }\n\n        #[wasm_bindgen(js_name = createMobject)]\n        pub fn create_mobject(\n            &self,\n            snapshot_json: &str,\n        ) -> Result<WasmAuthoringMobjectHandle, JsValue> {\n            let handle = FrontendMobjectHandle::from_json(snapshot_json).map_err(js_error)?;\n            let id = self.semantics.borrow_mut().insert_authoring_object();\n            Ok(WasmAuthoringMobjectHandle(\n                handle,\n                Some(Rc::clone(&self.semantics)),\n                Some(id),\n            ))\n        }\n\n        #[wasm_bindgen(js_name = createFamily)]\n        pub fn create_family(&self) -> WasmAuthoringFamilyHandle {\n            let id = self.semantics.borrow_mut().insert_family();\n            WasmAuthoringFamilyHandle {\n                semantics: Rc::clone(&self.semantics),\n                id,\n            }\n        }\n    }\n\n    #[wasm_bindgen]\n    pub struct WasmAuthoringFamilyHandle {\n        semantics: SharedSemanticStore,\n        id: SemanticNodeId,\n    }\n\n    impl WasmAuthoringFamilyHandle {\n        fn object_member_id(\n            &self,\n            member: &WasmAuthoringMobjectHandle,\n        ) -> Result<SemanticNodeId, JsValue> {\n            let store = member.1.as_ref().ok_or_else(|| {\n                JsValue::from_str(\"mobject is not attached to a shared authoring store\")\n            })?;\n            if !Rc::ptr_eq(&self.semantics, store) {\n                return Err(JsValue::from_str(\n                    \"family and mobject belong to different authoring stores\",\n                ));\n            }\n            member\n                .2\n                .ok_or_else(|| JsValue::from_str(\"mobject has no semantic identity\"))\n        }\n\n        fn family_member_id(\n            &self,\n            member: &WasmAuthoringFamilyHandle,\n        ) -> Result<SemanticNodeId, JsValue> {\n            if !Rc::ptr_eq(&self.semantics, &member.semantics) {\n                return Err(JsValue::from_str(\n                    \"families belong to different authoring stores\",\n                ));\n            }\n            Ok(member.id)\n        }\n\n        fn add_id(&mut self, member: SemanticNodeId) -> Result<bool, JsValue> {\n            let before = self.member_count();\n            self.semantics\n                .borrow_mut()\n                .add_member(self.id, member)\n                .map_err(|error| js_error(error.to_string()))?;\n            Ok(self.member_count() != before)\n        }\n\n        fn remove_id(&mut self, member: SemanticNodeId) -> Result<bool, JsValue> {\n            self.semantics\n                .borrow_mut()\n                .remove_member(self.id, member)\n                .map_err(|error| js_error(error.to_string()))\n        }\n    }\n\n    #[wasm_bindgen]\n    impl WasmAuthoringFamilyHandle {\n        #[wasm_bindgen(getter, js_name = semanticSlot)]\n        pub fn semantic_slot(&self) -> u32 {\n            self.id.slot()\n        }\n\n        #[wasm_bindgen(getter, js_name = semanticGeneration)]\n        pub fn semantic_generation(&self) -> u32 {\n            self.id.generation()\n        }\n\n        #[wasm_bindgen(getter, js_name = memberCount)]\n        pub fn member_count(&self) -> usize {\n            self.semantics\n                .borrow()\n                .node(self.id)\n                .map_or(0, |node| node.members().len())\n        }\n\n        #[wasm_bindgen(js_name = memberSlot)]\n        pub fn member_slot(&self, index: usize) -> Result<u32, JsValue> {\n            self.semantics\n                .borrow()\n                .node(self.id)\n                .and_then(|node| node.members().get(index).copied())\n                .map(SemanticNodeId::slot)\n                .ok_or_else(|| JsValue::from_str(\"family member index is out of bounds\"))\n        }\n\n        #[wasm_bindgen(js_name = memberGeneration)]\n        pub fn member_generation(&self, index: usize) -> Result<u32, JsValue> {\n            self.semantics\n                .borrow()\n                .node(self.id)\n                .and_then(|node| node.members().get(index).copied())\n                .map(SemanticNodeId::generation)\n                .ok_or_else(|| JsValue::from_str(\"family member index is out of bounds\"))\n        }\n\n        #[wasm_bindgen(js_name = addMobject)]\n        pub fn add_mobject(\n            &mut self,\n            member: &WasmAuthoringMobjectHandle,\n        ) -> Result<bool, JsValue> {\n            let id = self.object_member_id(member)?;\n            self.add_id(id)\n        }\n\n        #[wasm_bindgen(js_name = addFamily)]\n        pub fn add_family(\n            &mut self,\n            member: &WasmAuthoringFamilyHandle,\n        ) -> Result<bool, JsValue> {\n            let id = self.family_member_id(member)?;\n            self.add_id(id)\n        }\n\n        #[wasm_bindgen(js_name = removeMobject)]\n        pub fn remove_mobject(\n            &mut self,\n            member: &WasmAuthoringMobjectHandle,\n        ) -> Result<bool, JsValue> {\n            let id = self.object_member_id(member)?;\n            self.remove_id(id)\n        }\n\n        #[wasm_bindgen(js_name = removeFamily)]\n        pub fn remove_family(\n            &mut self,\n            member: &WasmAuthoringFamilyHandle,\n        ) -> Result<bool, JsValue> {\n            let id = self.family_member_id(member)?;\n            self.remove_id(id)\n        }\n    }\n\n    #[wasm_bindgen]\n    pub struct WasmAuthoringMobjectHandle(\n        FrontendMobjectHandle,\n        Option<SharedSemanticStore>,\n        Option<SemanticNodeId>,\n    );\n""",
    "wasm shared family handles",
)
replace_once(
    "crates/noon-web/src/authoring_mobject.rs",
    """        pub fn new(snapshot_json: &str) -> Result<WasmAuthoringMobjectHandle, JsValue> {\n            FrontendMobjectHandle::from_json(snapshot_json)\n                .map(WasmAuthoringMobjectHandle)\n                .map_err(js_error)\n        }\n\n        #[wasm_bindgen(js_name = cloneHandle)]\n        pub fn clone_handle(&self) -> WasmAuthoringMobjectHandle {\n            WasmAuthoringMobjectHandle(self.0.clone())\n        }\n""",
    """        pub fn new(snapshot_json: &str) -> Result<WasmAuthoringMobjectHandle, JsValue> {\n            FrontendMobjectHandle::from_json(snapshot_json)\n                .map(|handle| WasmAuthoringMobjectHandle(handle, None, None))\n                .map_err(js_error)\n        }\n\n        fn clone_with_handle(&self, handle: FrontendMobjectHandle) -> WasmAuthoringMobjectHandle {\n            if let Some(store) = &self.1 {\n                let id = store.borrow_mut().insert_authoring_object();\n                WasmAuthoringMobjectHandle(handle, Some(Rc::clone(store)), Some(id))\n            } else {\n                WasmAuthoringMobjectHandle(handle, None, None)\n            }\n        }\n\n        #[wasm_bindgen(getter, js_name = semanticSlot)]\n        pub fn semantic_slot(&self) -> Result<u32, JsValue> {\n            self.2\n                .map(SemanticNodeId::slot)\n                .ok_or_else(|| JsValue::from_str(\"mobject has no shared semantic identity\"))\n        }\n\n        #[wasm_bindgen(getter, js_name = semanticGeneration)]\n        pub fn semantic_generation(&self) -> Result<u32, JsValue> {\n            self.2\n                .map(SemanticNodeId::generation)\n                .ok_or_else(|| JsValue::from_str(\"mobject has no shared semantic identity\"))\n        }\n\n        #[wasm_bindgen(js_name = cloneHandle)]\n        pub fn clone_handle(&self) -> WasmAuthoringMobjectHandle {\n            self.clone_with_handle(self.0.clone())\n        }\n""",
    "wasm mobject identity and clone",
)
replace_once(
    "crates/noon-web/src/authoring_mobject.rs",
    """        pub fn target_editor(&self) -> WasmAuthoringMobjectHandle {\n            WasmAuthoringMobjectHandle(self.0.target_editor())\n        }\n""",
    """        pub fn target_editor(&self) -> WasmAuthoringMobjectHandle {\n            self.clone_with_handle(self.0.target_editor())\n        }\n""",
    "wasm target editor semantic identity",
)

# Browser Python bridge allocates every Mobject and Group from the same store.
replace_once(
    "web/python-worker.js",
    """  WasmAuthoringMobjectHandle,\n""",
    """  WasmAuthoringStore,\n""",
    "python worker authoring-store import",
)
replace_once(
    "web/python-worker.js",
    """  await initNoonWeb();\n  self.noonCreateAuthoringMobjectHandle = (snapshotJson) =>\n    new WasmAuthoringMobjectHandle(snapshotJson);\n""",
    """  await initNoonWeb();\n  const authoringStore = new WasmAuthoringStore();\n  self.noonCreateAuthoringMobjectHandle = (snapshotJson) =>\n    authoringStore.createMobject(snapshotJson);\n  self.noonCreateAuthoringFamilyHandle = () => authoringStore.createFamily();\n""",
    "python worker shared authoring store",
)

# Python Group remains a wrapper mirror; Rust decides direct membership transitions.
replace_once(
    "web/python/_manim_semantic_handles.py",
    """try:\n    from js import noonCreateAuthoringMobjectHandle as _create_handle\nexcept ImportError:  # Native CPython tests do not have the browser bridge.\n    _create_handle = None\n""",
    """try:\n    from js import (\n        noonCreateAuthoringFamilyHandle as _create_family_handle,\n        noonCreateAuthoringMobjectHandle as _create_handle,\n    )\nexcept ImportError:  # Native CPython tests do not have the browser bridge.\n    _create_handle = None\n    _create_family_handle = None\n""",
    "python family bridge imports",
)
replace_once(
    "web/python/_manim_semantic_handles.py",
    """_ORIGINAL_GET_STROKE_OPACITY = _compat.VMobject.get_stroke_opacity\n\n\ndef _snapshot_json""",
    """_ORIGINAL_GET_STROKE_OPACITY = _compat.VMobject.get_stroke_opacity\n_ORIGINAL_GROUP_INIT = _compat.Group.__init__\n_ORIGINAL_GROUP_ADD = _compat.Group.add\n_ORIGINAL_GROUP_REMOVE = _compat.Group.remove\n\n\ndef _snapshot_json""",
    "capture original group methods",
)
replace_once(
    "web/python/_manim_semantic_handles.py",
    """\ndef install() -> None:\n""",
    """\ndef _family_member_handle(value: object) -> tuple[str | None, object | None]:\n    if isinstance(value, _compat.Group):\n        return \"family\", getattr(value, \"_semantic_family_handle\", None)\n    if isinstance(value, _base.Mobject):\n        # Family identity survives scene binding even though ordinary detached-state\n        # mutations stop using this handle after binding.\n        return \"mobject\", getattr(value, \"_semantic_handle\", None)\n    return None, None\n\n\ndef _family_add_handle(family_handle: object, value: object) -> bool:\n    kind, handle = _family_member_handle(value)\n    if handle is None:\n        raise RuntimeError(\"family member has no shared semantic identity\")\n    if kind == \"family\":\n        return bool(family_handle.addFamily(handle))\n    return bool(family_handle.addMobject(handle))\n\n\ndef _family_remove_handle(family_handle: object, value: object) -> bool:\n    kind, handle = _family_member_handle(value)\n    if handle is None:\n        raise RuntimeError(\"family member has no shared semantic identity\")\n    if kind == \"family\":\n        return bool(family_handle.removeFamily(handle))\n    return bool(family_handle.removeMobject(handle))\n\n\ndef _validate_group_members(owner: _compat.Group, mobjects: tuple[object, ...]) -> None:\n    for mobject in mobjects:\n        if not isinstance(mobject, (_base.Mobject, _compat.Group)):\n            raise TypeError(\"Group members must be Mobjects or Groups\")\n        if mobject is owner:\n            raise ValueError(\"Group cannot contain itself\")\n\n\ndef _group_init(self: _compat.Group, *mobjects: object) -> None:\n    self._semantic_family_handle = _create_family_handle()\n    _ORIGINAL_GROUP_INIT(self, *mobjects)\n\n\ndef _group_add(self: _compat.Group, *mobjects: object) -> _compat.Group:\n    _validate_group_members(self, mobjects)\n    family_handle = self._semantic_family_handle\n    for mobject in mobjects:\n        if _family_add_handle(family_handle, mobject):\n            _ORIGINAL_GROUP_ADD(self, mobject)\n    return self\n\n\ndef _group_remove(self: _compat.Group, *mobjects: object) -> _compat.Group:\n    family_handle = self._semantic_family_handle\n    for mobject in mobjects:\n        if _family_remove_handle(family_handle, mobject):\n            _ORIGINAL_GROUP_REMOVE(self, mobject)\n    return self\n\n\ndef install() -> None:\n""",
    "python shared family hooks",
)
replace_once(
    "web/python/_manim_semantic_handles.py",
    """    _compat.VMobject.get_stroke_opacity = _get_stroke_opacity\n    _compat._bounds_for = _compat_bounds_for\n""",
    """    _compat.VMobject.get_stroke_opacity = _get_stroke_opacity\n    _compat._bounds_for = _compat_bounds_for\n\n    if _create_family_handle is not None:\n        _compat.Group.__init__ = _group_init\n        _compat.Group.add = _group_add\n        _compat.Group.remove = _group_remove\n""",
    "install shared family hooks",
)

# Ownership ratchet: direct family identity/order is now shared; Python only mirrors wrappers.
replace_once(
    "compat/semantic-ownership-v1.json",
    """    {\n      \"id\": \"group-family\",\n      \"surface\": \"Group/VGroup family membership and flattening\",\n      \"classification\": \"python-semantic-duplicate\",\n      \"owner\": {\"language\": \"python\", \"path\": \"web/python/_manim_compat.py\", \"symbol\": \"Group/VGroup\"},\n      \"shared_owner\": {\"language\": \"rust\", \"path\": \"crates/noon-core/src/semantic_store.rs\", \"symbol\": \"SemanticStore::add_member/remove_member\"},\n      \"reason\": \"Python groups still own child collections and flatten family operations before lowering, while Rust has family references in the semantic store.\",\n      \"replacement\": \"Retain family identity in shared semantic handles and lower group operations from that shared representation.\",\n      \"migration_issue\": \"#61\"\n    },\n""",
    """    {\n      \"id\": \"group-family\",\n      \"surface\": \"Group/VGroup direct family identity, membership, and order\",\n      \"classification\": \"shared-rust\",\n      \"owner\": {\"language\": \"rust\", \"path\": \"crates/noon-core/src/semantic_store.rs\", \"symbol\": \"SemanticStore::insert_family/add_member/remove_member\"},\n      \"adapters\": [{\"language\": \"python\", \"path\": \"web/python/_manim_semantic_handles.py\", \"symbol\": \"_group_init/_group_add/_group_remove\"}],\n      \"reason\": \"The shared semantic store assigns stable object/family identities, owns duplicate suppression and direct-member order, and returns membership decisions that Python mirrors for wrapper identity.\"\n    },\n    {\n      \"id\": \"group-family-wrapper-mirror\",\n      \"surface\": \"Python Group wrapper references and leaf iteration\",\n      \"classification\": \"python-adapter-only\",\n      \"owner\": {\"language\": \"python\", \"path\": \"web/python/_manim_compat.py\", \"symbol\": \"Group.submobjects/_leaf_mobjects\"},\n      \"reason\": \"Python retains object references for Manim class/protocol identity, but membership transitions are accepted or rejected by the shared family handle before this mirror changes.\",\n      \"replacement\": \"Keep the wrapper mirror as host-language identity metadata; deterministic family semantics remain shared.\"\n    },\n""",
    "ownership group family ratchet",
)

# CPython fake-bridge regression for Rust-authoritative family decisions.
Path("web/python/test_manim_shared_family_identity.py").write_text(r'''import json
import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimSharedFamilyIdentityTests(unittest.TestCase):
    def test_group_wrapper_mirrors_shared_family_membership(self) -> None:
        python_dir = Path(__file__).resolve().parent
        env = os.environ.copy()
        existing_pythonpath = env.get("PYTHONPATH")
        env["PYTHONPATH"] = (
            str(python_dir)
            if not existing_pythonpath
            else os.pathsep.join((str(python_dir), existing_pythonpath))
        )

        source = textwrap.dedent(
            """
            import json

            import _manim_compat
            _manim_compat.install()
            import _manim_phase_b  # noqa: F401
            import _manim_semantic_handles as handles


            class FakeObjectHandle:
                def __init__(self, store, snapshot_json):
                    self.store = store
                    self.identity = store.allocate()
                    self.snapshot = json.loads(snapshot_json)

                def snapshotJson(self):
                    return json.dumps(self.snapshot, separators=(\",\", \":\"))

                def replaceSnapshotJson(self, snapshot_json):
                    self.snapshot = json.loads(snapshot_json)

                def cloneHandle(self):
                    clone = FakeObjectHandle(self.store, self.snapshotJson())
                    return clone

                def targetEditor(self):
                    return self.cloneHandle()

                def setFillOpacity(self, opacity):
                    fill = self.snapshot[\"style\"][\"fill\"]
                    if fill is not None:
                        fill[\"alpha\"] = float(opacity)

                def setStrokeOpacity(self, opacity):
                    stroke = self.snapshot[\"style\"][\"stroke\"]
                    if stroke is not None:
                        stroke[\"alpha\"] = float(opacity)


            class FakeFamilyHandle:
                def __init__(self, store):
                    self.store = store
                    self.identity = store.allocate()
                    self.members = []

                @property
                def memberCount(self):
                    return len(self.members)

                def _add(self, member):
                    key = member.identity
                    if key in self.members:
                        return False
                    self.members.append(key)
                    return True

                def addMobject(self, member):
                    assert member.store is self.store
                    return self._add(member)

                def addFamily(self, member):
                    assert member.store is self.store
                    return self._add(member)

                def _remove(self, member):
                    key = member.identity
                    if key not in self.members:
                        return False
                    self.members.remove(key)
                    return True

                def removeMobject(self, member):
                    return self._remove(member)

                def removeFamily(self, member):
                    return self._remove(member)


            class FakeStore:
                def __init__(self):
                    self.next_identity = 0

                def allocate(self):
                    value = self.next_identity
                    self.next_identity += 1
                    return value

                def createMobject(self, snapshot_json):
                    return FakeObjectHandle(self, snapshot_json)

                def createFamily(self):
                    return FakeFamilyHandle(self)


            store = FakeStore()
            handles._create_handle = store.createMobject
            handles._create_family_handle = store.createFamily
            handles.install()

            from noon import Circle, Square, VGroup

            first = Circle(radius=0.2)
            second = Square(side_length=0.4)
            family = VGroup(first, second)
            assert len(family) == 2
            assert family._semantic_family_handle.memberCount == 2

            # The shared family graph owns duplicate suppression. Python mirrors the
            # returned decision rather than independently appending another wrapper.
            family.add(first)
            assert len(family) == 2
            assert family._semantic_family_handle.memberCount == 2

            family.remove(first)
            assert list(family) == [second]
            assert family._semantic_family_handle.memberCount == 1
            family.remove(first)
            assert list(family) == [second]
            assert family._semantic_family_handle.memberCount == 1

            nested = VGroup(first)
            outer = VGroup(nested, second)
            assert outer._semantic_family_handle.memberCount == 2
            assert nested._semantic_family_handle.memberCount == 1

            clone = outer.copy()
            assert clone is not outer
            assert clone._semantic_family_handle is not outer._semantic_family_handle
            assert clone._semantic_family_handle.memberCount == 2
            assert clone[0] is not nested
            assert clone[1] is not second
            """
        )
        completed = subprocess.run(
            [sys.executable, "-c", source],
            cwd=python_dir,
            env=env,
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(
            completed.returncode,
            0,
            msg=f"stdout:\n{completed.stdout}\nstderr:\n{completed.stderr}",
        )


if __name__ == "__main__":
    unittest.main()
''')

# Exercise the actual WASM/Pyodide shared store in the existing browser smoke.
replace_once(
    "scripts/manim-compat-smoke.mjs",
    """        pair = VGroup(left, right).arrange(RIGHT, buff=0.4)\n\n        assert isinstance(pair, Mobject)\n""",
    """        pair = VGroup(left, right).arrange(RIGHT, buff=0.4)\n\n        assert int(pair._semantic_family_handle.memberCount) == 2\n        pair.add(left)\n        assert len(pair) == 2\n        assert int(pair._semantic_family_handle.memberCount) == 2\n        alias = VGroup(left)\n        assert int(alias._semantic_family_handle.memberCount) == 1\n\n        assert isinstance(pair, Mobject)\n""",
    "browser shared family smoke",
)
