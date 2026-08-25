from pathlib import Path


def replace_once(text: str, before: str, after: str, label: str) -> str:
    count = text.count(before)
    if count != 1:
        raise SystemExit(f"{label}: expected one match, found {count}")
    return text.replace(before, after, 1)


# --- Shared semantic family graph -------------------------------------------------
path = Path("crates/noon-core/src/semantic_store.rs")
text = path.read_text()
text = replace_once(
    text,
    """pub enum SemanticNodeKind {\n    /// Compatibility payload while `SceneDefinition` consumers migrate.\n    Object(ObjectDefinition),\n    /// A semantic family/collection with no implied transform ownership.\n    Family,\n}\n""",
    """pub enum SemanticNodeKind {\n    /// Compatibility payload while `SceneDefinition` consumers migrate.\n    Object(ObjectDefinition),\n    /// Object identity whose authoring payload is held by another shared semantic\n    /// resource. This is used by frontend stores while detached snapshots migrate\n    /// away from the legacy `ObjectDefinition` shape.\n    AuthoringObject,\n    /// A semantic family/collection with no implied transform ownership.\n    Family,\n}\n""",
    "semantic node kind",
)
text = replace_once(
    text,
    """        match &self.kind {\n            SemanticNodeKind::Object(object) => Some(object),\n            SemanticNodeKind::Family => None,\n        }\n""",
    """        match &self.kind {\n            SemanticNodeKind::Object(object) => Some(object),\n            SemanticNodeKind::AuthoringObject | SemanticNodeKind::Family => None,\n        }\n""",
    "object accessor",
)
text = replace_once(
    text,
    """        match &mut self.kind {\n            SemanticNodeKind::Object(object) => Some(object),\n            SemanticNodeKind::Family => None,\n        }\n""",
    """        match &mut self.kind {\n            SemanticNodeKind::Object(object) => Some(object),\n            SemanticNodeKind::AuthoringObject | SemanticNodeKind::Family => None,\n        }\n""",
    "object mut accessor",
)
text = replace_once(
    text,
    """    pub fn insert_family(&mut self) -> SemanticNodeId {\n        self.insert_kind(SemanticNodeKind::Family)\n    }\n""",
    """    pub fn insert_authoring_object(&mut self) -> SemanticNodeId {\n        self.insert_kind(SemanticNodeKind::AuthoringObject)\n    }\n\n    pub fn insert_family(&mut self) -> SemanticNodeId {\n        self.insert_kind(SemanticNodeKind::Family)\n    }\n""",
    "authoring object insertion",
)
add_anchor = """    pub fn remove_member(\n        &mut self,\n        family: SemanticNodeId,\n        member: SemanticNodeId,\n    ) -> Result<bool, SemanticStoreError> {\n"""
insert_methods = """    /// Insert an ordered family edge at a specific position. Unlike `add_member`,\n    /// this intentionally preserves Manim's `Mobject.insert` behavior and therefore\n    /// may create a repeated direct member. Parent identity remains set-like.\n    pub fn insert_member(\n        &mut self,\n        family: SemanticNodeId,\n        index: usize,\n        member: SemanticNodeId,\n    ) -> Result<(), SemanticStoreError> {\n        if !matches!(\n            self.node(family).map(SemanticNode::kind),\n            Some(SemanticNodeKind::Family)\n        ) {\n            return Err(SemanticStoreError::NotFamily(family));\n        }\n        if self.node(member).is_none() {\n            return Err(SemanticStoreError::UnknownNode(member));\n        }\n        let len = self.node(family).expect("family validated above").members.len();\n        if index > len {\n            return Err(SemanticStoreError::MemberIndexOutOfBounds { family, index, len });\n        }\n        if family == member {\n            return Err(SemanticStoreError::FamilyCycle { family, member });\n        }\n        let already_direct = self\n            .node(family)\n            .expect("family validated above")\n            .members\n            .contains(&member);\n        let (creates_cycle, visited) = if already_direct {\n            (false, 0)\n        } else {\n            self.reaches(member, family)\n        };\n        if creates_cycle {\n            self.last_mutation = SemanticMutationStats {\n                slots_written: 0,\n                cycle_nodes_visited: visited,\n            };\n            return Err(SemanticStoreError::FamilyCycle { family, member });\n        }\n\n        self.node_mut(family)\n            .expect("family validated above")\n            .members\n            .insert(index, member);\n        let mut writes = 1;\n        if !already_direct {\n            self.node_mut(member)\n                .expect("member validated above")\n                .parents\n                .push(family);\n            writes += 1;\n        }\n        self.last_mutation = SemanticMutationStats {\n            slots_written: writes,\n            cycle_nodes_visited: visited,\n        };\n        Ok(())\n    }\n\n    /// Reorder one direct member without changing semantic identity or parentage.\n    pub fn move_member(\n        &mut self,\n        family: SemanticNodeId,\n        from: usize,\n        to: usize,\n    ) -> Result<(), SemanticStoreError> {\n        if !matches!(\n            self.node(family).map(SemanticNode::kind),\n            Some(SemanticNodeKind::Family)\n        ) {\n            return Err(SemanticStoreError::NotFamily(family));\n        }\n        let len = self.node(family).expect("family validated above").members.len();\n        if from >= len {\n            return Err(SemanticStoreError::MemberIndexOutOfBounds {\n                family,\n                index: from,\n                len,\n            });\n        }\n        if to >= len {\n            return Err(SemanticStoreError::MemberIndexOutOfBounds {\n                family,\n                index: to,\n                len,\n            });\n        }\n        if from != to {\n            let members = &mut self\n                .node_mut(family)\n                .expect("family validated above")\n                .members;\n            let member = members.remove(from);\n            members.insert(to, member);\n            self.last_mutation = SemanticMutationStats {\n                slots_written: 1,\n                cycle_nodes_visited: 0,\n            };\n        } else {\n            self.last_mutation = SemanticMutationStats::default();\n        }\n        Ok(())\n    }\n\n""" + add_anchor
text = replace_once(text, add_anchor, insert_methods, "family insert/reorder methods")
text = replace_once(
    text,
    """        let parents = &mut self\n            .node_mut(member)\n            .expect(\"member validated above\")\n            .parents;\n        if let Some(position) = parents.iter().position(|id| *id == family) {\n            parents.remove(position);\n        }\n        self.last_mutation = SemanticMutationStats {\n            slots_written: 2,\n""",
    """        let still_direct = self\n            .node(family)\n            .expect(\"family validated above\")\n            .members\n            .contains(&member);\n        let mut writes = 1;\n        if !still_direct {\n            let parents = &mut self\n                .node_mut(member)\n                .expect(\"member validated above\")\n                .parents;\n            if let Some(position) = parents.iter().position(|id| *id == family) {\n                parents.remove(position);\n                writes += 1;\n            }\n        }\n        self.last_mutation = SemanticMutationStats {\n            slots_written: writes,\n""",
    "duplicate-aware removal",
)
text = replace_once(
    text,
    """    DuplicateSourceIdentity(SourceIdentity),\n}\n""",
    """    DuplicateSourceIdentity(SourceIdentity),\n    MemberIndexOutOfBounds {\n        family: SemanticNodeId,\n        index: usize,\n        len: usize,\n    },\n}\n""",
    "semantic store error variant",
)
text = replace_once(
    text,
    """            Self::DuplicateSourceIdentity(source) => {\n                write!(formatter, \"duplicate semantic source identity {source:?}\")\n            }\n""",
    """            Self::DuplicateSourceIdentity(source) => {\n                write!(formatter, \"duplicate semantic source identity {source:?}\")\n            }\n            Self::MemberIndexOutOfBounds { family, index, len } => write!(\n                formatter,\n                \"family {}:{} member index {index} is out of bounds for length {len}\",\n                family.slot(),\n                family.generation()\n            ),\n""",
    "semantic store error display",
)
test_anchor = """    #[test]\n    fn reused_slot_invalidates_stale_generation() {\n"""
family_test = """    #[test]\n    fn ordered_authoring_family_supports_aliasing_insertion_and_reorder() {\n        let mut store = SemanticStore::new();\n        let first = store.insert_authoring_object();\n        let second = store.insert_authoring_object();\n        let alias_parent = store.insert_family();\n        let family = store.insert_family();\n\n        store.add_member(family, first).unwrap();\n        store.add_member(family, second).unwrap();\n        store.add_member(alias_parent, first).unwrap();\n        assert_eq!(store.node(first).unwrap().parents().len(), 2);\n\n        store.insert_member(family, 1, first).unwrap();\n        assert_eq!(store.node(family).unwrap().members(), &[first, first, second]);\n        assert_eq!(store.node(first).unwrap().parents().len(), 2);\n\n        store.move_member(family, 2, 0).unwrap();\n        assert_eq!(store.node(family).unwrap().members(), &[second, first, first]);\n\n        store.remove_member(family, first).unwrap();\n        assert!(store.node(first).unwrap().parents().contains(&family));\n        store.remove_member(family, first).unwrap();\n        assert!(!store.node(first).unwrap().parents().contains(&family));\n        assert!(store.node(first).unwrap().parents().contains(&alias_parent));\n    }\n\n""" + test_anchor
text = replace_once(text, test_anchor, family_test, "semantic family test")
path.write_text(text)


# --- WASM authoring handles share the semantic graph -----------------------------
path = Path("crates/noon-web/src/authoring_mobject.rs")
text = path.read_text()
text = replace_once(
    text,
    """mod wasm {\n    use wasm_bindgen::prelude::*;\n\n    use super::FrontendMobjectHandle;\n""",
    """mod wasm {\n    use std::{cell::RefCell, rc::Rc};\n\n    use noon_core::{SemanticNodeId, SemanticStore};\n    use wasm_bindgen::prelude::*;\n\n    use super::FrontendMobjectHandle;\n""",
    "wasm imports",
)
old_header = """    #[wasm_bindgen]\n    pub struct WasmAuthoringMobjectHandle(FrontendMobjectHandle);\n\n    #[wasm_bindgen]\n    impl WasmAuthoringMobjectHandle {\n        #[wasm_bindgen(constructor)]\n        pub fn new(snapshot_json: &str) -> Result<WasmAuthoringMobjectHandle, JsValue> {\n            FrontendMobjectHandle::from_json(snapshot_json)\n                .map(WasmAuthoringMobjectHandle)\n                .map_err(js_error)\n        }\n\n        #[wasm_bindgen(js_name = cloneHandle)]\n        pub fn clone_handle(&self) -> WasmAuthoringMobjectHandle {\n            WasmAuthoringMobjectHandle(self.0.clone())\n        }\n"""
new_header = """    type SharedSemanticStore = Rc<RefCell<SemanticStore>>;\n\n    #[wasm_bindgen]\n    pub struct WasmAuthoringStore {\n        semantics: SharedSemanticStore,\n    }\n\n    #[wasm_bindgen]\n    impl WasmAuthoringStore {\n        #[wasm_bindgen(constructor)]\n        pub fn new() -> Self {\n            Self {\n                semantics: Rc::new(RefCell::new(SemanticStore::new())),\n            }\n        }\n\n        #[wasm_bindgen(js_name = createMobject)]\n        pub fn create_mobject(\n            &self,\n            snapshot_json: &str,\n        ) -> Result<WasmAuthoringMobjectHandle, JsValue> {\n            let handle = FrontendMobjectHandle::from_json(snapshot_json).map_err(js_error)?;\n            let id = self.semantics.borrow_mut().insert_authoring_object();\n            Ok(WasmAuthoringMobjectHandle(\n                handle,\n                Some(Rc::clone(&self.semantics)),\n                Some(id),\n            ))\n        }\n\n        #[wasm_bindgen(js_name = createFamily)]\n        pub fn create_family(&self) -> WasmAuthoringFamilyHandle {\n            let id = self.semantics.borrow_mut().insert_family();\n            WasmAuthoringFamilyHandle {\n                semantics: Rc::clone(&self.semantics),\n                id,\n            }\n        }\n    }\n\n    #[wasm_bindgen]\n    pub struct WasmAuthoringFamilyHandle {\n        semantics: SharedSemanticStore,\n        id: SemanticNodeId,\n    }\n\n    impl WasmAuthoringFamilyHandle {\n        fn object_member_id(\n            &self,\n            member: &WasmAuthoringMobjectHandle,\n        ) -> Result<SemanticNodeId, JsValue> {\n            let store = member\n                .1\n                .as_ref()\n                .ok_or_else(|| JsValue::from_str(\"mobject is not attached to a shared authoring store\"))?;\n            if !Rc::ptr_eq(&self.semantics, store) {\n                return Err(JsValue::from_str(\n                    \"family and mobject belong to different authoring stores\",\n                ));\n            }\n            member\n                .2\n                .ok_or_else(|| JsValue::from_str(\"mobject has no semantic identity\"))\n        }\n\n        fn family_member_id(\n            &self,\n            member: &WasmAuthoringFamilyHandle,\n        ) -> Result<SemanticNodeId, JsValue> {\n            if !Rc::ptr_eq(&self.semantics, &member.semantics) {\n                return Err(JsValue::from_str(\n                    \"families belong to different authoring stores\",\n                ));\n            }\n            Ok(member.id)\n        }\n\n        fn add_id(&mut self, member: SemanticNodeId) -> Result<(), JsValue> {\n            self.semantics\n                .borrow_mut()\n                .add_member(self.id, member)\n                .map_err(|error| js_error(error.to_string()))\n        }\n\n        fn insert_id(&mut self, index: usize, member: SemanticNodeId) -> Result<(), JsValue> {\n            self.semantics\n                .borrow_mut()\n                .insert_member(self.id, index, member)\n                .map_err(|error| js_error(error.to_string()))\n        }\n\n        fn remove_id(&mut self, member: SemanticNodeId) -> Result<bool, JsValue> {\n            self.semantics\n                .borrow_mut()\n                .remove_member(self.id, member)\n                .map_err(|error| js_error(error.to_string()))\n        }\n    }\n\n    #[wasm_bindgen]\n    impl WasmAuthoringFamilyHandle {\n        #[wasm_bindgen(getter, js_name = semanticSlot)]\n        pub fn semantic_slot(&self) -> u32 {\n            self.id.slot()\n        }\n\n        #[wasm_bindgen(getter, js_name = semanticGeneration)]\n        pub fn semantic_generation(&self) -> u32 {\n            self.id.generation()\n        }\n\n        #[wasm_bindgen(getter, js_name = memberCount)]\n        pub fn member_count(&self) -> usize {\n            self.semantics\n                .borrow()\n                .node(self.id)\n                .map_or(0, |node| node.members().len())\n        }\n\n        #[wasm_bindgen(js_name = memberSlot)]\n        pub fn member_slot(&self, index: usize) -> Result<u32, JsValue> {\n            self.semantics\n                .borrow()\n                .node(self.id)\n                .and_then(|node| node.members().get(index).copied())\n                .map(SemanticNodeId::slot)\n                .ok_or_else(|| JsValue::from_str(\"family member index is out of bounds\"))\n        }\n\n        #[wasm_bindgen(js_name = memberGeneration)]\n        pub fn member_generation(&self, index: usize) -> Result<u32, JsValue> {\n            self.semantics\n                .borrow()\n                .node(self.id)\n                .and_then(|node| node.members().get(index).copied())\n                .map(SemanticNodeId::generation)\n                .ok_or_else(|| JsValue::from_str(\"family member index is out of bounds\"))\n        }\n\n        #[wasm_bindgen(js_name = addMobject)]\n        pub fn add_mobject(\n            &mut self,\n            member: &WasmAuthoringMobjectHandle,\n        ) -> Result<(), JsValue> {\n            let id = self.object_member_id(member)?;\n            self.add_id(id)\n        }\n\n        #[wasm_bindgen(js_name = addFamily)]\n        pub fn add_family(\n            &mut self,\n            member: &WasmAuthoringFamilyHandle,\n        ) -> Result<(), JsValue> {\n            let id = self.family_member_id(member)?;\n            self.add_id(id)\n        }\n\n        #[wasm_bindgen(js_name = insertMobject)]\n        pub fn insert_mobject(\n            &mut self,\n            index: usize,\n            member: &WasmAuthoringMobjectHandle,\n        ) -> Result<(), JsValue> {\n            let id = self.object_member_id(member)?;\n            self.insert_id(index, id)\n        }\n\n        #[wasm_bindgen(js_name = insertFamily)]\n        pub fn insert_family(\n            &mut self,\n            index: usize,\n            member: &WasmAuthoringFamilyHandle,\n        ) -> Result<(), JsValue> {\n            let id = self.family_member_id(member)?;\n            self.insert_id(index, id)\n        }\n\n        #[wasm_bindgen(js_name = removeMobject)]\n        pub fn remove_mobject(\n            &mut self,\n            member: &WasmAuthoringMobjectHandle,\n        ) -> Result<bool, JsValue> {\n            let id = self.object_member_id(member)?;\n            self.remove_id(id)\n        }\n\n        #[wasm_bindgen(js_name = removeFamily)]\n        pub fn remove_family(\n            &mut self,\n            member: &WasmAuthoringFamilyHandle,\n        ) -> Result<bool, JsValue> {\n            let id = self.family_member_id(member)?;\n            self.remove_id(id)\n        }\n\n        #[wasm_bindgen(js_name = moveMember)]\n        pub fn move_member(&mut self, from: usize, to: usize) -> Result<(), JsValue> {\n            self.semantics\n                .borrow_mut()\n                .move_member(self.id, from, to)\n                .map_err(|error| js_error(error.to_string()))\n        }\n    }\n\n    #[wasm_bindgen]\n    pub struct WasmAuthoringMobjectHandle(\n        FrontendMobjectHandle,\n        Option<SharedSemanticStore>,\n        Option<SemanticNodeId>,\n    );\n\n    #[wasm_bindgen]\n    impl WasmAuthoringMobjectHandle {\n        #[wasm_bindgen(constructor)]\n        pub fn new(snapshot_json: &str) -> Result<WasmAuthoringMobjectHandle, JsValue> {\n            FrontendMobjectHandle::from_json(snapshot_json)\n                .map(|handle| WasmAuthoringMobjectHandle(handle, None, None))\n                .map_err(js_error)\n        }\n\n        #[wasm_bindgen(getter, js_name = semanticSlot)]\n        pub fn semantic_slot(&self) -> Result<u32, JsValue> {\n            self.2\n                .map(SemanticNodeId::slot)\n                .ok_or_else(|| JsValue::from_str(\"mobject has no shared semantic identity\"))\n        }\n\n        #[wasm_bindgen(getter, js_name = semanticGeneration)]\n        pub fn semantic_generation(&self) -> Result<u32, JsValue> {\n            self.2\n                .map(SemanticNodeId::generation)\n                .ok_or_else(|| JsValue::from_str(\"mobject has no shared semantic identity\"))\n        }\n\n        #[wasm_bindgen(js_name = cloneHandle)]\n        pub fn clone_handle(&self) -> WasmAuthoringMobjectHandle {\n            if let Some(store) = &self.1 {\n                let id = store.borrow_mut().insert_authoring_object();\n                WasmAuthoringMobjectHandle(self.0.clone(), Some(Rc::clone(store)), Some(id))\n            } else {\n                WasmAuthoringMobjectHandle(self.0.clone(), None, None)\n            }\n        }\n"""
text = replace_once(text, old_header, new_header, "wasm shared semantic store")
path.write_text(text)


# --- Worker creates all handles from one shared semantic store --------------------
path = Path("web/python-worker.js")
text = path.read_text()
text = replace_once(
    text,
    """import initNoonWeb, {\n  WasmAuthoringMobjectHandle,\n""",
    """import initNoonWeb, {\n  WasmAuthoringStore,\n""",
    "worker wasm import",
)
text = replace_once(
    text,
    """  await initNoonWeb();\n  self.noonCreateAuthoringMobjectHandle = (snapshotJson) =>\n    new WasmAuthoringMobjectHandle(snapshotJson);\n""",
    """  await initNoonWeb();\n  const authoringStore = new WasmAuthoringStore();\n  self.noonCreateAuthoringMobjectHandle = (snapshotJson) =>\n    authoringStore.createMobject(snapshotJson);\n  self.noonCreateAuthoringFamilyHandle = () => authoringStore.createFamily();\n""",
    "worker shared handle factories",
)
path.write_text(text)


# --- Native observable Group behavior follows Manim v0.21 ------------------------
path = Path("web/python/_manim_compat.py")
text = path.read_text()
text = replace_once(
    text,
    """    def __getitem__(self, index: int) -> object:\n        return self.submobjects[index]\n\n    def add(self, *mobjects: object) -> Group:\n        for mobject in mobjects:\n            if not isinstance(mobject, (_BaseMobject, Group)):\n                raise TypeError(\"Group members must be Mobjects or Groups\")\n            if mobject is self:\n                raise ValueError(\"Group cannot contain itself\")\n            self.submobjects.append(mobject)\n        return self\n\n    def remove(self, *mobjects: object) -> Group:\n        identities = {id(mobject) for mobject in mobjects}\n        self.submobjects = [\n            mobject for mobject in self.submobjects if id(mobject) not in identities\n        ]\n        return self\n""",
    """    def __getitem__(self, index: int | slice) -> object:\n        if isinstance(index, slice):\n            return type(self)(*self.submobjects[index])\n        return self.submobjects[index]\n\n    @staticmethod\n    def _validate_members(owner: Group, mobjects: tuple[object, ...]) -> None:\n        for mobject in mobjects:\n            if not isinstance(mobject, (_BaseMobject, Group)):\n                raise TypeError(\"Group members must be Mobjects or Groups\")\n            if mobject is owner:\n                raise ValueError(\"Group cannot contain itself\")\n\n    def add(self, *mobjects: object) -> Group:\n        self._validate_members(self, mobjects)\n        for mobject in mobjects:\n            if not any(existing is mobject for existing in self.submobjects):\n                self.submobjects.append(mobject)\n        return self\n\n    def insert(self, index: int, mobject: object) -> None:\n        self._validate_members(self, (mobject,))\n        self.submobjects.insert(int(index), mobject)\n\n    def add_to_back(self, *mobjects: object) -> Group:\n        self._validate_members(self, mobjects)\n        unique: list[object] = []\n        for mobject in mobjects:\n            if not any(existing is mobject for existing in unique):\n                unique.append(mobject)\n        for mobject in unique:\n            if mobject in self.submobjects:\n                self.submobjects.remove(mobject)\n        self.submobjects = unique + self.submobjects\n        return self\n\n    def remove(self, *mobjects: object) -> Group:\n        for mobject in mobjects:\n            if mobject in self.submobjects:\n                self.submobjects.remove(mobject)\n        return self\n""",
    "group observable membership behavior",
)
text = text.replace(
    """    \"\"\"Authoring-time Mobject-family group lowered to operations on member objects.\n\n    Noon intentionally keeps runtime hierarchy flat. The group therefore has no single\n    serialized object ID; its transforms and animations lower to its leaf members.\n    \"\"\"\n""",
    """    \"\"\"Manim-compatible family wrapper.\n\n    The browser layer attaches this Python object-reference mirror to Noon's shared\n    Rust semantic family graph. Renderer lowering still targets leaf resources, but\n    membership identity and ordering are no longer Python-only semantics.\n    \"\"\"\n""",
)
path.write_text(text)


# --- Browser Group mutations update the Rust semantic graph first ----------------
path = Path("web/python/_manim_semantic_handles.py")
text = path.read_text()
text = replace_once(
    text,
    """try:\n    from js import noonCreateAuthoringMobjectHandle as _create_handle\nexcept ImportError:  # Native CPython tests do not have the browser bridge.\n    _create_handle = None\n""",
    """try:\n    from js import (\n        noonCreateAuthoringFamilyHandle as _create_family_handle,\n        noonCreateAuthoringMobjectHandle as _create_handle,\n    )\nexcept ImportError:  # Native CPython tests do not have the browser bridge.\n    _create_handle = None\n    _create_family_handle = None\n""",
    "family bridge import",
)
text = replace_once(
    text,
    """_ORIGINAL_GET_STROKE_OPACITY = _compat.VMobject.get_stroke_opacity\n\n\ndef _snapshot_json""",
    """_ORIGINAL_GET_STROKE_OPACITY = _compat.VMobject.get_stroke_opacity\n_ORIGINAL_GROUP_INIT = _compat.Group.__init__\n_ORIGINAL_GROUP_ADD = _compat.Group.add\n_ORIGINAL_GROUP_INSERT = _compat.Group.insert\n_ORIGINAL_GROUP_ADD_TO_BACK = _compat.Group.add_to_back\n_ORIGINAL_GROUP_REMOVE = _compat.Group.remove\n\n\ndef _snapshot_json""",
    "group original methods",
)
install_anchor = """def install() -> None:\n"""
family_bridge = r'''
def _family_member_handle(value: object):
    if isinstance(value, _compat.Group):
        handle = getattr(value, "_semantic_family_handle", None)
        return "family", handle
    if isinstance(value, _base.Mobject):
        handle = getattr(value, "_semantic_handle", None)
        return "mobject", handle
    return None, None


def _family_add_handle(family_handle: object, value: object) -> None:
    kind, handle = _family_member_handle(value)
    if handle is None:
        raise RuntimeError("family member has no shared semantic identity")
    if kind == "family":
        family_handle.addFamily(handle)
    else:
        family_handle.addMobject(handle)


def _family_insert_handle(family_handle: object, index: int, value: object) -> None:
    kind, handle = _family_member_handle(value)
    if handle is None:
        raise RuntimeError("family member has no shared semantic identity")
    if kind == "family":
        family_handle.insertFamily(index, handle)
    else:
        family_handle.insertMobject(index, handle)


def _family_remove_handle(family_handle: object, value: object) -> None:
    kind, handle = _family_member_handle(value)
    if handle is None:
        raise RuntimeError("family member has no shared semantic identity")
    if kind == "family":
        family_handle.removeFamily(handle)
    else:
        family_handle.removeMobject(handle)


def _group_init(self: _compat.Group, *mobjects: object) -> None:
    self._semantic_family_handle = _create_family_handle()
    _ORIGINAL_GROUP_INIT(self, *mobjects)


def _group_add(self: _compat.Group, *mobjects: object) -> _compat.Group:
    self._validate_members(self, mobjects)
    family_handle = self._semantic_family_handle
    for mobject in mobjects:
        if any(existing is mobject for existing in self.submobjects):
            continue
        _family_add_handle(family_handle, mobject)
    return _ORIGINAL_GROUP_ADD(self, *mobjects)


def _normalized_insert_index(length: int, index: int) -> int:
    index = int(index)
    if index < 0:
        return max(0, length + index)
    return min(length, index)


def _group_insert(self: _compat.Group, index: int, mobject: object) -> None:
    self._validate_members(self, (mobject,))
    normalized = _normalized_insert_index(len(self.submobjects), index)
    _family_insert_handle(self._semantic_family_handle, normalized, mobject)
    _ORIGINAL_GROUP_INSERT(self, index, mobject)


def _group_add_to_back(self: _compat.Group, *mobjects: object) -> _compat.Group:
    self._validate_members(self, mobjects)
    unique: list[object] = []
    for mobject in mobjects:
        if not any(existing is mobject for existing in unique):
            unique.append(mobject)

    family_handle = self._semantic_family_handle
    # Match Manim's remove-then-prepend rule while keeping the Rust graph authoritative.
    for mobject in unique:
        if any(existing is mobject for existing in self.submobjects):
            _family_remove_handle(family_handle, mobject)
    for mobject in reversed(unique):
        _family_insert_handle(family_handle, 0, mobject)
    return _ORIGINAL_GROUP_ADD_TO_BACK(self, *mobjects)


def _group_remove(self: _compat.Group, *mobjects: object) -> _compat.Group:
    family_handle = self._semantic_family_handle
    shadow = list(self.submobjects)
    for mobject in mobjects:
        if mobject in shadow:
            _family_remove_handle(family_handle, mobject)
            shadow.remove(mobject)
    return _ORIGINAL_GROUP_REMOVE(self, *mobjects)


''' + install_anchor
text = replace_once(text, install_anchor, family_bridge, "browser family synchronization")
text = replace_once(
    text,
    """    _compat.VMobject.get_stroke_opacity = _get_stroke_opacity\n    _compat._bounds_for = _compat_bounds_for\n""",
    """    _compat.VMobject.get_stroke_opacity = _get_stroke_opacity\n    _compat._bounds_for = _compat_bounds_for\n\n    _compat.Group.__init__ = _group_init\n    _compat.Group.add = _group_add\n    _compat.Group.insert = _group_insert\n    _compat.Group.add_to_back = _group_add_to_back\n    _compat.Group.remove = _group_remove\n""",
    "install browser family synchronization",
)
path.write_text(text)


# --- Differential family coverage -------------------------------------------------
path = Path("scripts/manim-differential.py")
text = path.read_text()
fixture_anchor = """FIXTURES = [\n"""
family_fixtures = r'''
def _member_centers(group: Any) -> list[list[float]]:
    return [_point_observation(member.get_center()) for member in group.submobjects]


def _noon_vgroup_duplicate_add() -> Any:
    first = noon.Circle(radius=0.2).shift(noon.LEFT)
    group = noon.VGroup(first).add(first, first)
    return {"length": len(group), "centers": _member_centers(group)}


def _manim_vgroup_duplicate_add() -> Any:
    first = manim.Circle(radius=0.2).shift(manim.LEFT)
    group = manim.VGroup(first).add(first, first)
    return {"length": len(group), "centers": _member_centers(group)}


def _noon_vgroup_insert() -> Any:
    first = noon.Circle(radius=0.2).shift(noon.LEFT)
    second = noon.Square(side_length=0.4).shift(noon.RIGHT)
    group = noon.VGroup(first, second)
    result = group.insert(1, first)
    return {"return_is_none": result is None, "centers": _member_centers(group)}


def _manim_vgroup_insert() -> Any:
    first = manim.Circle(radius=0.2).shift(manim.LEFT)
    second = manim.Square(side_length=0.4).shift(manim.RIGHT)
    group = manim.VGroup(first, second)
    result = group.insert(1, first)
    return {"return_is_none": result is None, "centers": _member_centers(group)}


def _noon_vgroup_add_to_back() -> Any:
    first = noon.Circle(radius=0.2).shift(noon.LEFT)
    second = noon.Square(side_length=0.4)
    third = noon.Rectangle(width=0.4, height=0.2).shift(noon.RIGHT)
    group = noon.VGroup(first, second, third).add_to_back(third, first)
    return _member_centers(group)


def _manim_vgroup_add_to_back() -> Any:
    first = manim.Circle(radius=0.2).shift(manim.LEFT)
    second = manim.Square(side_length=0.4)
    third = manim.Rectangle(width=0.4, height=0.2).shift(manim.RIGHT)
    group = manim.VGroup(first, second, third).add_to_back(third, first)
    return _member_centers(group)


def _noon_vgroup_slice() -> Any:
    group = noon.VGroup(
        noon.Circle(radius=0.2).shift(noon.LEFT),
        noon.Square(side_length=0.4),
        noon.Rectangle(width=0.4, height=0.2).shift(noon.RIGHT),
    )
    subset = group[1:]
    return {"type": type(subset).__name__, "centers": _member_centers(subset)}


def _manim_vgroup_slice() -> Any:
    group = manim.VGroup(
        manim.Circle(radius=0.2).shift(manim.LEFT),
        manim.Square(side_length=0.4),
        manim.Rectangle(width=0.4, height=0.2).shift(manim.RIGHT),
    )
    subset = group[1:]
    return {"type": type(subset).__name__, "centers": _member_centers(subset)}


def _noon_nested_family_alias() -> Any:
    shared = noon.Circle(radius=0.2).shift(noon.LEFT)
    inner = noon.VGroup(shared)
    outer = noon.Group(inner, shared)
    inner.shift(noon.RIGHT * 0.5)
    return {
        "outer_length": len(outer),
        "inner_length": len(inner),
        "shared": _point_observation(shared.get_center()),
        "outer": _members_observation(outer),
    }


def _manim_nested_family_alias() -> Any:
    shared = manim.Circle(radius=0.2).shift(manim.LEFT)
    inner = manim.VGroup(shared)
    outer = manim.Group(inner, shared)
    inner.shift(manim.RIGHT * 0.5)
    return {
        "outer_length": len(outer),
        "inner_length": len(inner),
        "shared": _point_observation(shared.get_center()),
        "outer": _members_observation(outer),
    }


''' + fixture_anchor
text = replace_once(text, fixture_anchor, family_fixtures, "family differential functions")
text = replace_once(
    text,
    """    Fixture(\"vgroup_add_remove\", _noon_vgroup_add_remove, _manim_vgroup_add_remove),\n""",
    """    Fixture(\"vgroup_add_remove\", _noon_vgroup_add_remove, _manim_vgroup_add_remove),\n    Fixture(\"vgroup_duplicate_add\", _noon_vgroup_duplicate_add, _manim_vgroup_duplicate_add),\n    Fixture(\"vgroup_insert\", _noon_vgroup_insert, _manim_vgroup_insert),\n    Fixture(\"vgroup_add_to_back\", _noon_vgroup_add_to_back, _manim_vgroup_add_to_back),\n    Fixture(\"vgroup_slice\", _noon_vgroup_slice, _manim_vgroup_slice),\n    Fixture(\"nested_family_alias\", _noon_nested_family_alias, _manim_nested_family_alias),\n""",
    "family differential registration",
)
text = replace_once(
    text,
    """    \"family_aliasing\": \"Python Group still flattens family identity pending shared semantic handles (#61)\",\n""",
    "",
    "family unsupported removal",
)
path.write_text(text)


# --- Browser smoke proves Python mirror == Rust semantic membership ---------------
path = Path("scripts/manim-compat-smoke.mjs")
text = path.read_text()
insert_before = """const animateParitySource = `\n"""
family_source = r'''const semanticFamilySource = `
from noon import *

class SemanticFamilyIdentity(Scene):
    def construct(self):
        left = Circle(radius=0.2).shift(LEFT)
        middle = Square(side_length=0.4)
        right = Rectangle(width=0.4, height=0.2).shift(RIGHT)
        family = VGroup(left, middle, right)
        rust = family._semantic_family_handle
        assert int(rust.memberCount) == 3
        assert int(rust.memberSlot(0)) == int(left._semantic_handle.semanticSlot)
        assert int(rust.memberGeneration(0)) == int(left._semantic_handle.semanticGeneration)

        # add() de-duplicates; insert() intentionally preserves Manim's duplicate rule.
        family.add(left, left)
        assert len(family) == 3 and int(rust.memberCount) == 3
        family.insert(1, left)
        assert len(family) == 4 and int(rust.memberCount) == 4
        assert int(rust.memberSlot(0)) == int(rust.memberSlot(1))

        family.add_to_back(right, left)
        assert family[0] is right and family[1] is left
        assert int(rust.memberSlot(0)) == int(right._semantic_handle.semanticSlot)
        assert int(rust.memberSlot(1)) == int(left._semantic_handle.semanticSlot)

        # A slice gets its own family identity while aliasing the same child identities.
        subset = family[1:3]
        subset_rust = subset._semantic_family_handle
        assert int(subset_rust.memberCount) == 2
        assert int(subset_rust.memberSlot(0)) == int(left._semantic_handle.semanticSlot)

        # The same object may belong to multiple families without copying its semantic node.
        alias = VGroup(left)
        assert int(alias._semantic_family_handle.memberSlot(0)) == int(left._semantic_handle.semanticSlot)

        family.remove(left)
        assert len(family) == 2 and int(rust.memberCount) == 2
        self.add(family)
`;

''' + insert_before
text = replace_once(text, insert_before, family_source, "browser family source")
run_anchor = """  const animateParity = await page.evaluate(\n"""
family_run = r'''  const semanticFamily = await page.evaluate(
    (pythonSource) => window.noonManimCompat.run(pythonSource),
    semanticFamilySource,
  );
  assert.equal(semanticFamily.kind, "scene_document");
  assert.equal(semanticFamily.document.objects.length, 2);

''' + run_anchor
text = replace_once(text, run_anchor, family_run, "browser family execution")
text = text.replace(
    "scene/group semantics, callable and chained animate builders",
    "scene/group semantics, shared semantic family identity/order, callable and chained animate builders",
)
path.write_text(text)
