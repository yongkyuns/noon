from pathlib import Path


def replace_once(text: str, old: str, new: str, *, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise RuntimeError(f"{label}: expected one anchor, found {count}")
    return text.replace(old, new, 1)


rust_path = Path("crates/noon-web/src/authoring_mobject.rs")
rust = rust_path.read_text()
if "pub struct FrontendFamilyMemberSelection" not in rust:
    native = r'''
/// Authoritative direct-member selection for Manim family placement.
///
/// Python may retain a wrapper list for language-level identity, but the semantic
/// store owns index normalization and the selected direct member identity. This is
/// especially important for negative indices and nested family members.
#[derive(Clone, Debug)]
pub struct FrontendFamilyMemberSelection {
    member: SemanticNodeId,
}

impl FrontendFamilyMemberSelection {
    pub fn begin(
        store: &SemanticStore,
        family: SemanticNodeId,
        index: i32,
    ) -> Result<Self, String> {
        let node = store
            .node(family)
            .ok_or_else(|| format!("unknown family semantic node {family:?}"))?;
        if !matches!(node.kind(), SemanticNodeKind::Family) {
            return Err(format!("semantic node {family:?} is not a family"));
        }
        let len = i64::try_from(node.members().len())
            .map_err(|_| "family member count exceeds supported index range".to_owned())?;
        let raw = i64::from(index);
        let normalized = if raw < 0 { len + raw } else { raw };
        if normalized < 0 || normalized >= len {
            return Err(format!(
                "family member index {index} is out of bounds for {len} members"
            ));
        }
        let member = node.members()[normalized as usize];
        Ok(Self { member })
    }

    pub const fn member_id(&self) -> SemanticNodeId {
        self.member
    }
}

'''
    rust = replace_once(
        rust,
        "/// Shared Manim family arrangement over authoritative direct-member identity.\n",
        native + "/// Shared Manim family arrangement over authoritative direct-member identity.\n",
        label="insert native member selection",
    )

    wasm_structs = r'''
    #[wasm_bindgen]
    pub struct WasmAuthoringLayoutBounds {
        semantics: SharedSemanticStore,
        bounds: Option<Bounds2D64>,
    }

    #[wasm_bindgen]
    pub struct WasmAuthoringFamilyMemberLayout {
        semantics: SharedSemanticStore,
        member_id: SemanticNodeId,
        accepted: bool,
        bounds: Option<Bounds2D64>,
    }

    impl WasmAuthoringFamilyMemberLayout {
        fn ensure_complete(&self) -> Result<(), JsValue> {
            if !self.accepted {
                return Err(JsValue::from_str(
                    "family member layout is incomplete: selected wrapper was not validated",
                ));
            }
            Ok(())
        }

        fn validate_store(&self, other: &SharedSemanticStore) -> Result<(), JsValue> {
            if !Rc::ptr_eq(&self.semantics, other) {
                return Err(JsValue::from_str(
                    "family member layout and selected wrapper belong to different authoring stores",
                ));
            }
            Ok(())
        }
    }

    #[wasm_bindgen]
    impl WasmAuthoringFamilyMemberLayout {
        #[wasm_bindgen(js_name = includeMobject)]
        pub fn include_mobject(
            &mut self,
            member: &WasmAuthoringMobjectHandle,
        ) -> Result<(), JsValue> {
            if self.accepted {
                return Err(JsValue::from_str(
                    "family member layout already accepted its selected wrapper",
                ));
            }
            let store = member.1.as_ref().ok_or_else(|| {
                JsValue::from_str(
                    "selected family member is not attached to a shared authoring store",
                )
            })?;
            self.validate_store(store)?;
            let id = member.2.ok_or_else(|| {
                JsValue::from_str("selected family member has no semantic identity")
            })?;
            if id != self.member_id {
                return Err(JsValue::from_str(&format!(
                    "family member selection mismatch: expected {:?}, got {id:?}",
                    self.member_id
                )));
            }
            self.bounds = member.0.layout_bounds();
            self.accepted = true;
            Ok(())
        }

        #[wasm_bindgen(js_name = includeFamily)]
        pub fn include_family(
            &mut self,
            layout: &WasmAuthoringFamilyLayout,
        ) -> Result<(), JsValue> {
            if self.accepted {
                return Err(JsValue::from_str(
                    "family member layout already accepted its selected wrapper",
                ));
            }
            layout.ensure_complete()?;
            self.validate_store(&layout.semantics)?;
            if layout.family_id != self.member_id {
                return Err(JsValue::from_str(&format!(
                    "family member selection mismatch: expected {:?}, got {:?}",
                    self.member_id, layout.family_id
                )));
            }
            self.bounds = layout.bounds;
            self.accepted = true;
            Ok(())
        }

        #[wasm_bindgen(js_name = boundsHandle)]
        pub fn bounds_handle(&self) -> Result<WasmAuthoringLayoutBounds, JsValue> {
            self.ensure_complete()?;
            Ok(WasmAuthoringLayoutBounds {
                semantics: Rc::clone(&self.semantics),
                bounds: self.bounds,
            })
        }
    }

'''
    rust = replace_once(
        rust,
        "    #[wasm_bindgen]\n    pub struct WasmAuthoringFamilyTranslation {\n",
        wasm_structs + "    #[wasm_bindgen]\n    pub struct WasmAuthoringFamilyTranslation {\n",
        label="insert wasm selection structs",
    )

    layout_helpers = r'''
        fn validate_layout_bounds(
            &self,
            bounds: &WasmAuthoringLayoutBounds,
        ) -> Result<(), JsValue> {
            if !Rc::ptr_eq(&self.semantics, &bounds.semantics) {
                return Err(JsValue::from_str(
                    "family placement source and aligner belong to different authoring stores",
                ));
            }
            Ok(())
        }

        fn critical_from_bounds(
            bounds: Option<Bounds2D64>,
            direction_x: f64,
            direction_y: f64,
        ) -> (f64, f64) {
            let Some(bounds) = bounds else {
                return (0.0, 0.0);
            };
            let center_x = (bounds.min_x + bounds.max_x) * 0.5;
            let center_y = (bounds.min_y + bounds.max_y) * 0.5;
            (
                if direction_x < 0.0 {
                    bounds.min_x
                } else if direction_x > 0.0 {
                    bounds.max_x
                } else {
                    center_x
                },
                if direction_y < 0.0 {
                    bounds.min_y
                } else if direction_y > 0.0 {
                    bounds.max_y
                } else {
                    center_y
                },
            )
        }

'''
    rust = replace_once(
        rust,
        "        fn validate_target_mobject(\n",
        layout_helpers + "        fn validate_target_mobject(\n",
        label="insert layout bounds helpers",
    )

    family_bounds_method = r'''
        #[wasm_bindgen(js_name = boundsHandle)]
        pub fn bounds_handle(&self) -> Result<WasmAuthoringLayoutBounds, JsValue> {
            self.ensure_complete()?;
            Ok(WasmAuthoringLayoutBounds {
                semantics: Rc::clone(&self.semantics),
                bounds: self.bounds,
            })
        }

'''
    rust = replace_once(
        rust,
        "        #[wasm_bindgen(getter, js_name = centerX)]\n",
        family_bounds_method + "        #[wasm_bindgen(getter, js_name = centerX)]\n",
        label="add family bounds handle",
    )

    selected_next_to = r'''
        #[wasm_bindgen(js_name = nextToPointWithAligner)]
        #[allow(clippy::too_many_arguments)]
        pub fn next_to_point_with_aligner(
            &self,
            source_aligner: &WasmAuthoringLayoutBounds,
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
            self.ensure_complete()?;
            self.validate_layout_bounds(source_aligner)?;
            let point = semantic_xy_f64(point_x, point_y).map_err(js_error)?;
            let direction = semantic_xy_f64(direction_x, direction_y).map_err(js_error)?;
            let edge = semantic_xy_f64(aligned_edge_x, aligned_edge_y).map_err(js_error)?;
            let source = Self::critical_from_bounds(
                source_aligner.bounds,
                edge.x - direction.x,
                edge.y - direction.y,
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

        #[wasm_bindgen(js_name = nextToBoundsWithAligner)]
        #[allow(clippy::too_many_arguments)]
        pub fn next_to_bounds_with_aligner(
            &self,
            source_aligner: &WasmAuthoringLayoutBounds,
            target_aligner: &WasmAuthoringLayoutBounds,
            direction_x: f64,
            direction_y: f64,
            buff: f64,
            aligned_edge_x: f64,
            aligned_edge_y: f64,
            mask_x: f64,
            mask_y: f64,
        ) -> Result<WasmAuthoringFamilyTranslation, JsValue> {
            self.ensure_complete()?;
            self.validate_layout_bounds(source_aligner)?;
            self.validate_layout_bounds(target_aligner)?;
            let direction = semantic_xy_f64(direction_x, direction_y).map_err(js_error)?;
            let edge = semantic_xy_f64(aligned_edge_x, aligned_edge_y).map_err(js_error)?;
            let source = Self::critical_from_bounds(
                source_aligner.bounds,
                edge.x - direction.x,
                edge.y - direction.y,
            );
            let target = Self::critical_from_bounds(
                target_aligner.bounds,
                edge.x + direction.x,
                edge.y + direction.y,
            );
            let delta = manim_family_next_to_delta(
                source,
                target,
                (direction.x, direction.y),
                buff,
                (mask_x, mask_y),
            )
            .map_err(js_error)?;
            self.translation(delta.0, delta.1)
        }

'''
    rust = replace_once(
        rust,
        "        #[wasm_bindgen(js_name = alignToPoint)]\n",
        selected_next_to + "        #[wasm_bindgen(js_name = alignToPoint)]\n",
        label="add selected next_to methods",
    )

    member_factory = r'''
        #[wasm_bindgen(js_name = memberLayoutSession)]
        pub fn member_layout_session(
            &self,
            index: i32,
        ) -> Result<WasmAuthoringFamilyMemberLayout, JsValue> {
            let selection = FrontendFamilyMemberSelection::begin(
                &self.semantics.borrow(),
                self.id,
                index,
            )
            .map_err(js_error)?;
            Ok(WasmAuthoringFamilyMemberLayout {
                semantics: Rc::clone(&self.semantics),
                member_id: selection.member_id(),
                accepted: false,
                bounds: None,
            })
        }

'''
    rust = replace_once(
        rust,
        "        #[wasm_bindgen(js_name = arrangeSession)]\n",
        member_factory + "        #[wasm_bindgen(js_name = arrangeSession)]\n",
        label="add member layout factory",
    )

    mobject_bounds = r'''
        #[wasm_bindgen(js_name = layoutBoundsHandle)]
        pub fn layout_bounds_handle(&self) -> Result<WasmAuthoringLayoutBounds, JsValue> {
            let store = self.1.as_ref().ok_or_else(|| {
                JsValue::from_str("mobject is not attached to a shared authoring store")
            })?;
            if self.2.is_none() {
                return Err(JsValue::from_str("mobject has no semantic identity"));
            }
            Ok(WasmAuthoringLayoutBounds {
                semantics: Rc::clone(store),
                bounds: self.0.layout_bounds(),
            })
        }

'''
    rust = replace_once(
        rust,
        "        #[wasm_bindgen(js_name = setTranslation)]\n",
        mobject_bounds + "        #[wasm_bindgen(js_name = setTranslation)]\n",
        label="add mobject bounds handle",
    )

    tests = r'''
    #[test]
    fn family_member_selection_owns_negative_index_normalization() {
        let mut store = SemanticStore::new();
        let first = store.insert_authoring_object();
        let second = store.insert_authoring_object();
        let nested = store.insert_family();
        store.add_member(nested, second).unwrap();
        let outer = store.insert_family();
        store.add_member(outer, first).unwrap();
        store.add_member(outer, nested).unwrap();

        assert_eq!(
            FrontendFamilyMemberSelection::begin(&store, outer, 0)
                .unwrap()
                .member_id(),
            first
        );
        assert_eq!(
            FrontendFamilyMemberSelection::begin(&store, outer, -1)
                .unwrap()
                .member_id(),
            nested
        );
        assert!(FrontendFamilyMemberSelection::begin(&store, outer, 2).is_err());
        assert!(FrontendFamilyMemberSelection::begin(&store, outer, -3).is_err());
    }

'''
    rust = replace_once(
        rust,
        "    #[test]\n    fn family_arrange_preserves_direct_order_spacing_and_recentering() {\n",
        tests + "    #[test]\n    fn family_arrange_preserves_direct_order_spacing_and_recentering() {\n",
        label="add member selection tests",
    )

    rust_path.write_text(rust)


python_path = Path("web/python/_manim_semantic_handles.py")
python = python_path.read_text()
if "def _family_member_bounds_handle(" not in python:
    helpers = r'''

def _layout_bounds_handle(value: object) -> object | None:
    if isinstance(value, _compat.Group):
        shared = _shared_family_layout_session(value)
        if shared is None or not hasattr(shared[0], "boundsHandle"):
            return None
        return shared[0].boundsHandle()
    if isinstance(value, _base.Mobject):
        handle = _handle_for(value)
        if handle is None or not hasattr(handle, "layoutBoundsHandle"):
            return None
        return handle.layoutBoundsHandle()
    return None


def _family_member_bounds_handle(value: object, index: int) -> object | None:
    if not isinstance(value, _compat.Group):
        return None
    family_handle = getattr(value, "_semantic_family_handle", None)
    if family_handle is None or not hasattr(family_handle, "memberLayoutSession"):
        return None
    selection = family_handle.memberLayoutSession(int(index))
    # The Python list is only an identity mirror: Rust normalized the index first and
    # validates the selected direct semantic member before accepting its live bounds.
    member = value.submobjects[index]
    if isinstance(member, _compat.Group):
        shared = _shared_family_layout_session(member)
        if shared is None or not hasattr(selection, "includeFamily"):
            return None
        selection.includeFamily(shared[0])
    elif isinstance(member, _base.Mobject):
        handle = _handle_for(member)
        if handle is None or not hasattr(selection, "includeMobject"):
            return None
        selection.includeMobject(handle)
    else:
        return None
    return selection.boundsHandle()

'''
    python = replace_once(
        python,
        "\ndef _group_next_to(\n",
        helpers + "\ndef _group_next_to(\n",
        label="insert shared member selection helpers",
    )

    old_guard = '''    if (
        submobject_to_align is not None
        or index_of_submobject_to_align is not None
    ):
        return _ORIGINAL_GROUP_NEXT_TO(
            self,
            mobject_or_point,
            direction,
            buff,
            aligned_edge=aligned_edge,
            submobject_to_align=submobject_to_align,
            index_of_submobject_to_align=index_of_submobject_to_align,
            coor_mask=coor_mask,
        )

'''
    python = replace_once(python, old_guard, "", label="remove group next_to selection fallback")

    old_target = '''    if _alignment_is_mobject(mobject_or_point):
        target_kind, target = _family_layout_target(mobject_or_point)
        if target is None:
            return _ORIGINAL_GROUP_NEXT_TO(
                self,
                mobject_or_point,
                direction,
                buff,
                aligned_edge=aligned_edge,
                coor_mask=coor_mask,
            )
        if target_kind == "family" and hasattr(layout, "nextToFamily"):
            translation = layout.nextToFamily(
                target,
                vector.x,
                vector.y,
                float(buff),
                edge.x,
                edge.y,
                mask.x,
                mask.y,
            )
        elif target_kind == "mobject" and hasattr(layout, "nextToMobject"):
            translation = layout.nextToMobject(
                target,
                vector.x,
                vector.y,
                float(buff),
                edge.x,
                edge.y,
                mask.x,
                mask.y,
            )
        else:
            return _ORIGINAL_GROUP_NEXT_TO(
                self,
                mobject_or_point,
                direction,
                buff,
                aligned_edge=aligned_edge,
                coor_mask=coor_mask,
            )
    else:
        if not hasattr(layout, "nextToPoint"):
            return _ORIGINAL_GROUP_NEXT_TO(
                self,
                mobject_or_point,
                direction,
                buff,
                aligned_edge=aligned_edge,
                coor_mask=coor_mask,
            )
        point = _base._as_vec2(mobject_or_point)
        translation = layout.nextToPoint(
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
'''
    new_target = '''    source_aligner_bounds = None
    if submobject_to_align is not None:
        source_aligner_bounds = _layout_bounds_handle(submobject_to_align)
    elif index_of_submobject_to_align is not None:
        source_aligner_bounds = _family_member_bounds_handle(
            self, int(index_of_submobject_to_align)
        )

    if source_aligner_bounds is not None:
        if _alignment_is_mobject(mobject_or_point):
            if index_of_submobject_to_align is not None:
                target_bounds = _family_member_bounds_handle(
                    mobject_or_point, int(index_of_submobject_to_align)
                )
            else:
                target_bounds = _layout_bounds_handle(mobject_or_point)
            if target_bounds is None or not hasattr(layout, "nextToBoundsWithAligner"):
                return _ORIGINAL_GROUP_NEXT_TO(
                    self,
                    mobject_or_point,
                    direction,
                    buff,
                    aligned_edge=aligned_edge,
                    submobject_to_align=submobject_to_align,
                    index_of_submobject_to_align=index_of_submobject_to_align,
                    coor_mask=coor_mask,
                )
            translation = layout.nextToBoundsWithAligner(
                source_aligner_bounds,
                target_bounds,
                vector.x,
                vector.y,
                float(buff),
                edge.x,
                edge.y,
                mask.x,
                mask.y,
            )
        else:
            if not hasattr(layout, "nextToPointWithAligner"):
                return _ORIGINAL_GROUP_NEXT_TO(
                    self,
                    mobject_or_point,
                    direction,
                    buff,
                    aligned_edge=aligned_edge,
                    submobject_to_align=submobject_to_align,
                    index_of_submobject_to_align=index_of_submobject_to_align,
                    coor_mask=coor_mask,
                )
            point = _base._as_vec2(mobject_or_point)
            translation = layout.nextToPointWithAligner(
                source_aligner_bounds,
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
    elif submobject_to_align is not None or index_of_submobject_to_align is not None:
        return _ORIGINAL_GROUP_NEXT_TO(
            self,
            mobject_or_point,
            direction,
            buff,
            aligned_edge=aligned_edge,
            submobject_to_align=submobject_to_align,
            index_of_submobject_to_align=index_of_submobject_to_align,
            coor_mask=coor_mask,
        )
    elif _alignment_is_mobject(mobject_or_point):
        target_kind, target = _family_layout_target(mobject_or_point)
        if target is None:
            return _ORIGINAL_GROUP_NEXT_TO(
                self,
                mobject_or_point,
                direction,
                buff,
                aligned_edge=aligned_edge,
                coor_mask=coor_mask,
            )
        if target_kind == "family" and hasattr(layout, "nextToFamily"):
            translation = layout.nextToFamily(
                target,
                vector.x,
                vector.y,
                float(buff),
                edge.x,
                edge.y,
                mask.x,
                mask.y,
            )
        elif target_kind == "mobject" and hasattr(layout, "nextToMobject"):
            translation = layout.nextToMobject(
                target,
                vector.x,
                vector.y,
                float(buff),
                edge.x,
                edge.y,
                mask.x,
                mask.y,
            )
        else:
            return _ORIGINAL_GROUP_NEXT_TO(
                self,
                mobject_or_point,
                direction,
                buff,
                aligned_edge=aligned_edge,
                coor_mask=coor_mask,
            )
    else:
        if not hasattr(layout, "nextToPoint"):
            return _ORIGINAL_GROUP_NEXT_TO(
                self,
                mobject_or_point,
                direction,
                buff,
                aligned_edge=aligned_edge,
                coor_mask=coor_mask,
            )
        point = _base._as_vec2(mobject_or_point)
        translation = layout.nextToPoint(
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
'''
    python = replace_once(python, old_target, new_target, label="route selected group next_to")
    python_path.write_text(python)


ownership_path = Path("compat/semantic-ownership-v1.json")
ownership = ownership_path.read_text()
old = '''      "id": "group-placement",
      "surface": "Group/VGroup next_to submobject_to_align/index_of_submobject_to_align selection and arrange forwarded placement kwargs",
      "classification": "python-semantic-duplicate",
      "owner": {"language": "python", "path": "web/python/_manim_semantic_handles.py", "symbol": "_manim_arrange/_group_next_to fallback"},
      "shared_owner": {"language": "rust", "path": "crates/noon-web/src/authoring_mobject.rs", "symbol": "WasmAuthoringFamilyLayout/FrontendFamilyArrangePlan/FrontendFamilyTranslation"},
      "reason": "Default family relative placement and ordinary arrange sequencing are shared-Rust-owned, but explicit wrapper/member aligner selection and forwarded arrange placement kwargs still resolve in Python.",
      "replacement": "Expose shared family-member selection handles and route forwarded arrange alignment policy through the shared family placement API.",
      "migration_issue": "#61"
    },'''
new = '''      "id": "group-member-placement-selection",
      "surface": "Group/VGroup next_to submobject_to_align/index_of_submobject_to_align selection",
      "classification": "shared-rust",
      "owner": {"language": "rust", "path": "crates/noon-web/src/authoring_mobject.rs", "symbol": "FrontendFamilyMemberSelection/WasmAuthoringFamilyMemberLayout/WasmAuthoringLayoutBounds"},
      "adapters": [{"language": "python", "path": "web/python/_manim_semantic_handles.py", "symbol": "_family_member_bounds_handle/_group_next_to"}],
      "reason": "Rust normalizes signed direct-member indices, validates the selected wrapper semantic identity, owns selected live bounds and critical-point placement math, and produces the full-family translation. Explicit source aligners are user-provided call arguments and are converted to shared Rust bounds without a Python geometry calculation."
    },
    {
      "id": "group-arrange-forwarded-placement",
      "surface": "Group/VGroup arrange forwarded next_to placement kwargs",
      "classification": "python-semantic-duplicate",
      "owner": {"language": "python", "path": "web/python/_manim_semantic_handles.py", "symbol": "_manim_arrange/_group_arrange fallback"},
      "shared_owner": {"language": "rust", "path": "crates/noon-web/src/authoring_mobject.rs", "symbol": "FrontendFamilyArrangePlan/WasmAuthoringFamilyMemberLayout/WasmAuthoringFamilyLayout"},
      "reason": "Ordinary arrange and indexed family next_to are shared-Rust-owned, but arrange still forwards arbitrary next_to kwargs through the pinned Python compatibility loop.",
      "replacement": "Add shared arrange policy inputs for aligned_edge/coor_mask and member-aligner selection, reusing shared family member selection.",
      "migration_issue": "#61"
    },'''
ownership = replace_once(ownership, old, new, label="ratchet family member selection")
ownership_path.write_text(ownership)


check_path = Path("scripts/check-web-package.mjs")
check = check_path.read_text()
if '"export class WasmAuthoringFamilyMemberLayout"' not in check:
    # These class anchors occur in both JS and TS surface arrays; replace both.
    class_anchor = '  "export class WasmAuthoringFamilyLayout",\n'
    class_replacement = (
        '  "export class WasmAuthoringFamilyLayout",\n'
        '  "export class WasmAuthoringFamilyMemberLayout",\n'
        '  "export class WasmAuthoringLayoutBounds",\n'
    )
    if check.count(class_anchor) != 2:
        raise RuntimeError(f"family member class anchors: expected 2, found {check.count(class_anchor)}")
    check = check.replace(class_anchor, class_replacement)

    check = replace_once(
        check,
        '  "layoutSession(",\n  "arrangeSession(",\n',
        '  "layoutSession(",\n  "memberLayoutSession(",\n  "layoutBoundsHandle(",\n  "boundsHandle(",\n  "nextToPointWithAligner(",\n  "nextToBoundsWithAligner(",\n  "arrangeSession(",\n',
        label="pin member selection javascript methods",
    )
    check = replace_once(
        check,
        '  "layoutSession(): WasmAuthoringFamilyLayout",\n  "arrangeSession(',
        '  "layoutSession(): WasmAuthoringFamilyLayout",\n  "memberLayoutSession(index: number): WasmAuthoringFamilyMemberLayout",\n  "layoutBoundsHandle(): WasmAuthoringLayoutBounds",\n  "boundsHandle(): WasmAuthoringLayoutBounds",\n  "nextToPointWithAligner(",\n  "nextToBoundsWithAligner(",\n  "arrangeSession(',
        label="pin member selection type methods",
    )
    check_path.write_text(check)


test_path = Path("web/python/test_manim_shared_family_member_selection.py")
if not test_path.exists():
    test_path.write_text(r'''import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimSharedFamilyMemberSelectionTests(unittest.TestCase):
    def test_indexed_and_explicit_group_aligners_stay_in_shared_semantics(self) -> None:
        python_dir = Path(__file__).resolve().parent
        env = os.environ.copy()
        env["PYTHONDONTWRITEBYTECODE"] = "1"
        env["PYTHONPATH"] = str(python_dir)
        source = textwrap.dedent(
            """
            import json
            import _manim_compat
            _manim_compat.install()
            import _manim_phase_b  # noqa: F401
            import _manim_semantic_handles as handles

            class Bounds:
                def __init__(self, store, identity):
                    self.store = store
                    self.identity = identity

            class Translation:
                def __init__(self, store, leaves, delta):
                    self.store = store
                    self.leaves = list(leaves)
                    self.delta = delta
                    self.next = 0
                def applyMobject(self, member):
                    assert member.identity == self.leaves[self.next]
                    member.shift(*self.delta)
                    self.next += 1
                def finish(self):
                    assert self.next == len(self.leaves)

            class Obj:
                def __init__(self, store, snapshot_json):
                    self.store = store
                    self.identity = store.alloc(self)
                    self.snapshot = json.loads(snapshot_json)
                    self.shifts = []
                def snapshotJson(self): return json.dumps(self.snapshot, separators=(\",\", \":\"))
                def replaceSnapshotJson(self, value): self.snapshot = json.loads(value)
                def cloneHandle(self): return Obj(self.store, self.snapshotJson())
                def targetEditor(self): return self.cloneHandle()
                def shift(self, x, y): self.shifts.append((float(x), float(y)))
                def layoutBoundsHandle(self):
                    self.store.bounds_calls.append((\"mobject\", self.identity))
                    return Bounds(self.store, self.identity)
                def setFillOpacity(self, value): pass
                def setStrokeOpacity(self, value): pass

            class Layout:
                def __init__(self, family):
                    self.family = family
                    self.leaves = []
                def includeMobject(self, member): self.leaves.append(member.identity)
                def boundsHandle(self): return Bounds(self.family.store, self.family.identity)
                def nextToBoundsWithAligner(self, source, target, *args):
                    self.family.store.selected_calls.append((\"bounds\", source.identity, target.identity, args))
                    return Translation(self.family.store, self.leaves, (3.0, 0.0))
                def nextToPointWithAligner(self, source, x, y, *args):
                    self.family.store.selected_calls.append((\"point\", source.identity, (x, y), args))
                    return Translation(self.family.store, self.leaves, (-2.0, 1.0))

            class MemberLayout:
                def __init__(self, family, index):
                    self.family = family
                    self.index = index
                    normalized = index if index >= 0 else len(family.members) + index
                    if normalized < 0 or normalized >= len(family.members):
                        raise IndexError(index)
                    self.expected = family.members[normalized]
                    self.accepted = None
                    family.store.member_selections.append((family.identity, index, self.expected))
                def includeMobject(self, member):
                    assert member.identity == self.expected
                    self.accepted = member.identity
                def includeFamily(self, layout):
                    assert layout.family.identity == self.expected
                    self.accepted = layout.family.identity
                def boundsHandle(self):
                    assert self.accepted == self.expected
                    return Bounds(self.family.store, self.expected)

            class Family:
                def __init__(self, store):
                    self.store = store
                    self.identity = store.alloc(self)
                    self.members = []
                def addMobject(self, member):
                    if member.identity not in self.members: self.members.append(member.identity)
                    return True
                def addFamily(self, member):
                    if member.identity not in self.members: self.members.append(member.identity)
                    return True
                def removeMobject(self, member): return False
                def removeFamily(self, member): return False
                @property
                def memberCount(self): return len(self.members)
                def layoutSession(self): return Layout(self)
                def memberLayoutSession(self, index): return MemberLayout(self, int(index))

            class Store:
                def __init__(self):
                    self.next = 0; self.entities = {}; self.member_selections=[]; self.bounds_calls=[]; self.selected_calls=[]
                def alloc(self, entity):
                    value=self.next; self.next+=1; self.entities[value]=entity; return value
                def createMobject(self, snapshot): return Obj(self, snapshot)
                def createFamily(self): return Family(self)

            store = Store()
            handles._create_handle = store.createMobject
            handles._create_family_handle = store.createFamily
            handles.install()
            handles._ORIGINAL_GROUP_NEXT_TO = lambda *a, **k: (_ for _ in ()).throw(AssertionError(\"fallback used\"))

            from noon import Circle, RIGHT, Square, VGroup
            first = Circle(radius=0.2)
            second = Square(side_length=0.4)
            source_nested = VGroup(second)
            source = VGroup(first, source_nested)

            target_first = Circle(radius=0.3)
            target_second = Square(side_length=0.5)
            target_nested = VGroup(target_second)
            target = VGroup(target_first, target_nested)

            source.next_to(target, RIGHT, index_of_submobject_to_align=-1)
            assert store.member_selections[-2:] == [
                (source._semantic_family_handle.identity, -1, source_nested._semantic_family_handle.identity),
                (target._semantic_family_handle.identity, -1, target_nested._semantic_family_handle.identity),
            ]
            assert store.selected_calls[-1][0] == \"bounds\"
            assert second._semantic_handle.shifts[-1] == (3.0, 0.0)

            external = Circle(radius=0.1)
            source.next_to((5.0, 2.0), RIGHT, submobject_to_align=external)
            assert store.bounds_calls[-1] == (\"mobject\", external._semantic_handle.identity)
            assert store.selected_calls[-1][0] == \"point\"
            assert first._semantic_handle.shifts[-1] == (-2.0, 1.0)
            assert second._semantic_handle.shifts[-1] == (-2.0, 1.0)
            """
        )
        completed = subprocess.run(
            [sys.executable, "-c", source], cwd=python_dir, env=env,
            capture_output=True, text=True, check=False,
        )
        self.assertEqual(completed.returncode, 0, msg=f"stdout:\n{completed.stdout}\nstderr:\n{completed.stderr}")

if __name__ == "__main__":
    unittest.main()
''')
