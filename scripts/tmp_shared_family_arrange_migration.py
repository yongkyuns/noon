from pathlib import Path


def replace_once(text: str, old: str, new: str, *, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise RuntimeError(f"{label}: expected one anchor, found {count}")
    return text.replace(old, new, 1)


rust_path = Path("crates/noon-web/src/authoring_mobject.rs")
rust = rust_path.read_text()
if "pub struct FrontendFamilyArrangePlan" not in rust:
    arrange_plan = r'''
/// Shared Manim family arrangement over authoritative direct-member identity.
///
/// The semantic store snapshots direct membership/order and recursively resolves the
/// leaf identities each direct member owns. Frontends only feed live shared bounds
/// for those members in the validated order; all sequencing, buffer math, optional
/// recentering, and resulting per-member translations are computed here.
#[derive(Clone, Debug)]
pub struct FrontendFamilyArrangePlan {
    members: Vec<FrontendFamilyArrangeMember>,
    next_member: usize,
}

#[derive(Clone, Debug)]
struct FrontendFamilyArrangeMember {
    id: SemanticNodeId,
    leaves: Vec<SemanticNodeId>,
    bounds: Option<Bounds2D64>,
}

impl FrontendFamilyArrangePlan {
    pub fn begin(store: &SemanticStore, source: SemanticNodeId) -> Result<Self, String> {
        let direct_members = {
            let source_node = store
                .node(source)
                .ok_or_else(|| format!("unknown family semantic node {source:?}"))?;
            if !matches!(source_node.kind(), SemanticNodeKind::Family) {
                return Err(format!("semantic node {source:?} is not a family"));
            }
            source_node.members().to_vec()
        };

        let mut members = Vec::with_capacity(direct_members.len());
        for id in direct_members {
            let node = store
                .node(id)
                .ok_or_else(|| format!("unknown family arrange member {id:?}"))?;
            let leaves = match node.kind() {
                SemanticNodeKind::AuthoringObject => vec![id],
                SemanticNodeKind::Family => semantic_family_leaf_ids(store, id)?,
                SemanticNodeKind::Object(_) => {
                    return Err(format!(
                        "family arrange member {id:?} is not an authoring object"
                    ));
                }
            };
            members.push(FrontendFamilyArrangeMember {
                id,
                leaves,
                bounds: None,
            });
        }
        Ok(Self {
            members,
            next_member: 0,
        })
    }

    pub fn accept_member_bounds(
        &mut self,
        member: SemanticNodeId,
        bounds: Option<Bounds2D64>,
    ) -> Result<(), String> {
        let expected = self
            .members
            .get(self.next_member)
            .ok_or_else(|| "family arrange received too many direct members".to_owned())?;
        if expected.id != member {
            return Err(format!(
                "family arrange member mismatch at index {}: expected {:?}, got {member:?}",
                self.next_member, expected.id
            ));
        }
        self.members[self.next_member].bounds = bounds;
        self.next_member += 1;
        Ok(())
    }

    pub fn ensure_complete(&self) -> Result<(), String> {
        if self.next_member != self.members.len() {
            return Err(format!(
                "family arrange is incomplete: accepted {} of {} direct members",
                self.next_member,
                self.members.len()
            ));
        }
        Ok(())
    }

    pub fn member_count(&self) -> usize {
        self.members.len()
    }

    pub fn finish(
        &self,
        direction_x: f64,
        direction_y: f64,
        buff: f64,
        center: bool,
    ) -> Result<Vec<FrontendFamilyTranslation>, String> {
        self.ensure_complete()?;
        let bounds = self
            .members
            .iter()
            .map(|member| member.bounds)
            .collect::<Vec<_>>();
        let deltas = manim_family_arrange_deltas(
            &bounds,
            direction_x,
            direction_y,
            buff,
            center,
        )?;
        self.members
            .iter()
            .zip(deltas)
            .map(|(member, delta)| {
                FrontendFamilyTranslation::from_members(
                    member.leaves.clone(),
                    delta.0,
                    delta.1,
                )
            })
            .collect()
    }
}

'''
    rust = replace_once(
        rust,
        "fn semantic_family_leaf_ids(\n",
        arrange_plan + "fn semantic_family_leaf_ids(\n",
        label="insert family arrange plan",
    )

    arrange_math = r'''
fn manim_family_arrange_deltas(
    member_bounds: &[Option<Bounds2D64>],
    direction_x: f64,
    direction_y: f64,
    buff: f64,
    center: bool,
) -> Result<Vec<(f64, f64)>, String> {
    let direction = semantic_xy_f64(direction_x, direction_y)?;
    let buff = render_f64("buffer", buff)?;
    if member_bounds.is_empty() {
        return Ok(Vec::new());
    }

    let critical = |bounds: Option<Bounds2D64>, x: f64, y: f64| -> (f64, f64) {
        let Some(bounds) = bounds else {
            return (0.0, 0.0);
        };
        let center_x = (bounds.min_x + bounds.max_x) * 0.5;
        let center_y = (bounds.min_y + bounds.max_y) * 0.5;
        (
            if x < 0.0 {
                bounds.min_x
            } else if x > 0.0 {
                bounds.max_x
            } else {
                center_x
            },
            if y < 0.0 {
                bounds.min_y
            } else if y > 0.0 {
                bounds.max_y
            } else {
                center_y
            },
        )
    };

    let mut deltas = vec![(0.0, 0.0); member_bounds.len()];
    for index in 1..member_bounds.len() {
        let source = critical(member_bounds[index], -direction.x, -direction.y);
        let previous = critical(
            member_bounds[index - 1],
            direction.x,
            direction.y,
        );
        deltas[index] = (
            previous.0 + deltas[index - 1].0 - source.0 + direction.x * buff,
            previous.1 + deltas[index - 1].1 - source.1 + direction.y * buff,
        );
    }

    if center {
        let mut arranged_bounds: Option<Bounds2D64> = None;
        for (bounds, delta) in member_bounds.iter().zip(&deltas) {
            let Some(bounds) = bounds else {
                continue;
            };
            let shifted = Bounds2D64 {
                min_x: bounds.min_x + delta.0,
                min_y: bounds.min_y + delta.1,
                max_x: bounds.max_x + delta.0,
                max_y: bounds.max_y + delta.1,
            };
            if let Some(total) = &mut arranged_bounds {
                total.include(shifted.min_x, shifted.min_y);
                total.include(shifted.max_x, shifted.max_y);
            } else {
                arranged_bounds = Some(shifted);
            }
        }
        if let Some(bounds) = arranged_bounds {
            let center_x = (bounds.min_x + bounds.max_x) * 0.5;
            let center_y = (bounds.min_y + bounds.max_y) * 0.5;
            for delta in &mut deltas {
                delta.0 -= center_x;
                delta.1 -= center_y;
            }
        }
    }

    Ok(deltas)
}

'''
    rust = replace_once(
        rust,
        '#[cfg(any(target_arch = "wasm32", test))]\nfn manim_family_next_to_delta(\n',
        arrange_math + '#[cfg(any(target_arch = "wasm32", test))]\nfn manim_family_next_to_delta(\n',
        label="insert family arrange math",
    )

    rust = replace_once(
        rust,
        "        manim_family_align_to_delta, manim_family_next_to_delta, semantic_family_leaf_ids,\n        semantic_xy_f64, Bounds2D64, FrontendFamilyTargetEditor, FrontendFamilyTranslation,\n        FrontendMobjectHandle, ManimNextToArgs, SemanticNodeId, SemanticStore,\n",
        "        manim_family_align_to_delta, manim_family_next_to_delta, render_f64,\n        semantic_family_leaf_ids, semantic_xy_f64, Bounds2D64, FrontendFamilyArrangePlan,\n        FrontendFamilyTargetEditor, FrontendFamilyTranslation, FrontendMobjectHandle,\n        ManimNextToArgs, SemanticNodeId, SemanticStore,\n",
        label="extend wasm imports",
    )

    rust = replace_once(
        rust,
        "    pub struct WasmAuthoringFamilyLayout {\n        semantics: SharedSemanticStore,\n        expected_leaves: Vec<SemanticNodeId>,\n",
        "    pub struct WasmAuthoringFamilyLayout {\n        semantics: SharedSemanticStore,\n        family_id: SemanticNodeId,\n        expected_leaves: Vec<SemanticNodeId>,\n",
        label="store family id on layout",
    )

    arrange_wasm_struct = r'''
    #[wasm_bindgen]
    pub struct WasmAuthoringFamilyArrange {
        semantics: SharedSemanticStore,
        plan: FrontendFamilyArrangePlan,
        direction: (f64, f64),
        buff: f64,
        center: bool,
        translations: Option<Vec<Option<FrontendFamilyTranslation>>>,
        next_translation: usize,
    }

    impl WasmAuthoringFamilyArrange {
        fn prepare(&mut self) -> Result<(), JsValue> {
            if self.translations.is_none() {
                let translations = self
                    .plan
                    .finish(
                        self.direction.0,
                        self.direction.1,
                        self.buff,
                        self.center,
                    )
                    .map_err(js_error)?;
                self.translations = Some(translations.into_iter().map(Some).collect());
            }
            Ok(())
        }

        fn mobject_member_id(
            &self,
            member: &WasmAuthoringMobjectHandle,
        ) -> Result<SemanticNodeId, JsValue> {
            let store = member.1.as_ref().ok_or_else(|| {
                JsValue::from_str("family arrange member is not attached to a shared authoring store")
            })?;
            if !Rc::ptr_eq(&self.semantics, store) {
                return Err(JsValue::from_str(
                    "family arrange and mobject belong to different authoring stores",
                ));
            }
            member
                .2
                .ok_or_else(|| JsValue::from_str("family arrange mobject has no semantic identity"))
        }
    }

    #[wasm_bindgen]
    impl WasmAuthoringFamilyArrange {
        #[wasm_bindgen(js_name = includeMobject)]
        pub fn include_mobject(
            &mut self,
            member: &WasmAuthoringMobjectHandle,
        ) -> Result<(), JsValue> {
            let id = self.mobject_member_id(member)?;
            self.plan
                .accept_member_bounds(id, member.0.layout_bounds())
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = includeFamily)]
        pub fn include_family(
            &mut self,
            layout: &WasmAuthoringFamilyLayout,
        ) -> Result<(), JsValue> {
            layout.ensure_complete()?;
            if !Rc::ptr_eq(&self.semantics, &layout.semantics) {
                return Err(JsValue::from_str(
                    "family arrange and nested family belong to different authoring stores",
                ));
            }
            self.plan
                .accept_member_bounds(layout.family_id, layout.bounds)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = nextTranslation)]
        pub fn next_translation(&mut self) -> Result<WasmAuthoringFamilyTranslation, JsValue> {
            self.prepare()?;
            let translations = self.translations.as_mut().expect("prepared translations");
            let slot = translations
                .get_mut(self.next_translation)
                .ok_or_else(|| JsValue::from_str("family arrange has no remaining translations"))?;
            let translation = slot
                .take()
                .ok_or_else(|| JsValue::from_str("family arrange translation was already consumed"))?;
            self.next_translation += 1;
            Ok(WasmAuthoringFamilyTranslation {
                semantics: Rc::clone(&self.semantics),
                translation,
            })
        }

        pub fn finish(&self) -> Result<(), JsValue> {
            self.plan.ensure_complete().map_err(js_error)?;
            if self.next_translation != self.plan.member_count() {
                return Err(JsValue::from_str(&format!(
                    "family arrange is incomplete: emitted {} of {} translations",
                    self.next_translation,
                    self.plan.member_count()
                )));
            }
            Ok(())
        }
    }

'''
    rust = replace_once(
        rust,
        "    #[wasm_bindgen]\n    pub struct WasmAuthoringFamilyTranslation {\n        semantics: SharedSemanticStore,\n        translation: FrontendFamilyTranslation,\n    }\n\n",
        "    #[wasm_bindgen]\n    pub struct WasmAuthoringFamilyTranslation {\n        semantics: SharedSemanticStore,\n        translation: FrontendFamilyTranslation,\n    }\n\n" + arrange_wasm_struct,
        label="insert wasm family arrange",
    )

    rust = replace_once(
        rust,
        "            Ok(WasmAuthoringFamilyLayout {\n                semantics: Rc::clone(&self.semantics),\n                expected_leaves,\n",
        "            Ok(WasmAuthoringFamilyLayout {\n                semantics: Rc::clone(&self.semantics),\n                family_id: self.id,\n                expected_leaves,\n",
        label="initialize layout family id",
    )

    arrange_factory = r'''
        #[wasm_bindgen(js_name = arrangeSession)]
        pub fn arrange_session(
            &self,
            direction_x: f64,
            direction_y: f64,
            buff: f64,
            center: bool,
        ) -> Result<WasmAuthoringFamilyArrange, JsValue> {
            let direction = semantic_xy_f64(direction_x, direction_y).map_err(js_error)?;
            let buff = render_f64("buffer", buff).map_err(js_error)?;
            let plan = FrontendFamilyArrangePlan::begin(&self.semantics.borrow(), self.id)
                .map_err(js_error)?;
            Ok(WasmAuthoringFamilyArrange {
                semantics: Rc::clone(&self.semantics),
                plan,
                direction: (direction.x, direction.y),
                buff,
                center,
                translations: None,
                next_translation: 0,
            })
        }

'''
    rust = replace_once(
        rust,
        "        #[wasm_bindgen(js_name = targetEditor)]\n        pub fn target_editor(&self) -> Result<WasmAuthoringFamilyTargetEditor, JsValue> {\n",
        arrange_factory + "        #[wasm_bindgen(js_name = targetEditor)]\n        pub fn target_editor(&self) -> Result<WasmAuthoringFamilyTargetEditor, JsValue> {\n",
        label="add arrange session factory",
    )

    test_code = r'''
    #[test]
    fn family_arrange_preserves_direct_order_spacing_and_recentering() {
        let bounds = [
            Some(Bounds2D64 {
                min_x: -1.0,
                min_y: -0.5,
                max_x: 1.0,
                max_y: 0.5,
            }),
            Some(Bounds2D64 {
                min_x: -0.5,
                min_y: -0.25,
                max_x: 0.5,
                max_y: 0.25,
            }),
        ];
        let deltas = manim_family_arrange_deltas(&bounds, 2.0, 0.0, 0.25, true)
            .expect("arrange deltas");
        assert_eq!(deltas, vec![(-0.75, 0.0), (1.25, 0.0)]);

        let mut store = SemanticStore::new();
        let first = store.insert_authoring_object();
        let second = store.insert_authoring_object();
        let nested = store.insert_family();
        store.add_member(nested, second).unwrap();
        let outer = store.insert_family();
        store.add_member(outer, first).unwrap();
        store.add_member(outer, nested).unwrap();

        let mut rejected = FrontendFamilyArrangePlan::begin(&store, outer).unwrap();
        assert!(rejected.accept_member_bounds(nested, bounds[1]).is_err());

        let mut plan = FrontendFamilyArrangePlan::begin(&store, outer).unwrap();
        plan.accept_member_bounds(first, bounds[0]).unwrap();
        plan.accept_member_bounds(nested, bounds[1]).unwrap();
        let translations = plan.finish(2.0, 0.0, 0.25, true).unwrap();
        assert_eq!(translations.len(), 2);
        assert_eq!(translations[0].source_members, vec![first]);
        assert_eq!(translations[0].delta, (-0.75, 0.0));
        assert_eq!(translations[1].source_members, vec![second]);
        assert_eq!(translations[1].delta, (1.25, 0.0));
    }

'''
    rust = replace_once(
        rust,
        "    #[test]\n    fn family_relative_placement_preserves_manim_direction_and_axis_semantics() {\n",
        test_code + "    #[test]\n    fn family_relative_placement_preserves_manim_direction_and_axis_semantics() {\n",
        label="add family arrange rust tests",
    )

    rust_path.write_text(rust)


python_path = Path("web/python/_manim_semantic_handles.py")
python = python_path.read_text()
if "def _group_arrange(" not in python:
    python = replace_once(
        python,
        "_ORIGINAL_GROUP_ALIGN_TO = _compat.Group.align_to\n",
        "_ORIGINAL_GROUP_ALIGN_TO = _compat.Group.align_to\n_ORIGINAL_GROUP_ARRANGE = _compat.Group.arrange\n",
        label="capture original group arrange",
    )

    group_arrange = r'''

def _group_arrange(
    self: _compat.Group,
    direction: object = _base.RIGHT,
    buff: float = _base.DEFAULT_MOBJECT_TO_MOBJECT_BUFFER,
    center: bool = True,
    **kwargs: Any,
) -> _compat.Group:
    # Forwarded placement kwargs can select additional alignment semantics; retain
    # the pinned compatibility path until shared member-selection support lands.
    if kwargs:
        return _ORIGINAL_GROUP_ARRANGE(
            self,
            direction=direction,
            buff=buff,
            center=center,
            **kwargs,
        )
    if not self.submobjects:
        return self

    family_handle = getattr(self, "_semantic_family_handle", None)
    if family_handle is None or not hasattr(family_handle, "arrangeSession"):
        return _ORIGINAL_GROUP_ARRANGE(self, direction=direction, buff=buff, center=center)

    axis = _base._as_vec2(_base.RIGHT if direction is None else direction)
    arrangement = family_handle.arrangeSession(axis.x, axis.y, float(buff), bool(center))
    prepared: list[tuple[object, list[_base.Mobject], list[object]]] = []

    for member in self.submobjects:
        if isinstance(member, _compat.Group):
            shared = _shared_family_layout_session(member, mutation=True)
            if shared is None or not hasattr(arrangement, "includeFamily"):
                return _ORIGINAL_GROUP_ARRANGE(
                    self, direction=direction, buff=buff, center=center
                )
            arrangement.includeFamily(shared[0])
            prepared.append((member, shared[1], shared[2]))
        elif isinstance(member, _base.Mobject):
            handle = _mutation_handle_for(member)
            if handle is None or not hasattr(arrangement, "includeMobject"):
                return _ORIGINAL_GROUP_ARRANGE(
                    self, direction=direction, buff=buff, center=center
                )
            arrangement.includeMobject(handle)
            prepared.append((member, [member], [handle]))
        else:
            return _ORIGINAL_GROUP_ARRANGE(self, direction=direction, buff=buff, center=center)

    for member, leaves, leaf_handles in prepared:
        translation = arrangement.nextTranslation()
        if isinstance(member, _compat.Group):
            _apply_family_translation(member, translation, leaves, leaf_handles)
        else:
            handle = leaf_handles[0]
            translation.applyMobject(handle)
            _sync_bound_transform(member, handle)
            translation.finish()
    arrangement.finish()
    return self

'''
    python = replace_once(
        python,
        "\ndef _compat_bounds_for(value: object) -> tuple[_base.Vec2, _base.Vec2] | None:\n",
        group_arrange + "\ndef _compat_bounds_for(value: object) -> tuple[_base.Vec2, _base.Vec2] | None:\n",
        label="insert group arrange adapter",
    )
    python = replace_once(
        python,
        "        _compat.Group.align_to = _group_align_to\n",
        "        _compat.Group.align_to = _group_align_to\n        _compat.Group.arrange = _group_arrange\n",
        label="install group arrange adapter",
    )
    python_path.write_text(python)


ownership_path = Path("compat/semantic-ownership-v1.json")
ownership = ownership_path.read_text()
old_layout = '''    {
      "id": "layout.arrange",
      "surface": "Group.arrange",
      "classification": "python-adapter-only",
      "owner": {"language": "python", "path": "web/python/_manim_semantic_handles.py", "symbol": "_manim_arrange"},
      "reason": "The operation is Manim collection syntax: it iterates Python children and delegates each placement to shared next_to semantics.",
      "replacement": "Keep only until shared family handles can express collection iteration directly."
    },'''
new_layout = '''    {
      "id": "layout.arrange",
      "surface": "Deterministic Group/VGroup arrange(direction, buff, center) without forwarded placement kwargs",
      "classification": "shared-rust",
      "owner": {"language": "rust", "path": "crates/noon-web/src/authoring_mobject.rs", "symbol": "FrontendFamilyArrangePlan/WasmAuthoringFamilyHandle::arrange_session/WasmAuthoringFamilyArrange"},
      "adapters": [{"language": "python", "path": "web/python/_manim_semantic_handles.py", "symbol": "_group_arrange"}],
      "reason": "Rust snapshots authoritative direct-member order, validates each member's shared bounds, computes sequential Manim next_to spacing and optional recentering, and returns ordered per-member shared translations. Python only mirrors wrapper identity and applies those translations."
    },'''
ownership = replace_once(ownership, old_layout, new_layout, label="ratchet layout arrange ownership")
old_group = '''      "id": "group-placement",
      "surface": "Group/VGroup arrange and next_to submobject_to_align/index_of_submobject_to_align selection",
      "classification": "python-semantic-duplicate",
      "owner": {"language": "python", "path": "web/python/_manim_semantic_handles.py", "symbol": "_manim_arrange/_group_next_to fallback"},
      "shared_owner": {"language": "rust", "path": "crates/noon-web/src/authoring_mobject.rs", "symbol": "WasmAuthoringFamilyLayout/FrontendFamilyTranslation"},
      "reason": "Default family relative placement is shared-Rust-owned, but direct member selection and arrange sequencing still traverse Python wrapper submobjects.",
      "replacement": "Expose shared family-member selection and ordered arrange sequencing behind WasmAuthoringFamilyHandle.",
      "migration_issue": "#61"
    },'''
new_group = '''      "id": "group-placement",
      "surface": "Group/VGroup next_to submobject_to_align/index_of_submobject_to_align selection and arrange forwarded placement kwargs",
      "classification": "python-semantic-duplicate",
      "owner": {"language": "python", "path": "web/python/_manim_semantic_handles.py", "symbol": "_manim_arrange/_group_next_to fallback"},
      "shared_owner": {"language": "rust", "path": "crates/noon-web/src/authoring_mobject.rs", "symbol": "WasmAuthoringFamilyLayout/FrontendFamilyArrangePlan/FrontendFamilyTranslation"},
      "reason": "Default family relative placement and ordinary arrange sequencing are shared-Rust-owned, but explicit wrapper/member aligner selection and forwarded arrange placement kwargs still resolve in Python.",
      "replacement": "Expose shared family-member selection handles and route forwarded arrange alignment policy through the shared family placement API.",
      "migration_issue": "#61"
    },'''
ownership = replace_once(ownership, old_group, new_group, label="narrow remaining group placement debt")
ownership_path.write_text(ownership)


check_path = Path("scripts/check-web-package.mjs")
check = check_path.read_text()
if '"export class WasmAuthoringFamilyArrange"' not in check:
    check = replace_once(
        check,
        '  "export class WasmAuthoringFamilyLayout",\n  "export class WasmAuthoringFamilyTranslation",\n',
        '  "export class WasmAuthoringFamilyLayout",\n  "export class WasmAuthoringFamilyArrange",\n  "export class WasmAuthoringFamilyTranslation",\n',
        label="pin arrange javascript class",
    )
    check = replace_once(
        check,
        '  "layoutSession(",\n  "shiftBy(",\n',
        '  "layoutSession(",\n  "arrangeSession(",\n  "nextTranslation(",\n  "shiftBy(",\n',
        label="pin arrange javascript methods",
    )
    check = replace_once(
        check,
        '  "export class WasmAuthoringFamilyLayout",\n  "export class WasmAuthoringFamilyTranslation",\n',
        '  "export class WasmAuthoringFamilyLayout",\n  "export class WasmAuthoringFamilyArrange",\n  "export class WasmAuthoringFamilyTranslation",\n',
        label="pin arrange type class",
    )
    check = replace_once(
        check,
        '  "layoutSession(): WasmAuthoringFamilyLayout",\n  "shiftBy(delta_x: number, delta_y: number): WasmAuthoringFamilyTranslation",\n',
        '  "layoutSession(): WasmAuthoringFamilyLayout",\n  "arrangeSession(direction_x: number, direction_y: number, buff: number, center: boolean): WasmAuthoringFamilyArrange",\n  "nextTranslation(): WasmAuthoringFamilyTranslation",\n  "shiftBy(delta_x: number, delta_y: number): WasmAuthoringFamilyTranslation",\n',
        label="pin arrange type methods",
    )
    check_path.write_text(check)


test_path = Path("web/python/test_manim_shared_family_arrange.py")
if not test_path.exists():
    test_path.write_text(r'''import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimSharedFamilyArrangeTests(unittest.TestCase):
    def test_group_arrange_dispatches_order_and_spacing_to_shared_family_plan(self) -> None:
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
                    self.identity = store.allocate(self)
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


            class FakeLayoutSession:
                def __init__(self, family):
                    self.family = family
                    self.members = []

                def includeMobject(self, member):
                    self.members.append(member)


            class FakeTranslation:
                def __init__(self, store, expected, delta):
                    self.store = store
                    self.expected = list(expected)
                    self.delta = delta
                    self.next_index = 0

                def applyMobject(self, member):
                    assert member.identity == self.expected[self.next_index]
                    member.shift(*self.delta)
                    self.store.applied.append(member.identity)
                    self.next_index += 1

                def finish(self):
                    assert self.next_index == len(self.expected)


            class FakeArrange:
                def __init__(self, family, direction_x, direction_y, buff, center):
                    self.family = family
                    self.store = family.store
                    self.expected = list(family.members)
                    self.next_include = 0
                    self.next_translation = 0
                    self.store.arrange_calls.append(
                        (family.identity, float(direction_x), float(direction_y), float(buff), bool(center))
                    )

                def _accept(self, identity, kind):
                    assert identity == self.expected[self.next_include]
                    self.store.arrange_includes.append((kind, identity))
                    self.next_include += 1

                def includeMobject(self, member):
                    self._accept(member.identity, \"mobject\")

                def includeFamily(self, layout):
                    self._accept(layout.family.identity, \"family\")

                def nextTranslation(self):
                    assert self.next_include == len(self.expected)
                    identity = self.expected[self.next_translation]
                    expected_leaves = [member.identity for member in self.store.leaves(identity)]
                    deltas = [(-1.0, 0.0), (2.0, 0.0), (4.0, 0.0)]
                    translation = FakeTranslation(
                        self.store,
                        expected_leaves,
                        deltas[self.next_translation],
                    )
                    self.next_translation += 1
                    return translation

                def finish(self):
                    assert self.next_translation == len(self.expected)
                    self.store.arrange_finishes += 1


            class FakeFamilyHandle:
                def __init__(self, store):
                    self.store = store
                    self.identity = store.allocate(self)
                    self.members = []

                def layoutSession(self):
                    return FakeLayoutSession(self)

                def arrangeSession(self, direction_x, direction_y, buff, center):
                    return FakeArrange(self, direction_x, direction_y, buff, center)

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
                    self.entities = {}
                    self.arrange_calls = []
                    self.arrange_includes = []
                    self.arrange_finishes = 0
                    self.applied = []

                def allocate(self, entity):
                    value = self.next_identity
                    self.next_identity += 1
                    self.entities[value] = entity
                    return value

                def leaves(self, identity):
                    entity = self.entities[identity]
                    if isinstance(entity, FakeObjectHandle):
                        return [entity]
                    result = []
                    for child in entity.members:
                        result.extend(self.leaves(child))
                    return result

                def createMobject(self, snapshot_json):
                    return FakeObjectHandle(self, snapshot_json)

                def createFamily(self):
                    return FakeFamilyHandle(self)


            store = FakeStore()
            handles._create_handle = store.createMobject
            handles._create_family_handle = store.createFamily
            handles.install()

            def forbidden_fallback(*args, **kwargs):
                raise AssertionError(\"Python arrange fallback must not run on shared path\")

            handles._ORIGINAL_GROUP_ARRANGE = forbidden_fallback

            from noon import Circle, RIGHT, Square, VGroup

            first = Circle(radius=0.2)
            second = Square(side_length=0.4)
            nested = VGroup(second)
            family = VGroup(first, nested)

            family.arrange(direction=2.0 * RIGHT, buff=0.25, center=True)

            assert store.arrange_calls == [
                (family._semantic_family_handle.identity, 2.0, 0.0, 0.25, True)
            ]
            assert store.arrange_includes == [
                (\"mobject\", first._semantic_handle.identity),
                (\"family\", nested._semantic_family_handle.identity),
            ]
            assert store.applied == [
                first._semantic_handle.identity,
                second._semantic_handle.identity,
            ]
            assert first._semantic_handle.shift_calls[-1] == (-1.0, 0.0)
            assert second._semantic_handle.shift_calls[-1] == (2.0, 0.0)
            assert store.arrange_finishes == 1
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
