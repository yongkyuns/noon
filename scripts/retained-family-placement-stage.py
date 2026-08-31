from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    file = Path(path)
    text = file.read_text()
    count = text.count(old)
    if count != 1:
        raise RuntimeError(f"{path}: expected one exact placement context, found {count}")
    file.write_text(text.replace(old, new, 1))


replace_once(
    "crates/noon-web/src/authoring_mobject.rs",
    '''        Ok(member.id)
    }

    #[wasm_bindgen]
    pub struct WasmAuthoringStore {
''',
    '''        Ok(member.id)
    }

    fn retained_native_critical_point(
        text: &WasmRetainedNativeTextAuthoringHandle,
        direction: (f64, f64),
    ) -> (f64, f64) {
        let bounds = text.family_layout_bounds();
        let center = (
            (bounds.min_x + bounds.max_x) * 0.5,
            (bounds.min_y + bounds.max_y) * 0.5,
        );
        (
            if direction.0 < 0.0 {
                bounds.min_x
            } else if direction.0 > 0.0 {
                bounds.max_x
            } else {
                center.0
            },
            if direction.1 < 0.0 {
                bounds.min_y
            } else if direction.1 > 0.0 {
                bounds.max_y
            } else {
                center.1
            },
        )
    }

    #[wasm_bindgen]
    pub struct WasmAuthoringStore {
''',
)

replace_once(
    "crates/noon-web/src/authoring_mobject.rs",
    '''        #[wasm_bindgen(js_name = moveToFamily)]
        pub fn move_to_family(
''',
    '''        #[wasm_bindgen(js_name = moveToRetainedNativeText)]
        pub fn move_to_retained_native_text(
            &self,
            target_member: &WasmAuthoringFamilyMemberHandle,
            target: &WasmRetainedNativeTextAuthoringHandle,
            aligned_edge_x: f64,
            aligned_edge_y: f64,
            mask_x: f64,
            mask_y: f64,
        ) -> Result<WasmAuthoringFamilyTranslation, JsValue> {
            self.ensure_complete()?;
            retained_native_member_id(
                &self.semantics,
                target_member,
                target,
                "family placement target",
            )?;
            let edge = semantic_xy_f64(aligned_edge_x, aligned_edge_y).map_err(js_error)?;
            let mask = semantic_xy_f64(mask_x, mask_y).map_err(js_error)?;
            let source_x = self.critical_x(edge.x, edge.y)?;
            let source_y = self.critical_y(edge.x, edge.y)?;
            let target_point = retained_native_critical_point(target, (edge.x, edge.y));
            self.translation(
                (target_point.0 - source_x) * mask.x,
                (target_point.1 - source_y) * mask.y,
            )
        }

        #[wasm_bindgen(js_name = moveToFamily)]
        pub fn move_to_family(
''',
)

replace_once(
    "crates/noon-web/src/authoring_mobject.rs",
    '''        #[wasm_bindgen(js_name = nextToFamily)]
        #[allow(clippy::too_many_arguments)]
        pub fn next_to_family(
''',
    '''        #[wasm_bindgen(js_name = nextToRetainedNativeText)]
        #[allow(clippy::too_many_arguments)]
        pub fn next_to_retained_native_text(
            &self,
            target_member: &WasmAuthoringFamilyMemberHandle,
            target: &WasmRetainedNativeTextAuthoringHandle,
            direction_x: f64,
            direction_y: f64,
            buff: f64,
            aligned_edge_x: f64,
            aligned_edge_y: f64,
            mask_x: f64,
            mask_y: f64,
        ) -> Result<WasmAuthoringFamilyTranslation, JsValue> {
            self.ensure_complete()?;
            retained_native_member_id(
                &self.semantics,
                target_member,
                target,
                "family placement target",
            )?;
            let direction = semantic_xy_f64(direction_x, direction_y).map_err(js_error)?;
            let edge = semantic_xy_f64(aligned_edge_x, aligned_edge_y).map_err(js_error)?;
            let source = (
                self.critical_x(edge.x - direction.x, edge.y - direction.y)?,
                self.critical_y(edge.x - direction.x, edge.y - direction.y)?,
            );
            let target_point = retained_native_critical_point(
                target,
                (edge.x + direction.x, edge.y + direction.y),
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

        #[wasm_bindgen(js_name = nextToFamily)]
        #[allow(clippy::too_many_arguments)]
        pub fn next_to_family(
''',
)

replace_once(
    "crates/noon-web/src/authoring_mobject.rs",
    '''        #[wasm_bindgen(js_name = alignToFamily)]
        pub fn align_to_family(
''',
    '''        #[wasm_bindgen(js_name = alignToRetainedNativeText)]
        pub fn align_to_retained_native_text(
            &self,
            target_member: &WasmAuthoringFamilyMemberHandle,
            target: &WasmRetainedNativeTextAuthoringHandle,
            axis_x: f64,
            axis_y: f64,
        ) -> Result<WasmAuthoringFamilyTranslation, JsValue> {
            self.ensure_complete()?;
            retained_native_member_id(
                &self.semantics,
                target_member,
                target,
                "family placement target",
            )?;
            let axis = semantic_xy_f64(axis_x, axis_y).map_err(js_error)?;
            let source = (
                self.critical_x(axis.x, axis.y)?,
                self.critical_y(axis.x, axis.y)?,
            );
            let target_point = retained_native_critical_point(target, (axis.x, axis.y));
            let delta = manim_family_align_to_delta(source, target_point, (axis.x, axis.y))
                .map_err(js_error)?;
            self.translation(delta.0, delta.1)
        }

        #[wasm_bindgen(js_name = alignToFamily)]
        pub fn align_to_family(
''',
)

replace_once(
    "web/python/_manim_semantic_handles.py",
    '''    elif _alignment_is_mobject(point_or_mobject):
        target_handle = _handle_for(point_or_mobject)
        if target_handle is not None and hasattr(session, "moveToMobject"):
            translation = session.moveToMobject(
                target_handle, edge.x, edge.y, mask.x, mask.y
            )
    elif hasattr(session, "moveToPoint"):
''',
    '''    elif _alignment_is_mobject(point_or_mobject):
        target_adapter = _family_layout_leaf_adapter(point_or_mobject)
        if target_adapter is not None:
            kind, payload = target_adapter
            if kind == "mobject" and hasattr(session, "moveToMobject"):
                translation = session.moveToMobject(
                    payload, edge.x, edge.y, mask.x, mask.y
                )
            elif kind == "retained_native_text" and hasattr(
                session, "moveToRetainedNativeText"
            ):
                identity, retained = payload
                translation = session.moveToRetainedNativeText(
                    identity, retained, edge.x, edge.y, mask.x, mask.y
                )
    elif hasattr(session, "moveToPoint"):
''',
)

replace_once(
    "web/python/_manim_semantic_handles.py",
    '''    elif _alignment_is_mobject(mobject_or_point):
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
''',
    '''    elif _alignment_is_mobject(mobject_or_point):
        target_adapter = _family_layout_leaf_adapter(mobject_or_point)
        if target_adapter is not None:
            kind, payload = target_adapter
            if kind == "mobject" and hasattr(session, "nextToMobject"):
                translation = session.nextToMobject(
                    payload,
                    vector.x,
                    vector.y,
                    float(buff),
                    edge.x,
                    edge.y,
                    mask.x,
                    mask.y,
                )
            elif kind == "retained_native_text" and hasattr(
                session, "nextToRetainedNativeText"
            ):
                identity, retained = payload
                translation = session.nextToRetainedNativeText(
                    identity,
                    retained,
                    vector.x,
                    vector.y,
                    float(buff),
                    edge.x,
                    edge.y,
                    mask.x,
                    mask.y,
                )
    elif hasattr(session, "nextToPoint"):
''',
)

replace_once(
    "web/python/_manim_semantic_handles.py",
    '''    elif _alignment_is_mobject(mobject_or_point):
        target_handle = _handle_for(mobject_or_point)
        if target_handle is not None and hasattr(session, "alignToMobject"):
            translation = session.alignToMobject(target_handle, axis.x, axis.y)
    elif hasattr(session, "alignToPoint"):
''',
    '''    elif _alignment_is_mobject(mobject_or_point):
        target_adapter = _family_layout_leaf_adapter(mobject_or_point)
        if target_adapter is not None:
            kind, payload = target_adapter
            if kind == "mobject" and hasattr(session, "alignToMobject"):
                translation = session.alignToMobject(payload, axis.x, axis.y)
            elif kind == "retained_native_text" and hasattr(
                session, "alignToRetainedNativeText"
            ):
                identity, retained = payload
                translation = session.alignToRetainedNativeText(
                    identity, retained, axis.x, axis.y
                )
    elif hasattr(session, "alignToPoint"):
''',
)

replace_once(
    "scripts/retained-text-family-layout-smoke.mjs",
    '''        square_before = square.get_center()
        mixed_text_before = mixed_text.get_center()
        mixed.shift(DOWN * 0.6)
        close(square.get_center().y, square_before.y - 0.6, "mixed family square shift")
        close(mixed_text.get_center().y, mixed_text_before.y - 0.6, "mixed family text shift")

        typst_family = VGroup(Typst("*Typst*", font_size=36))
''',
    '''        square_before = square.get_center()
        mixed_text_before = mixed_text.get_center()
        mixed.shift(DOWN * 0.6)
        close(square.get_center().y, square_before.y - 0.6, "mixed family square shift")
        close(mixed_text.get_center().y, mixed_text_before.y - 0.6, "mixed family text shift")

        placement_target = Text("Target", font_size=34).shift(RIGHT * 2.5 + UP * 0.8)
        placement_a = Text("P", font_size=30)
        placement_b = Text("QQ", font_size=30).shift(RIGHT * 0.8)
        placement = VGroup(placement_a, placement_b)
        placement.move_to(placement_target)
        close(placement.get_center().x, placement_target.get_center().x, "move_to retained target x")
        close(placement.get_center().y, placement_target.get_center().y, "move_to retained target y")
        placement.next_to(placement_target, RIGHT, buff=0.25)
        gap = placement.get_critical_point(LEFT).x - placement_target.get_critical_point(RIGHT).x
        close(gap, 0.25, "next_to retained target gap")
        placement.align_to(placement_target, UP)
        close(
            placement.get_critical_point(UP).y,
            placement_target.get_critical_point(UP).y,
            "align_to retained target top",
        )

        typst_family = VGroup(Typst("*Typst*", font_size=36))
''',
)

print("retained family placement patch applied")
