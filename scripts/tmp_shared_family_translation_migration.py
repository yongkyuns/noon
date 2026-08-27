from __future__ import annotations

import json
from pathlib import Path


def replace_once(text: str, old: str, new: str, *, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected exactly one anchor, found {count}")
    return text.replace(old, new, 1)


# Rust shared family translation semantics.
rust_path = Path("crates/noon-web/src/authoring_mobject.rs")
rust = rust_path.read_text()

rust = replace_once(
    rust,
    '#[cfg(any(target_arch = "wasm32", test))]\nfn semantic_family_leaf_ids(',
    'fn semantic_family_leaf_ids(',
    label="ungate semantic_family_leaf_ids",
)

translation_core = r'''

/// Ordered family translation over authoritative shared semantic leaf identity.
///
/// Frontends may retain wrapper trees for language-level identity, but the shared
/// semantic family decides which leaves are mutated and in what order. The delta is
/// validated once in Rust and then applied directly to each shared leaf handle.
#[derive(Clone, Debug)]
pub struct FrontendFamilyTranslation {
    source_members: Vec<SemanticNodeId>,
    next_index: usize,
    delta: (f64, f64),
}

impl FrontendFamilyTranslation {
    pub fn begin(
        store: &SemanticStore,
        source: SemanticNodeId,
        delta_x: f64,
        delta_y: f64,
    ) -> Result<Self, String> {
        let source_members = semantic_family_leaf_ids(store, source)?;
        Self::from_members(source_members, delta_x, delta_y)
    }

    fn from_members(
        source_members: Vec<SemanticNodeId>,
        delta_x: f64,
        delta_y: f64,
    ) -> Result<Self, String> {
        let delta = semantic_xy_f64(delta_x, delta_y)?;
        Ok(Self {
            source_members,
            next_index: 0,
            delta: (delta.x, delta.y),
        })
    }

    pub fn apply(
        &mut self,
        source_member: SemanticNodeId,
        member: &mut FrontendMobjectHandle,
    ) -> Result<(), String> {
        let expected = self
            .source_members
            .get(self.next_index)
            .copied()
            .ok_or_else(|| "family translation has no remaining leaves".to_owned())?;
        if source_member != expected {
            return Err(format!(
                "family translation leaf mismatch at index {}: expected {expected:?}, got {source_member:?}",
                self.next_index
            ));
        }
        member.shift(self.delta.0, self.delta.1)?;
        self.next_index += 1;
        Ok(())
    }

    pub fn finish(&self) -> Result<(), String> {
        if self.next_index != self.source_members.len() {
            return Err(format!(
                "family translation is incomplete: applied {} of {} leaves",
                self.next_index,
                self.source_members.len()
            ));
        }
        Ok(())
    }
}
'''

rust = replace_once(
    rust,
    '\nfn semantic_family_leaf_ids(\n',
    translation_core + '\nfn semantic_family_leaf_ids(\n',
    label="insert FrontendFamilyTranslation",
)

rust = replace_once(
    rust,
    "        semantic_family_leaf_ids, Bounds2D64, FrontendFamilyTargetEditor, FrontendMobjectHandle,\n        ManimNextToArgs, SemanticNodeId, SemanticStore,\n",
    "        semantic_family_leaf_ids, Bounds2D64, FrontendFamilyTargetEditor,\n        FrontendFamilyTranslation, FrontendMobjectHandle, ManimNextToArgs, SemanticNodeId,\n        SemanticStore,\n",
    label="import FrontendFamilyTranslation into wasm module",
)

layout_struct = '''    #[wasm_bindgen]\n    pub struct WasmAuthoringFamilyLayout {\n        semantics: SharedSemanticStore,\n        expected_leaves: Vec<SemanticNodeId>,\n        next_leaf: usize,\n        bounds: Option<Bounds2D64>,\n    }\n'''
translation_struct = layout_struct + '''\n    #[wasm_bindgen]\n    pub struct WasmAuthoringFamilyTranslation {\n        semantics: SharedSemanticStore,\n        translation: FrontendFamilyTranslation,\n    }\n'''
rust = replace_once(
    rust,
    layout_struct,
    translation_struct,
    label="insert WasmAuthoringFamilyTranslation",
)

layout_helper_anchor = '''            Ok(())\n        }\n    }\n\n    #[wasm_bindgen]\n    impl WasmAuthoringFamilyLayout {\n'''
layout_helper_replacement = '''            Ok(())\n        }\n\n        fn translation(\n            &self,\n            delta_x: f64,\n            delta_y: f64,\n        ) -> Result<WasmAuthoringFamilyTranslation, JsValue> {\n            self.ensure_complete()?;\n            let translation = FrontendFamilyTranslation::from_members(\n                self.expected_leaves.clone(),\n                delta_x,\n                delta_y,\n            )\n            .map_err(js_error)?;\n            Ok(WasmAuthoringFamilyTranslation {\n                semantics: Rc::clone(&self.semantics),\n                translation,\n            })\n        }\n\n        fn validate_target_mobject(\n            &self,\n            member: &WasmAuthoringMobjectHandle,\n        ) -> Result<(), JsValue> {\n            let store = member.1.as_ref().ok_or_else(|| {\n                JsValue::from_str(\n                    \"family placement target is not attached to a shared authoring store\",\n                )\n            })?;\n            if !Rc::ptr_eq(&self.semantics, store) {\n                return Err(JsValue::from_str(\n                    \"family placement source and target belong to different authoring stores\",\n                ));\n            }\n            if member.2.is_none() {\n                return Err(JsValue::from_str(\n                    \"family placement target has no semantic identity\",\n                ));\n            }\n            Ok(())\n        }\n    }\n\n    #[wasm_bindgen]\n    impl WasmAuthoringFamilyLayout {\n'''
rust = replace_once(
    rust,
    layout_helper_anchor,
    layout_helper_replacement,
    label="add family layout translation helpers",
)

placement_methods = r'''
        #[wasm_bindgen(js_name = shiftBy)]
        pub fn shift_by(
            &self,
            delta_x: f64,
            delta_y: f64,
        ) -> Result<WasmAuthoringFamilyTranslation, JsValue> {
            self.translation(delta_x, delta_y)
        }

        #[wasm_bindgen(js_name = moveToPoint)]
        pub fn move_to_point(
            &self,
            point_x: f64,
            point_y: f64,
            aligned_edge_x: f64,
            aligned_edge_y: f64,
            mask_x: f64,
            mask_y: f64,
        ) -> Result<WasmAuthoringFamilyTranslation, JsValue> {
            let point = semantic_xy_f64(point_x, point_y).map_err(js_error)?;
            let edge = semantic_xy_f64(aligned_edge_x, aligned_edge_y).map_err(js_error)?;
            let mask = semantic_xy_f64(mask_x, mask_y).map_err(js_error)?;
            let source_x = self.critical_x(edge.x, edge.y)?;
            let source_y = self.critical_y(edge.x, edge.y)?;
            self.translation(
                (point.x - source_x) * mask.x,
                (point.y - source_y) * mask.y,
            )
        }

        #[wasm_bindgen(js_name = moveToMobject)]
        pub fn move_to_mobject(
            &self,
            target: &WasmAuthoringMobjectHandle,
            aligned_edge_x: f64,
            aligned_edge_y: f64,
            mask_x: f64,
            mask_y: f64,
        ) -> Result<WasmAuthoringFamilyTranslation, JsValue> {
            self.ensure_complete()?;
            self.validate_target_mobject(target)?;
            let edge = semantic_xy_f64(aligned_edge_x, aligned_edge_y).map_err(js_error)?;
            let mask = semantic_xy_f64(mask_x, mask_y).map_err(js_error)?;
            let source_x = self.critical_x(edge.x, edge.y)?;
            let source_y = self.critical_y(edge.x, edge.y)?;
            let target_point = target.0.critical_point(edge.x, edge.y);
            self.translation(
                (target_point.0 - source_x) * mask.x,
                (target_point.1 - source_y) * mask.y,
            )
        }

        #[wasm_bindgen(js_name = moveToFamily)]
        pub fn move_to_family(
            &self,
            target: &WasmAuthoringFamilyLayout,
            aligned_edge_x: f64,
            aligned_edge_y: f64,
            mask_x: f64,
            mask_y: f64,
        ) -> Result<WasmAuthoringFamilyTranslation, JsValue> {
            self.ensure_complete()?;
            target.ensure_complete()?;
            if !Rc::ptr_eq(&self.semantics, &target.semantics) {
                return Err(JsValue::from_str(
                    "family placement source and target belong to different authoring stores",
                ));
            }
            let edge = semantic_xy_f64(aligned_edge_x, aligned_edge_y).map_err(js_error)?;
            let mask = semantic_xy_f64(mask_x, mask_y).map_err(js_error)?;
            let source_x = self.critical_x(edge.x, edge.y)?;
            let source_y = self.critical_y(edge.x, edge.y)?;
            let target_x = target.critical_x(edge.x, edge.y)?;
            let target_y = target.critical_y(edge.x, edge.y)?;
            self.translation(
                (target_x - source_x) * mask.x,
                (target_y - source_y) * mask.y,
            )
        }

'''
critical_anchor = '        #[wasm_bindgen(js_name = criticalX)]\n        pub fn critical_x('
rust = replace_once(
    rust,
    critical_anchor,
    placement_methods + critical_anchor,
    label="insert shared family placement methods",
)

translation_impl = r'''
    #[wasm_bindgen]
    impl WasmAuthoringFamilyTranslation {
        #[wasm_bindgen(js_name = applyMobject)]
        pub fn apply_mobject(
            &mut self,
            member: &mut WasmAuthoringMobjectHandle,
        ) -> Result<(), JsValue> {
            let store = member.1.as_ref().ok_or_else(|| {
                JsValue::from_str(
                    "family translation member is not attached to a shared authoring store",
                )
            })?;
            if !Rc::ptr_eq(&self.semantics, store) {
                return Err(JsValue::from_str(
                    "family translation and mobject belong to different authoring stores",
                ));
            }
            let id = member.2.ok_or_else(|| {
                JsValue::from_str("family translation member has no semantic identity")
            })?;
            self.translation.apply(id, &mut member.0).map_err(js_error)
        }

        pub fn finish(&self) -> Result<(), JsValue> {
            self.translation.finish().map_err(js_error)
        }
    }

'''
family_layout_session_anchor = '''    #[wasm_bindgen]\n    impl WasmAuthoringFamilyHandle {\n        #[wasm_bindgen(js_name = layoutSession)]\n'''
rust = replace_once(
    rust,
    family_layout_session_anchor,
    translation_impl + family_layout_session_anchor,
    label="insert WasmAuthoringFamilyTranslation impl",
)

native_test = r'''
    #[test]
    fn family_translation_uses_shared_recursive_leaf_order() {
        let mut store = SemanticStore::new();
        let first = store.insert_authoring_object();
        let second = store.insert_authoring_object();
        let nested = store.insert_family();
        store.add_member(nested, first).unwrap();
        let outer = store.insert_family();
        store.add_member(outer, nested).unwrap();
        store.add_member(outer, second).unwrap();

        let mut first_handle = FrontendMobjectHandle::manim_circle(1.0).unwrap();
        let mut second_handle = FrontendMobjectHandle::manim_square(2.0).unwrap();
        let first_before = first_handle.center();
        let second_before = second_handle.center();

        let mut translation = FrontendFamilyTranslation::begin(&store, outer, 2.5, -1.25).unwrap();
        translation.apply(first, &mut first_handle).unwrap();
        translation.apply(second, &mut second_handle).unwrap();
        translation.finish().unwrap();

        assert_eq!(first_handle.center(), (first_before.0 + 2.5, first_before.1 - 1.25));
        assert_eq!(second_handle.center(), (second_before.0 + 2.5, second_before.1 - 1.25));

        let mut reordered = FrontendFamilyTranslation::begin(&store, outer, 1.0, 0.0).unwrap();
        let error = reordered.apply(second, &mut second_handle).unwrap_err();
        assert!(error.contains("mismatch at index 0"));
        assert!(reordered.finish().unwrap_err().contains("incomplete"));
    }

'''
rust = replace_once(
    rust,
    '    #[test]\n    fn family_target_editor_builds_target_from_shared_source_order() {\n',
    native_test + '    #[test]\n    fn family_target_editor_builds_target_from_shared_source_order() {\n',
    label="insert native family translation test",
)

rust_path.write_text(rust)


# Python adapter: shared family session owns delta + ordered leaf mutation.
py_path = Path("web/python/_manim_semantic_handles.py")
py = py_path.read_text()
py = replace_once(
    py,
    '_ORIGINAL_GROUP_REMOVE = _compat.Group.remove\n_GROUP_COPY_DELEGATE = None\n',
    '_ORIGINAL_GROUP_REMOVE = _compat.Group.remove\n_ORIGINAL_GROUP_SHIFT = _compat.Group.shift\n_ORIGINAL_GROUP_MOVE_TO = _compat.Group.move_to\n_GROUP_COPY_DELEGATE = None\n',
    label="capture original Group placement",
)

family_helpers = r'''
def _shared_family_layout_session(value: object, *, mutation: bool = False):
    if not isinstance(value, _compat.Group):
        return None
    family_handle = getattr(value, "_semantic_family_handle", None)
    if family_handle is None or not hasattr(family_handle, "layoutSession"):
        return None
    leaves = _compat._leaf_mobjects(value)
    resolver = _mutation_handle_for if mutation else _handle_for
    leaf_handles = [resolver(member) for member in leaves]
    if not all(handle is not None for handle in leaf_handles):
        return None
    session = family_handle.layoutSession()
    for handle in leaf_handles:
        session.includeMobject(handle)
    return session, leaves, leaf_handles


def _apply_family_translation(
    self: _compat.Group,
    translation: object,
    leaves: list[_base.Mobject],
    leaf_handles: list[object],
) -> _compat.Group:
    for member, handle in zip(leaves, leaf_handles):
        translation.applyMobject(handle)
        _sync_bound_transform(member, handle)
    translation.finish()
    return self


def _group_shift(self: _compat.Group, direction: object) -> _compat.Group:
    shared = _shared_family_layout_session(self, mutation=True)
    if shared is None:
        return _ORIGINAL_GROUP_SHIFT(self, direction)
    session, leaves, leaf_handles = shared
    if not hasattr(session, "shiftBy"):
        return _ORIGINAL_GROUP_SHIFT(self, direction)
    offset = _base._as_vec2(direction)
    translation = session.shiftBy(offset.x, offset.y)
    return _apply_family_translation(self, translation, leaves, leaf_handles)


def _group_move_to(
    self: _compat.Group,
    point_or_mobject: object,
    aligned_edge: object = _base.ORIGIN,
    coor_mask: object = (1.0, 1.0, 1.0),
) -> _compat.Group:
    shared = _shared_family_layout_session(self, mutation=True)
    if shared is None:
        return _ORIGINAL_GROUP_MOVE_TO(self, point_or_mobject, aligned_edge, coor_mask)
    session, leaves, leaf_handles = shared
    edge = _base._as_vec2(aligned_edge)
    mask = _alignment_mask2(coor_mask)

    translation = None
    if isinstance(point_or_mobject, _compat.Group):
        target_shared = _shared_family_layout_session(point_or_mobject)
        if target_shared is not None and hasattr(session, "moveToFamily"):
            target_session = target_shared[0]
            translation = session.moveToFamily(
                target_session, edge.x, edge.y, mask.x, mask.y
            )
    elif _alignment_is_mobject(point_or_mobject):
        target_handle = _handle_for(point_or_mobject)
        if target_handle is not None and hasattr(session, "moveToMobject"):
            translation = session.moveToMobject(
                target_handle, edge.x, edge.y, mask.x, mask.y
            )
    elif hasattr(session, "moveToPoint"):
        point = _base._as_vec2(point_or_mobject)
        translation = session.moveToPoint(
            point.x, point.y, edge.x, edge.y, mask.x, mask.y
        )

    if translation is None:
        return _ORIGINAL_GROUP_MOVE_TO(self, point_or_mobject, aligned_edge, coor_mask)
    return _apply_family_translation(self, translation, leaves, leaf_handles)


'''
py = replace_once(
    py,
    'def _compat_bounds_for(value: object) -> tuple[_base.Vec2, _base.Vec2] | None:\n',
    family_helpers + 'def _compat_bounds_for(value: object) -> tuple[_base.Vec2, _base.Vec2] | None:\n',
    label="insert shared family translation adapters",
)

py = replace_once(
    py,
    '        _compat.Group.remove = _group_remove\n        _compat.Group.copy = _group_copy\n',
    '        _compat.Group.remove = _group_remove\n        _compat.Group.shift = _group_shift\n        _compat.Group.move_to = _group_move_to\n        _compat.Group.copy = _group_copy\n',
    label="install shared family translation adapters",
)
py_path.write_text(py)


# Package contract pins the new browser API.
package_path = Path("scripts/check-web-package.mjs")
package = package_path.read_text()
package = replace_once(
    package,
    '  "export class WasmAuthoringFamilyLayout",\n  "export class WasmAuthoringFamilyTargetEditor",\n',
    '  "export class WasmAuthoringFamilyLayout",\n  "export class WasmAuthoringFamilyTranslation",\n  "export class WasmAuthoringFamilyTargetEditor",\n',
    label="pin JS family translation class",
)
package = replace_once(
    package,
    '  "layoutSession(",\n  "targetEditor(",\n',
    '  "layoutSession(",\n  "shiftBy(",\n  "moveToPoint(",\n  "moveToMobject(",\n  "moveToFamily(",\n  "applyMobject(",\n  "targetEditor(",\n',
    label="pin JS family translation methods",
)
package = replace_once(
    package,
    '  "export class WasmAuthoringFamilyLayout",\n  "export class WasmAuthoringFamilyTargetEditor",\n',
    '  "export class WasmAuthoringFamilyLayout",\n  "export class WasmAuthoringFamilyTranslation",\n  "export class WasmAuthoringFamilyTargetEditor",\n',
    label="pin TS family translation class",
)
package = replace_once(
    package,
    '  "layoutSession(): WasmAuthoringFamilyLayout",\n  "targetEditor(): WasmAuthoringMobjectHandle",\n',
    '  "layoutSession(): WasmAuthoringFamilyLayout",\n  "shiftBy(delta_x: number, delta_y: number): WasmAuthoringFamilyTranslation",\n  "moveToPoint(",\n  "moveToMobject(",\n  "moveToFamily(",\n  "applyMobject(member: WasmAuthoringMobjectHandle): void",\n  "targetEditor(): WasmAuthoringMobjectHandle",\n',
    label="pin TS family translation methods",
)
package_path.write_text(package)


# Ownership inventory: shift/move_to are now shared; remaining relative placement stays debt.
ownership_path = Path("compat/semantic-ownership-v1.json")
ownership = ownership_path.read_text()
old_group = '''    {\n      "id": "group-placement",\n      "surface": "Group/VGroup move_to/next_to/align_to/arrange family-bound placement",\n      "classification": "python-semantic-duplicate",\n      "owner": {"language": "python", "path": "web/python/_manim_semantic_handles.py", "symbol": "_manim_move_to/_manim_next_to/_manim_align_to/_manim_arrange"},\n      "shared_owner": {"language": "rust", "path": "crates/noon-web/src/authoring_mobject.rs", "symbol": "WasmAuthoringFamilyHandle::layout_session/WasmAuthoringFamilyLayout"},\n      "reason": "Aggregate deterministic family bounds are shared-Rust-owned, but Python still computes group placement deltas and dispatches the resulting mutation across member wrappers.",\n      "replacement": "Move family placement deltas and member mutation application behind WasmAuthoringFamilyHandle.",\n      "migration_issue": "#61"\n    },\n'''
new_group = '''    {\n      "id": "group-translation",\n      "surface": "Deterministic Group/VGroup shift/move_to/center/set_x/set_y translation",\n      "classification": "shared-rust",\n      "owner": {"language": "rust", "path": "crates/noon-web/src/authoring_mobject.rs", "symbol": "FrontendFamilyTranslation/WasmAuthoringFamilyLayout::shift_by/move_to_*/WasmAuthoringFamilyTranslation::apply_mobject"},\n      "adapters": [{"language": "python", "path": "web/python/_manim_semantic_handles.py", "symbol": "_group_shift/_group_move_to/_apply_family_translation"}],\n      "reason": "Rust computes the family translation delta from authoritative shared bounds, validates recursive leaf order, and mutates each shared leaf handle; Python only coerces host arguments and synchronizes the legacy scene transport mirror."\n    },\n    {\n      "id": "group-placement",\n      "surface": "Group/VGroup next_to/align_to/arrange relative family placement",\n      "classification": "python-semantic-duplicate",\n      "owner": {"language": "python", "path": "web/python/_manim_semantic_handles.py", "symbol": "_manim_next_to/_manim_align_to/_manim_arrange"},\n      "shared_owner": {"language": "rust", "path": "crates/noon-web/src/authoring_mobject.rs", "symbol": "WasmAuthoringFamilyLayout/FrontendFamilyTranslation"},\n      "reason": "Family translation and aggregate bounds are shared-Rust-owned, but Python still computes relative next_to/align_to deltas and direct-submobject arrange sequencing.",\n      "replacement": "Move relative family placement intent and arrange sequencing behind the shared family handle.",\n      "migration_issue": "#61"\n    },\n'''
ownership = replace_once(
    ownership,
    old_group,
    new_group,
    label="ratchet group translation ownership",
)
ownership_path.write_text(ownership)
json.loads(ownership)


# Focused host regression proving Python does not derive the family delta or mutate wrappers itself.
test_path = Path("web/python/test_manim_shared_family_translation.py")
test_path.write_text(r'''import json
import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimSharedFamilyTranslationTests(unittest.TestCase):
    def test_group_translation_uses_shared_family_session(self) -> None:
        python_dir = Path(__file__).resolve().parent
        env = os.environ.copy()
        env["PYTHONDONTWRITEBYTECODE"] = "1"
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
                    self.shift_calls = []

                def snapshotJson(self):
                    return json.dumps(self.snapshot, separators=(\",\", \":\"))

                def replaceSnapshotJson(self, snapshot_json):
                    self.snapshot = json.loads(snapshot_json)

                def cloneHandle(self):
                    return FakeObjectHandle(self.store, self.snapshotJson())

                def targetEditor(self):
                    return self.cloneHandle()

                def shift(self, x, y):
                    self.shift_calls.append((float(x), float(y)))
                    translation = self.snapshot[\"transform\"][\"translation\"]
                    translation[\"x\"] += float(x)
                    translation[\"y\"] += float(y)

                def setFillOpacity(self, opacity):
                    fill = self.snapshot[\"style\"][\"fill\"]
                    if fill is not None:
                        fill[\"alpha\"] = float(opacity)

                def setStrokeOpacity(self, opacity):
                    stroke = self.snapshot[\"style\"][\"stroke\"]
                    if stroke is not None:
                        stroke[\"alpha\"] = float(opacity)


            class FakeTranslation:
                def __init__(self, store, members, dx, dy):
                    self.store = store
                    self.expected = [member.identity for member in members]
                    self.next_index = 0
                    self.dx = float(dx)
                    self.dy = float(dy)

                def applyMobject(self, member):
                    assert member.identity == self.expected[self.next_index]
                    self.store.applied.append(member.identity)
                    member.shift(self.dx, self.dy)
                    self.next_index += 1

                def finish(self):
                    assert self.next_index == len(self.expected)
                    self.store.finishes += 1


            class FakeLayoutSession:
                def __init__(self, store):
                    self.store = store
                    self.members = []
                    store.layout_sessions += 1

                def includeMobject(self, member):
                    self.members.append(member)

                def shiftBy(self, dx, dy):
                    self.store.shift_by.append((float(dx), float(dy)))
                    return FakeTranslation(self.store, self.members, dx, dy)

                def moveToPoint(self, x, y, edge_x, edge_y, mask_x, mask_y):
                    self.store.move_to_point.append(
                        (float(x), float(y), float(edge_x), float(edge_y), float(mask_x), float(mask_y))
                    )
                    # The fake family has center (1, 1) for aligned_edge == ORIGIN.
                    return FakeTranslation(
                        self.store,
                        self.members,
                        (float(x) - 1.0) * float(mask_x),
                        (float(y) - 1.0) * float(mask_y),
                    )

                def criticalX(self, direction_x, direction_y):
                    raise AssertionError("Python must not derive shared family move_to delta")

                def criticalY(self, direction_x, direction_y):
                    raise AssertionError("Python must not derive shared family move_to delta")


            class FakeFamilyHandle:
                def __init__(self, store):
                    self.store = store
                    self.identity = store.allocate()
                    self.members = []

                def layoutSession(self):
                    return FakeLayoutSession(self.store)

                @property
                def memberCount(self):
                    return len(self.members)

                def addMobject(self, member):
                    if member.identity in self.members:
                        return False
                    self.members.append(member.identity)
                    return True

                def addFamily(self, member):
                    if member.identity in self.members:
                        return False
                    self.members.append(member.identity)
                    return True

                def removeMobject(self, member):
                    if member.identity not in self.members:
                        return False
                    self.members.remove(member.identity)
                    return True

                def removeFamily(self, member):
                    return self.removeMobject(member)


            class FakeStore:
                def __init__(self):
                    self.next_identity = 0
                    self.layout_sessions = 0
                    self.shift_by = []
                    self.move_to_point = []
                    self.applied = []
                    self.finishes = 0

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

            from noon import Circle, RIGHT, Square, VGroup

            first = Circle(radius=0.2)
            second = Square(side_length=0.4)
            family = VGroup(first, second)
            ids = [first._semantic_handle.identity, second._semantic_handle.identity]

            family.shift(RIGHT)
            assert store.shift_by == [(1.0, 0.0)]
            assert store.applied == ids
            assert first._semantic_handle.shift_calls == [(1.0, 0.0)]
            assert second._semantic_handle.shift_calls == [(1.0, 0.0)]

            before = len(store.applied)
            family.move_to((5.0, 4.0, 0.0))
            assert store.move_to_point == [(5.0, 4.0, 0.0, 0.0, 1.0, 1.0)]
            assert store.applied[before:] == ids
            assert first._semantic_handle.shift_calls[-1] == (4.0, 3.0)
            assert second._semantic_handle.shift_calls[-1] == (4.0, 3.0)
            assert store.finishes == 2
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
