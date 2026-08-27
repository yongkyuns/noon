from __future__ import annotations

import json
from pathlib import Path


def replace_once(text: str, old: str, new: str, label: str) -> str:
    if text.count(old) != 1:
        raise RuntimeError(f"{label}: expected one anchor, found {text.count(old)}")
    return text.replace(old, new, 1)


rust_path = Path("crates/noon-web/src/authoring_mobject.rs")
rust = rust_path.read_text()

leaf_helper = r'''
fn semantic_family_leaf_ids(
    store: &SemanticStore,
    family: SemanticNodeId,
) -> Result<Vec<SemanticNodeId>, String> {
    fn collect(
        store: &SemanticStore,
        node_id: SemanticNodeId,
        leaves: &mut Vec<SemanticNodeId>,
    ) -> Result<(), String> {
        let node = store
            .node(node_id)
            .ok_or_else(|| format!("unknown semantic family member {node_id:?}"))?;
        match node.kind() {
            SemanticNodeKind::AuthoringObject => {
                leaves.push(node_id);
                Ok(())
            }
            SemanticNodeKind::Family => {
                for member in node.members() {
                    collect(store, *member, leaves)?;
                }
                Ok(())
            }
            SemanticNodeKind::Object(_) => Err(format!(
                "family layout member {node_id:?} is not an authoring object"
            )),
        }
    }

    let root = store
        .node(family)
        .ok_or_else(|| format!("unknown family semantic node {family:?}"))?;
    if !matches!(root.kind(), SemanticNodeKind::Family) {
        return Err(format!("semantic node {family:?} is not a family"));
    }

    let mut leaves = Vec::new();
    collect(store, family, &mut leaves)?;
    Ok(leaves)
}

'''
rust = replace_once(rust, "fn finite_f32(name: &str, value: f64) -> Result<f32, String> {", leaf_helper + "fn finite_f32(name: &str, value: f64) -> Result<f32, String> {", "family leaf helper")

old_import = '''    use super::{
        FrontendFamilyTargetEditor, FrontendMobjectHandle, ManimNextToArgs, SemanticNodeId,
        SemanticStore,
    };'''
new_import = '''    use super::{
        semantic_family_leaf_ids, Bounds2D64, FrontendFamilyTargetEditor, FrontendMobjectHandle,
        ManimNextToArgs, SemanticNodeId, SemanticStore,
    };'''
rust = replace_once(rust, old_import, new_import, "wasm imports")

family_struct = '''    #[wasm_bindgen]
    pub struct WasmAuthoringFamilyHandle {
        semantics: SharedSemanticStore,
        id: SemanticNodeId,
    }
'''
layout_struct = family_struct + r'''
    #[wasm_bindgen]
    pub struct WasmAuthoringFamilyLayout {
        semantics: SharedSemanticStore,
        expected_leaves: Vec<SemanticNodeId>,
        next_leaf: usize,
        bounds: Option<Bounds2D64>,
    }

    impl WasmAuthoringFamilyLayout {
        fn ensure_complete(&self) -> Result<(), JsValue> {
            if self.next_leaf != self.expected_leaves.len() {
                return Err(JsValue::from_str(&format!(
                    "family layout is incomplete: accepted {} of {} leaves",
                    self.next_leaf,
                    self.expected_leaves.len()
                )));
            }
            Ok(())
        }

        fn center(&self) -> Result<(f64, f64), JsValue> {
            self.ensure_complete()?;
            Ok(self.bounds.as_ref().map_or((0.0, 0.0), |bounds| {
                (
                    (bounds.min_x + bounds.max_x) * 0.5,
                    (bounds.min_y + bounds.max_y) * 0.5,
                )
            }))
        }

        fn include_bounds(&mut self, bounds: Bounds2D64) {
            if let Some(total) = &mut self.bounds {
                total.include(bounds.min_x, bounds.min_y);
                total.include(bounds.max_x, bounds.max_y);
            } else {
                self.bounds = Some(bounds);
            }
        }
    }

    #[wasm_bindgen]
    impl WasmAuthoringFamilyLayout {
        #[wasm_bindgen(js_name = includeMobject)]
        pub fn include_mobject(
            &mut self,
            member: &WasmAuthoringMobjectHandle,
        ) -> Result<(), JsValue> {
            let store = member.1.as_ref().ok_or_else(|| {
                JsValue::from_str("family layout member is not attached to a shared authoring store")
            })?;
            if !Rc::ptr_eq(&self.semantics, store) {
                return Err(JsValue::from_str(
                    "family layout and mobject belong to different authoring stores",
                ));
            }
            let id = member
                .2
                .ok_or_else(|| JsValue::from_str("family layout member has no semantic identity"))?;
            let expected = self
                .expected_leaves
                .get(self.next_leaf)
                .copied()
                .ok_or_else(|| JsValue::from_str("family layout received too many leaves"))?;
            if id != expected {
                return Err(JsValue::from_str(&format!(
                    "family layout leaf mismatch at index {}: expected {expected:?}, got {id:?}",
                    self.next_leaf
                )));
            }
            if let Some(bounds) = member.0.layout_bounds() {
                self.include_bounds(bounds);
            }
            self.next_leaf += 1;
            Ok(())
        }

        #[wasm_bindgen(getter, js_name = centerX)]
        pub fn center_x(&self) -> Result<f64, JsValue> {
            self.center().map(|center| center.0)
        }

        #[wasm_bindgen(getter, js_name = centerY)]
        pub fn center_y(&self) -> Result<f64, JsValue> {
            self.center().map(|center| center.1)
        }

        #[wasm_bindgen(getter)]
        pub fn width(&self) -> Result<f64, JsValue> {
            self.ensure_complete()?;
            Ok(self.bounds.as_ref().map_or(0.0, Bounds2D64::width))
        }

        #[wasm_bindgen(getter)]
        pub fn height(&self) -> Result<f64, JsValue> {
            self.ensure_complete()?;
            Ok(self.bounds.as_ref().map_or(0.0, Bounds2D64::height))
        }

        #[wasm_bindgen(js_name = criticalX)]
        pub fn critical_x(&self, direction_x: f64, _direction_y: f64) -> Result<f64, JsValue> {
            self.ensure_complete()?;
            let center = self.center()?.0;
            Ok(self.bounds.as_ref().map_or(center, |bounds| {
                if direction_x < 0.0 {
                    bounds.min_x
                } else if direction_x > 0.0 {
                    bounds.max_x
                } else {
                    center
                }
            }))
        }

        #[wasm_bindgen(js_name = criticalY)]
        pub fn critical_y(&self, _direction_x: f64, direction_y: f64) -> Result<f64, JsValue> {
            self.ensure_complete()?;
            let center = self.center()?.1;
            Ok(self.bounds.as_ref().map_or(center, |bounds| {
                if direction_y < 0.0 {
                    bounds.min_y
                } else if direction_y > 0.0 {
                    bounds.max_y
                } else {
                    center
                }
            }))
        }
    }
'''
rust = replace_once(rust, family_struct, layout_struct, "family layout struct")

layout_method_anchor = '''    #[wasm_bindgen]
    impl WasmAuthoringFamilyHandle {
        #[wasm_bindgen(js_name = targetEditor)]'''
layout_method = '''    #[wasm_bindgen]
    impl WasmAuthoringFamilyHandle {
        #[wasm_bindgen(js_name = layoutSession)]
        pub fn layout_session(&self) -> Result<WasmAuthoringFamilyLayout, JsValue> {
            let expected_leaves = semantic_family_leaf_ids(&self.semantics.borrow(), self.id)
                .map_err(js_error)?;
            Ok(WasmAuthoringFamilyLayout {
                semantics: Rc::clone(&self.semantics),
                expected_leaves,
                next_leaf: 0,
                bounds: None,
            })
        }

        #[wasm_bindgen(js_name = targetEditor)]'''
rust = replace_once(rust, layout_method_anchor, layout_method, "family layout method")

test_anchor = '''    #[test]
    fn family_target_editor_builds_target_from_shared_source_order() {'''
test_code = r'''    #[test]
    fn family_layout_leaf_order_comes_from_shared_semantic_graph() {
        let mut store = SemanticStore::new();
        let first = store.insert_authoring_object();
        let second = store.insert_authoring_object();
        let nested = store.insert_family();
        store.add_member(nested, first).unwrap();
        let outer = store.insert_family();
        store.add_member(outer, nested).unwrap();
        store.add_member(outer, second).unwrap();

        assert_eq!(semantic_family_leaf_ids(&store, outer).unwrap(), vec![first, second]);

        let alias = store.insert_family();
        store.add_member(alias, first).unwrap();
        let aliased_outer = store.insert_family();
        store.add_member(aliased_outer, nested).unwrap();
        store.add_member(aliased_outer, alias).unwrap();
        assert_eq!(
            semantic_family_leaf_ids(&store, aliased_outer).unwrap(),
            vec![first, first]
        );
    }

    #[test]
    fn family_target_editor_builds_target_from_shared_source_order() {'''
rust = replace_once(rust, test_anchor, test_code, "family layout Rust test")
rust_path.write_text(rust)


py_path = Path("web/python/_manim_semantic_handles.py")
py = py_path.read_text()
start = py.index("def _compat_bounds_for(value: object)")
end = py.index("\ndef _family_member_handle", start)
new_bounds = r'''def _compat_bounds_for(value: object) -> tuple[_base.Vec2, _base.Vec2] | None:
    leaves = _compat._leaf_mobjects(value)

    # Group/VGroup wrapper traversal remains host-language metadata, but the shared
    # family graph independently derives the expected recursive leaf sequence and
    # rejects any wrapper divergence. Rust owns the actual aggregate bounds math.
    if isinstance(value, _compat.Group):
        family_handle = getattr(value, "_semantic_family_handle", None)
        leaf_handles = [_handle_for(member) for member in leaves]
        if (
            family_handle is not None
            and hasattr(family_handle, "layoutSession")
            and all(handle is not None for handle in leaf_handles)
        ):
            session = family_handle.layoutSession()
            for handle in leaf_handles:
                session.includeMobject(handle)
            return (
                _base.Vec2(
                    float(session.criticalX(-1.0, 0.0)),
                    float(session.criticalY(0.0, -1.0)),
                ),
                _base.Vec2(
                    float(session.criticalX(1.0, 0.0)),
                    float(session.criticalY(0.0, 1.0)),
                ),
            )

    # Host-dynamic/stale bound leaves intentionally retain the evaluated-snapshot
    # fallback until runtime family queries exist. Deterministic shared handles do
    # not execute this aggregation path.
    present: list[tuple[_base.Vec2, _base.Vec2]] = []
    for member in leaves:
        bounds = (
            _layout_bounds(member)
            if _handle_for(member) is not None
            else _base._bounds(member._current_raw())
        )
        if bounds is not None:
            present.append(bounds)
    if not present:
        return None
    return (
        _base.Vec2(
            min(bound[0].x for bound in present),
            min(bound[0].y for bound in present),
        ),
        _base.Vec2(
            max(bound[1].x for bound in present),
            max(bound[1].y for bound in present),
        ),
    )
'''
py = py[:start] + new_bounds + py[end:]
py_path.write_text(py)


test_path = Path("web/python/test_manim_shared_family_identity.py")
test = test_path.read_text()
test = replace_once(
    test,
    '''            class FakeFamilyHandle:\n                def __init__(self, store):\n                    self.store = store\n                    self.identity = store.allocate()\n                    self.members = []\n''',
    '''            class FakeLayoutSession:\n                def __init__(self, store):\n                    self.store = store\n                    self.members = []\n                    store.layout_sessions += 1\n\n                def includeMobject(self, member):\n                    self.members.append(member.identity)\n\n                def _complete(self):\n                    assert len(self.members) == 2\n\n                def criticalX(self, direction_x, direction_y):\n                    del direction_y\n                    self._complete()\n                    return -3.0 if direction_x < 0 else (5.0 if direction_x > 0 else 1.0)\n\n                def criticalY(self, direction_x, direction_y):\n                    del direction_x\n                    self._complete()\n                    return -2.0 if direction_y < 0 else (4.0 if direction_y > 0 else 1.0)\n\n\n            class FakeFamilyHandle:\n                def __init__(self, store):\n                    self.store = store\n                    self.identity = store.allocate()\n                    self.members = []\n\n                def layoutSession(self):\n                    return FakeLayoutSession(self.store)\n''',
    "fake family layout session",
)
test = replace_once(
    test,
    '''            class FakeStore:\n                def __init__(self):\n                    self.next_identity = 0\n''',
    '''            class FakeStore:\n                def __init__(self):\n                    self.next_identity = 0\n                    self.layout_sessions = 0\n''',
    "fake store layout counter",
)
test = replace_once(
    test,
    '''            assert outer._semantic_family_handle.memberCount == 2\n            assert nested._semantic_family_handle.memberCount == 1\n\n            clone = outer.copy()\n''',
    '''            assert outer._semantic_family_handle.memberCount == 2\n            assert nested._semantic_family_handle.memberCount == 1\n\n            center = outer.get_center()\n            assert center.x == 1.0 and center.y == 1.0\n            assert outer.width == 8.0\n            assert outer.height == 6.0\n            assert store.layout_sessions == 3\n\n            clone = outer.copy()\n''',
    "family layout assertions",
)
test_path.write_text(test)


ownership_path = Path("compat/semantic-ownership-v1.json")
ownership = json.loads(ownership_path.read_text())
ops = ownership["operations"]
index = next(i for i, item in enumerate(ops) if item["id"] == "group-placement")
ops.insert(index, {
    "id": "group-layout-bounds",
    "surface": "Deterministic Group/VGroup aggregate bounds, center, width/height, and critical-point queries",
    "classification": "shared-rust",
    "owner": {
        "language": "rust",
        "path": "crates/noon-web/src/authoring_mobject.rs",
        "symbol": "semantic_family_leaf_ids/WasmAuthoringFamilyHandle::layout_session/WasmAuthoringFamilyLayout",
    },
    "adapters": [{
        "language": "python",
        "path": "web/python/_manim_semantic_handles.py",
        "symbol": "_compat_bounds_for",
    }],
    "reason": "Rust derives recursive leaf order from SemanticStore, validates the Python wrapper mirror against that order, and computes the aggregate bounds; Python only supplies the corresponding live shared leaf handles.",
})
placement = next(item for item in ops if item["id"] == "group-placement")
placement["shared_owner"] = {
    "language": "rust",
    "path": "crates/noon-web/src/authoring_mobject.rs",
    "symbol": "WasmAuthoringFamilyHandle::layout_session/WasmAuthoringFamilyLayout",
}
placement["reason"] = "Aggregate deterministic family bounds are shared-Rust-owned, but Python still computes group placement deltas and dispatches the resulting mutation across member wrappers."
placement["replacement"] = "Move family placement deltas and member mutation application behind WasmAuthoringFamilyHandle."
ownership_path.write_text(json.dumps(ownership, indent=2) + "\n")


package_path = Path("scripts/check-web-package.mjs")
package = package_path.read_text()
package = replace_once(
    package,
    '  "export class WasmAuthoringFamilyHandle",\n  "export class WasmAuthoringFamilyTargetEditor",',
    '  "export class WasmAuthoringFamilyHandle",\n  "export class WasmAuthoringFamilyLayout",\n  "export class WasmAuthoringFamilyTargetEditor",\n  "layoutSession(",',
    "javascript family layout contract",
)
package = replace_once(
    package,
    '  "export class WasmAuthoringFamilyHandle",\n  "export class WasmAuthoringFamilyTargetEditor",\n  "targetEditor(): WasmAuthoringMobjectHandle",',
    '  "export class WasmAuthoringFamilyHandle",\n  "export class WasmAuthoringFamilyLayout",\n  "export class WasmAuthoringFamilyTargetEditor",\n  "layoutSession(): WasmAuthoringFamilyLayout",\n  "targetEditor(): WasmAuthoringMobjectHandle",',
    "type family layout contract",
)
package_path.write_text(package)


smoke_path = Path("scripts/manim-compat-smoke.mjs")
smoke = smoke_path.read_text()
smoke = replace_once(
    smoke,
    '''        assert int(pair._semantic_family_handle.memberCount) == 2\n        alias = VGroup(left)\n''',
    '''        assert int(pair._semantic_family_handle.memberCount) == 2\n        layout = pair._semantic_family_handle.layoutSession()\n        layout.includeMobject(left._semantic_handle)\n        layout.includeMobject(right._semantic_handle)\n        assert abs(float(layout.width) - pair.width) < 1e-12\n        assert abs(float(layout.height) - pair.height) < 1e-12\n        alias = VGroup(left)\n''',
    "browser family layout smoke",
)
smoke_path.write_text(smoke)
