from pathlib import Path
import json


def replace_once(text: str, old: str, new: str, *, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise RuntimeError(f"{label}: expected one anchor, found {count}")
    return text.replace(old, new, 1)


rust_path = Path("crates/noon-web/src/authoring_mobject.rs")
rust = rust_path.read_text()

helpers = r'''
fn manim_family_next_to_delta(
    source: (f64, f64),
    target: (f64, f64),
    direction: (f64, f64),
    buff: f64,
    mask: (f64, f64),
) -> Result<(f64, f64), String> {
    let direction = semantic_xy_f64(direction.0, direction.1)?;
    let mask = semantic_xy_f64(mask.0, mask.1)?;
    let buff = render_f64("buffer", buff)?;
    Ok((
        (target.0 - source.0 + direction.x * buff) * mask.x,
        (target.1 - source.1 + direction.y * buff) * mask.y,
    ))
}

fn manim_family_align_to_delta(
    source: (f64, f64),
    target: (f64, f64),
    axis: (f64, f64),
) -> Result<(f64, f64), String> {
    let axis = semantic_xy_f64(axis.0, axis.1)?;
    Ok((
        if axis.x != 0.0 { target.0 - source.0 } else { 0.0 },
        if axis.y != 0.0 { target.1 - source.1 } else { 0.0 },
    ))
}

'''
rust = replace_once(
    rust,
    '#[cfg(target_arch = "wasm32")]\nmod wasm {\n',
    helpers + '#[cfg(target_arch = "wasm32")]\nmod wasm {\n',
    label="insert relative-placement helpers",
)

# Make the shared helper functions visible to the WASM adapter.
rust = replace_once(
    rust,
    '        authoring_style_from_legacy, finite_f32, legacy_solid_color, render_f64,\n',
    '        authoring_style_from_legacy, finite_f32, legacy_solid_color, manim_family_align_to_delta,\n        manim_family_next_to_delta, render_f64,\n',
    label="import relative-placement helpers into wasm module",
)

methods = r'''
        #[wasm_bindgen(js_name = nextToPoint)]
        #[allow(clippy::too_many_arguments)]
        pub fn next_to_point(
            &self,
            point_x: f64,
            point_y: f64,
            direction_x: f64,
            direction_y: f64,
            buff: f64,
            aligned_edge_x: f64,
            aligned_edge_y: f64,
            mask_x: f64,
            mask_y: f64,
        ) -> Result<WasmAuthoringFamilyTranslation, JsValue> {
            let point = semantic_xy_f64(point_x, point_y).map_err(js_error)?;
            let direction = semantic_xy_f64(direction_x, direction_y).map_err(js_error)?;
            let edge = semantic_xy_f64(aligned_edge_x, aligned_edge_y).map_err(js_error)?;
            let source = (
                self.critical_x(edge.x - direction.x, edge.y - direction.y)?,
                self.critical_y(edge.x - direction.x, edge.y - direction.y)?,
            );
            let delta = manim_family_next_to_delta(
                source,
                (point.x, point.y),
                (direction.x, direction.y),
                buff,
                (mask_x, mask_y),
            )
            .map_err(js_error)?;
            self.translation(delta.0, delta.1)
        }

        #[wasm_bindgen(js_name = nextToMobject)]
        #[allow(clippy::too_many_arguments)]
        pub fn next_to_mobject(
            &self,
            target: &WasmAuthoringMobjectHandle,
            direction_x: f64,
            direction_y: f64,
            buff: f64,
            aligned_edge_x: f64,
            aligned_edge_y: f64,
            mask_x: f64,
            mask_y: f64,
        ) -> Result<WasmAuthoringFamilyTranslation, JsValue> {
            self.ensure_complete()?;
            self.validate_target_mobject(target)?;
            let direction = semantic_xy_f64(direction_x, direction_y).map_err(js_error)?;
            let edge = semantic_xy_f64(aligned_edge_x, aligned_edge_y).map_err(js_error)?;
            let source = (
                self.critical_x(edge.x - direction.x, edge.y - direction.y)?,
                self.critical_y(edge.x - direction.x, edge.y - direction.y)?,
            );
            let target_point = target
                .0
                .critical_point(edge.x + direction.x, edge.y + direction.y);
            let delta = manim_family_next_to_delta(
                source,
                target_point,
                (direction.x, direction.y),
                buff,
                (mask_x, mask_y),
            )
            .map_err(js_error)?;
            self.translation(delta.0, delta.1)
        }

        #[wasm_bindgen(js_name = nextToFamily)]
        #[allow(clippy::too_many_arguments)]
        pub fn next_to_family(
            &self,
            target: &WasmAuthoringFamilyLayout,
            direction_x: f64,
            direction_y: f64,
            buff: f64,
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
            let direction = semantic_xy_f64(direction_x, direction_y).map_err(js_error)?;
            let edge = semantic_xy_f64(aligned_edge_x, aligned_edge_y).map_err(js_error)?;
            let source = (
                self.critical_x(edge.x - direction.x, edge.y - direction.y)?,
                self.critical_y(edge.x - direction.x, edge.y - direction.y)?,
            );
            let target_point = (
                target.critical_x(edge.x + direction.x, edge.y + direction.y)?,
                target.critical_y(edge.x + direction.x, edge.y + direction.y)?,
            );
            let delta = manim_family_next_to_delta(
                source,
                target_point,
                (direction.x, direction.y),
                buff,
                (mask_x, mask_y),
            )
            .map_err(js_error)?;
            self.translation(delta.0, delta.1)
        }

        #[wasm_bindgen(js_name = alignToPoint)]
        pub fn align_to_point(
            &self,
            point_x: f64,
            point_y: f64,
            axis_x: f64,
            axis_y: f64,
        ) -> Result<WasmAuthoringFamilyTranslation, JsValue> {
            let point = semantic_xy_f64(point_x, point_y).map_err(js_error)?;
            let axis = semantic_xy_f64(axis_x, axis_y).map_err(js_error)?;
            let source = (
                self.critical_x(axis.x, axis.y)?,
                self.critical_y(axis.x, axis.y)?,
            );
            let delta = manim_family_align_to_delta(
                source,
                (point.x, point.y),
                (axis.x, axis.y),
            )
            .map_err(js_error)?;
            self.translation(delta.0, delta.1)
        }

        #[wasm_bindgen(js_name = alignToMobject)]
        pub fn align_to_mobject(
            &self,
            target: &WasmAuthoringMobjectHandle,
            axis_x: f64,
            axis_y: f64,
        ) -> Result<WasmAuthoringFamilyTranslation, JsValue> {
            self.ensure_complete()?;
            self.validate_target_mobject(target)?;
            let axis = semantic_xy_f64(axis_x, axis_y).map_err(js_error)?;
            let source = (
                self.critical_x(axis.x, axis.y)?,
                self.critical_y(axis.x, axis.y)?,
            );
            let target_point = target.0.critical_point(axis.x, axis.y);
            let delta = manim_family_align_to_delta(source, target_point, (axis.x, axis.y))
                .map_err(js_error)?;
            self.translation(delta.0, delta.1)
        }

        #[wasm_bindgen(js_name = alignToFamily)]
        pub fn align_to_family(
            &self,
            target: &WasmAuthoringFamilyLayout,
            axis_x: f64,
            axis_y: f64,
        ) -> Result<WasmAuthoringFamilyTranslation, JsValue> {
            self.ensure_complete()?;
            target.ensure_complete()?;
            if !Rc::ptr_eq(&self.semantics, &target.semantics) {
                return Err(JsValue::from_str(
                    "family placement source and target belong to different authoring stores",
                ));
            }
            let axis = semantic_xy_f64(axis_x, axis_y).map_err(js_error)?;
            let source = (
                self.critical_x(axis.x, axis.y)?,
                self.critical_y(axis.x, axis.y)?,
            );
            let target_point = (
                target.critical_x(axis.x, axis.y)?,
                target.critical_y(axis.x, axis.y)?,
            );
            let delta = manim_family_align_to_delta(source, target_point, (axis.x, axis.y))
                .map_err(js_error)?;
            self.translation(delta.0, delta.1)
        }

'''
rust = replace_once(
    rust,
    '        #[wasm_bindgen(js_name = criticalX)]\n        pub fn critical_x(&self, direction_x: f64, _direction_y: f64) -> Result<f64, JsValue> {\n',
    methods + '        #[wasm_bindgen(js_name = criticalX)]\n        pub fn critical_x(&self, direction_x: f64, _direction_y: f64) -> Result<f64, JsValue> {\n',
    label="insert family relative-placement wasm methods",
)

tests = r'''
    #[test]
    fn family_relative_placement_preserves_manim_direction_and_axis_semantics() {
        let next = manim_family_next_to_delta(
            (2.0, 3.0),
            (7.0, 11.0),
            (2.0, -3.0),
            0.5,
            (1.0, 0.25),
        )
        .expect("next_to delta");
        assert_eq!(next, (6.0, 1.625));

        let aligned = manim_family_align_to_delta((2.0, 3.0), (7.0, 11.0), (0.0, -1.0))
            .expect("align_to delta");
        assert_eq!(aligned, (0.0, 8.0));
    }

'''
rust = replace_once(
    rust,
    '    use super::*;\n\n    fn snapshot(geometry: GeometryRef) -> ObjectSnapshot {\n',
    '    use super::*;\n\n' + tests + '    fn snapshot(geometry: GeometryRef) -> ObjectSnapshot {\n',
    label="add family relative-placement native test",
)
rust_path.write_text(rust)


py_path = Path("web/python/_manim_semantic_handles.py")
py = py_path.read_text()
py = replace_once(
    py,
    '_ORIGINAL_GROUP_SHIFT = _compat.Group.shift\n_ORIGINAL_GROUP_MOVE_TO = _compat.Group.move_to\n',
    '_ORIGINAL_GROUP_SHIFT = _compat.Group.shift\n_ORIGINAL_GROUP_MOVE_TO = _compat.Group.move_to\n_ORIGINAL_GROUP_NEXT_TO = _compat.Group.next_to\n_ORIGINAL_GROUP_ALIGN_TO = _compat.Group.align_to\n',
    label="capture family relative-placement fallbacks",
)

py_helpers = r'''
def _group_next_to(
    self: _compat.Group,
    mobject_or_point: object,
    direction: object = _base.RIGHT,
    buff: float = _base.DEFAULT_MOBJECT_TO_MOBJECT_BUFFER,
    aligned_edge: object = _base.ORIGIN,
    submobject_to_align: object | None = None,
    index_of_submobject_to_align: int | None = None,
    coor_mask: object = (1.0, 1.0, 1.0),
) -> _compat.Group:
    # Selecting a specific wrapper/member remains explicit #61 debt until shared
    # family-member handles expose that selection. Do not silently rederive it here.
    if submobject_to_align is not None or index_of_submobject_to_align is not None:
        return _ORIGINAL_GROUP_NEXT_TO(
            self,
            mobject_or_point,
            direction,
            buff,
            aligned_edge,
            submobject_to_align,
            index_of_submobject_to_align,
            coor_mask,
        )

    shared = _shared_family_layout_session(self, mutation=True)
    if shared is None:
        return _ORIGINAL_GROUP_NEXT_TO(
            self,
            mobject_or_point,
            direction,
            buff,
            aligned_edge,
            submobject_to_align,
            index_of_submobject_to_align,
            coor_mask,
        )
    session, leaves, leaf_handles = shared
    vector = _base._as_vec2(direction)
    edge = _base._as_vec2(aligned_edge)
    mask = _alignment_mask2(coor_mask)

    translation = None
    if isinstance(mobject_or_point, _compat.Group):
        target_shared = _shared_family_layout_session(mobject_or_point)
        if target_shared is not None and hasattr(session, "nextToFamily"):
            translation = session.nextToFamily(
                target_shared[0],
                vector.x,
                vector.y,
                float(buff),
                edge.x,
                edge.y,
                mask.x,
                mask.y,
            )
    elif _alignment_is_mobject(mobject_or_point):
        target_handle = _handle_for(mobject_or_point)
        if target_handle is not None and hasattr(session, "nextToMobject"):
            translation = session.nextToMobject(
                target_handle,
                vector.x,
                vector.y,
                float(buff),
                edge.x,
                edge.y,
                mask.x,
                mask.y,
            )
    elif hasattr(session, "nextToPoint"):
        point = _base._as_vec2(mobject_or_point)
        translation = session.nextToPoint(
            point.x,
            point.y,
            vector.x,
            vector.y,
            float(buff),
            edge.x,
            edge.y,
            mask.x,
            mask.y,
        )

    if translation is None:
        return _ORIGINAL_GROUP_NEXT_TO(
            self,
            mobject_or_point,
            direction,
            buff,
            aligned_edge,
            submobject_to_align,
            index_of_submobject_to_align,
            coor_mask,
        )
    return _apply_family_translation(self, translation, leaves, leaf_handles)


def _group_align_to(
    self: _compat.Group,
    mobject_or_point: object,
    direction: object = _base.ORIGIN,
) -> _compat.Group:
    shared = _shared_family_layout_session(self, mutation=True)
    if shared is None:
        return _ORIGINAL_GROUP_ALIGN_TO(self, mobject_or_point, direction)
    session, leaves, leaf_handles = shared
    axis = _base._as_vec2(direction)

    translation = None
    if isinstance(mobject_or_point, _compat.Group):
        target_shared = _shared_family_layout_session(mobject_or_point)
        if target_shared is not None and hasattr(session, "alignToFamily"):
            translation = session.alignToFamily(target_shared[0], axis.x, axis.y)
    elif _alignment_is_mobject(mobject_or_point):
        target_handle = _handle_for(mobject_or_point)
        if target_handle is not None and hasattr(session, "alignToMobject"):
            translation = session.alignToMobject(target_handle, axis.x, axis.y)
    elif hasattr(session, "alignToPoint"):
        point = _base._as_vec2(mobject_or_point)
        translation = session.alignToPoint(point.x, point.y, axis.x, axis.y)

    if translation is None:
        return _ORIGINAL_GROUP_ALIGN_TO(self, mobject_or_point, direction)
    return _apply_family_translation(self, translation, leaves, leaf_handles)


'''
py = replace_once(
    py,
    'def _compat_bounds_for(value: object) -> tuple[_base.Vec2, _base.Vec2] | None:\n',
    py_helpers + 'def _compat_bounds_for(value: object) -> tuple[_base.Vec2, _base.Vec2] | None:\n',
    label="insert family relative-placement adapters",
)
py = replace_once(
    py,
    '        _compat.Group.shift = _group_shift\n        _compat.Group.move_to = _group_move_to\n        _compat.Group.copy = _group_copy\n',
    '        _compat.Group.shift = _group_shift\n        _compat.Group.move_to = _group_move_to\n        _compat.Group.next_to = _group_next_to\n        _compat.Group.align_to = _group_align_to\n        _compat.Group.copy = _group_copy\n',
    label="install family relative-placement adapters",
)
py_path.write_text(py)


package_path = Path("scripts/check-web-package.mjs")
package = package_path.read_text()
package = replace_once(
    package,
    '  "moveToFamily(",\n  "applyMobject(",\n',
    '  "moveToFamily(",\n  "nextToPoint(",\n  "nextToMobject(",\n  "nextToFamily(",\n  "alignToPoint(",\n  "alignToMobject(",\n  "alignToFamily(",\n  "applyMobject(",\n',
    label="pin JS family relative-placement methods",
)
package = replace_once(
    package,
    '  "moveToFamily(",\n  "applyMobject(member: WasmAuthoringMobjectHandle): void",\n',
    '  "moveToFamily(",\n  "nextToPoint(",\n  "nextToMobject(",\n  "nextToFamily(",\n  "alignToPoint(",\n  "alignToMobject(",\n  "alignToFamily(",\n  "applyMobject(member: WasmAuthoringMobjectHandle): void",\n',
    label="pin TS family relative-placement methods",
)
package_path.write_text(package)


ownership_path = Path("compat/semantic-ownership-v1.json")
ownership = ownership_path.read_text()
old = '''    {\n      "id": "group-placement",\n      "surface": "Group/VGroup next_to/align_to/arrange relative family placement",\n      "classification": "python-semantic-duplicate",\n      "owner": {"language": "python", "path": "web/python/_manim_semantic_handles.py", "symbol": "_manim_next_to/_manim_align_to/_manim_arrange"},\n      "shared_owner": {"language": "rust", "path": "crates/noon-web/src/authoring_mobject.rs", "symbol": "WasmAuthoringFamilyLayout/FrontendFamilyTranslation"},\n      "reason": "Family translation and aggregate bounds are shared-Rust-owned, but Python still computes relative next_to/align_to deltas and direct-submobject arrange sequencing.",\n      "replacement": "Move relative family placement intent and arrange sequencing behind the shared family handle.",\n      "migration_issue": "#61"\n    },\n'''
new = '''    {\n      "id": "group-relative-placement",\n      "surface": "Deterministic Group/VGroup next_to/align_to without explicit submobject selection",\n      "classification": "shared-rust",\n      "owner": {"language": "rust", "path": "crates/noon-web/src/authoring_mobject.rs", "symbol": "WasmAuthoringFamilyLayout::next_to_*/align_to_*/FrontendFamilyTranslation"},\n      "adapters": [{"language": "python", "path": "web/python/_manim_semantic_handles.py", "symbol": "_group_next_to/_group_align_to"}],\n      "reason": "Rust resolves family/leaf/point critical points, Manim's unnormalized next_to direction and buffer, align_to axis masking, and the authoritative recursive leaf mutation order. Python only coerces host arguments and selects the target handle kind."\n    },\n    {\n      "id": "group-placement",\n      "surface": "Group/VGroup arrange and next_to submobject_to_align/index_of_submobject_to_align selection",\n      "classification": "python-semantic-duplicate",\n      "owner": {"language": "python", "path": "web/python/_manim_semantic_handles.py", "symbol": "_manim_arrange/_group_next_to fallback"},\n      "shared_owner": {"language": "rust", "path": "crates/noon-web/src/authoring_mobject.rs", "symbol": "WasmAuthoringFamilyLayout/FrontendFamilyTranslation"},\n      "reason": "Default family relative placement is shared-Rust-owned, but direct member selection and arrange sequencing still traverse Python wrapper submobjects.",\n      "replacement": "Expose shared family-member selection and ordered arrange sequencing behind WasmAuthoringFamilyHandle.",\n      "migration_issue": "#61"\n    },\n'''
ownership = replace_once(ownership, old, new, label="ratchet family relative-placement ownership")
ownership_path.write_text(ownership)
json.loads(ownership)


test_path = Path("web/python/test_manim_shared_family_relative_placement.py")
test_path.write_text(r'''import json
import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimSharedFamilyRelativePlacementTests(unittest.TestCase):
    def test_group_next_to_and_align_to_dispatch_to_shared_family_session(self) -> None:
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
                    pass

                def setStrokeOpacity(self, opacity):
                    pass


            class FakeTranslation:
                def __init__(self, store, members, dx, dy):
                    self.store = store
                    self.expected = [member.identity for member in members]
                    self.next_index = 0
                    self.dx = float(dx)
                    self.dy = float(dy)

                def applyMobject(self, member):
                    assert member.identity == self.expected[self.next_index]
                    member.shift(self.dx, self.dy)
                    self.store.applied.append(member.identity)
                    self.next_index += 1

                def finish(self):
                    assert self.next_index == len(self.expected)
                    self.store.finishes += 1


            class FakeLayoutSession:
                def __init__(self, store):
                    self.store = store
                    self.members = []

                def includeMobject(self, member):
                    self.members.append(member)

                def nextToPoint(self, *args):
                    self.store.next_to_point.append(tuple(float(value) for value in args))
                    return FakeTranslation(self.store, self.members, 2.0, -1.0)

                def nextToFamily(self, target, *args):
                    self.store.next_to_family.append(tuple(float(value) for value in args))
                    assert len(target.members) == 1
                    return FakeTranslation(self.store, self.members, 3.0, 0.5)

                def alignToPoint(self, *args):
                    self.store.align_to_point.append(tuple(float(value) for value in args))
                    return FakeTranslation(self.store, self.members, 0.0, 4.0)

                def alignToFamily(self, target, *args):
                    self.store.align_to_family.append(tuple(float(value) for value in args))
                    assert len(target.members) == 1
                    return FakeTranslation(self.store, self.members, -2.0, 0.0)

                def criticalX(self, direction_x, direction_y):
                    raise AssertionError("Python must not derive family relative-placement deltas")

                def criticalY(self, direction_x, direction_y):
                    raise AssertionError("Python must not derive family relative-placement deltas")


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
                    self.next_to_point = []
                    self.next_to_family = []
                    self.align_to_point = []
                    self.align_to_family = []
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

            from noon import Circle, RIGHT, Square, UP, VGroup

            first = Circle(radius=0.2)
            second = Square(side_length=0.4)
            family = VGroup(first, second)
            ids = [first._semantic_handle.identity, second._semantic_handle.identity]

            family.next_to((5.0, 6.0, 0.0), 2.0 * RIGHT, buff=0.25)
            assert store.next_to_point == [(5.0, 6.0, 2.0, 0.0, 0.25, 0.0, 0.0, 1.0, 1.0)]
            assert store.applied == ids
            assert first._semantic_handle.shift_calls[-1] == (2.0, -1.0)
            assert second._semantic_handle.shift_calls[-1] == (2.0, -1.0)

            target_leaf = Circle(radius=0.1)
            target = VGroup(target_leaf)
            before = len(store.applied)
            family.next_to(target, RIGHT, buff=0.5)
            assert store.next_to_family == [(1.0, 0.0, 0.5, 0.0, 0.0, 1.0, 1.0)]
            assert store.applied[before:] == ids

            before = len(store.applied)
            family.align_to((9.0, 8.0, 0.0), UP)
            assert store.align_to_point == [(9.0, 8.0, 0.0, 1.0)]
            assert store.applied[before:] == ids
            assert first._semantic_handle.shift_calls[-1] == (0.0, 4.0)

            before = len(store.applied)
            family.align_to(target, RIGHT)
            assert store.align_to_family == [(1.0, 0.0)]
            assert store.applied[before:] == ids
            assert first._semantic_handle.shift_calls[-1] == (-2.0, 0.0)
            assert store.finishes == 4
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
