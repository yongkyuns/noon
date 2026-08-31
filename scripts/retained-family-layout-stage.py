from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    file = Path(path)
    text = file.read_text()
    count = text.count(old)
    if count != 1:
        raise RuntimeError(f"{path}: expected one exact patch context, found {count}")
    file.write_text(text.replace(old, new, 1))


replace_once(
    "crates/noon-web/src/retained_authoring.rs",
    '''#[cfg(any(target_arch = "wasm32", test))]
use noon_core::Rect;
use noon_core::{Color, ObjectId, Transform2D, Vec2, WHITE};
''',
    '''#[cfg(any(target_arch = "wasm32", test))]
use noon_core::Rect;
#[cfg(target_arch = "wasm32")]
use noon_core::{Bounds2D64, SemanticNodeId};
use noon_core::{Color, ObjectId, Transform2D, Vec2, WHITE};
''',
)

replace_once(
    "crates/noon-web/src/retained_authoring.rs",
    '''    pub struct WasmRetainedNativeTextAuthoringHandle {
        inner: RetainedTextAuthoringSpec,
        intrinsic_bounds: Rect,
    }

    impl WasmRetainedNativeTextAuthoringHandle {
        fn bounds(&self) -> Rect {
            transformed_bounds(self.intrinsic_bounds, self.inner.transform)
        }
    }
''',
    '''    pub struct WasmRetainedNativeTextAuthoringHandle {
        inner: RetainedTextAuthoringSpec,
        intrinsic_bounds: Rect,
        family_identity: Option<SemanticNodeId>,
    }

    impl WasmRetainedNativeTextAuthoringHandle {
        fn bounds(&self) -> Rect {
            transformed_bounds(self.intrinsic_bounds, self.inner.transform)
        }

        pub(crate) fn bind_family_identity(
            &mut self,
            identity: SemanticNodeId,
        ) -> Result<(), String> {
            if let Some(existing) = self.family_identity {
                if existing != identity {
                    return Err(
                        "retained native Text is already bound to another family identity"
                            .to_owned(),
                    );
                }
                return Ok(());
            }
            self.family_identity = Some(identity);
            Ok(())
        }

        pub(crate) fn family_identity(&self) -> Option<SemanticNodeId> {
            self.family_identity
        }

        pub(crate) fn family_layout_bounds(&self) -> Bounds2D64 {
            let bounds = self.bounds();
            Bounds2D64 {
                min_x: f64::from(bounds.min.x),
                min_y: f64::from(bounds.min.y),
                max_x: f64::from(bounds.max.x),
                max_y: f64::from(bounds.max.y),
            }
        }

        pub(crate) fn apply_family_translation(
            &mut self,
            delta_x: f64,
            delta_y: f64,
        ) -> Result<(), String> {
            let current = self.inner.transform.translation;
            let next_x = f64::from(current.x) + delta_x;
            let next_y = f64::from(current.y) + delta_y;
            if !next_x.is_finite()
                || !next_y.is_finite()
                || next_x.abs() > f64::from(f32::MAX)
                || next_y.abs() > f64::from(f32::MAX)
            {
                return Err(
                    "retained native Text family translation must remain f32-compatible"
                        .to_owned(),
                );
            }
            self.inner
                .move_to(Vec2::new(next_x as f32, next_y as f32))
        }
    }
''',
)

replace_once(
    "crates/noon-web/src/retained_authoring.rs",
    '''            Ok(Self {
                inner,
                intrinsic_bounds,
            })
''',
    '''            Ok(Self {
                inner,
                intrinsic_bounds,
                family_identity: None,
            })
''',
)

replace_once(
    "crates/noon-web/src/authoring_mobject.rs",
    '''    use wasm_bindgen::prelude::*;

    use super::{
''',
    '''    use wasm_bindgen::prelude::*;

    use crate::WasmRetainedNativeTextAuthoringHandle;

    use super::{
''',
)

replace_once(
    "crates/noon-web/src/authoring_mobject.rs",
    '''    pub fn apply(
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
''',
    '''    fn apply_with<F>(
        &mut self,
        source_member: SemanticNodeId,
        apply: F,
    ) -> Result<(), String>
    where
        F: FnOnce((f64, f64)) -> Result<(), String>,
    {
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
        apply(self.delta)?;
        self.next_index += 1;
        Ok(())
    }

    pub fn apply(
        &mut self,
        source_member: SemanticNodeId,
        member: &mut FrontendMobjectHandle,
    ) -> Result<(), String> {
        self.apply_with(source_member, |delta| member.shift(delta.0, delta.1))
    }
''',
)

replace_once(
    "crates/noon-web/src/authoring_mobject.rs",
    '''    type SharedSemanticStore = Rc<RefCell<SemanticStore>>;

    #[wasm_bindgen]
    pub struct WasmAuthoringStore {
''',
    '''    type SharedSemanticStore = Rc<RefCell<SemanticStore>>;

    fn retained_native_member_id(
        semantics: &SharedSemanticStore,
        member: &WasmAuthoringFamilyMemberHandle,
        text: &WasmRetainedNativeTextAuthoringHandle,
        context: &str,
    ) -> Result<SemanticNodeId, JsValue> {
        if !Rc::ptr_eq(semantics, &member.semantics) {
            return Err(JsValue::from_str(&format!(
                "{context} and retained member belong to different authoring stores"
            )));
        }
        if text.family_identity() != Some(member.id) {
            return Err(JsValue::from_str(&format!(
                "{context} retained member identity does not match retained native Text handle"
            )));
        }
        Ok(member.id)
    }

    #[wasm_bindgen]
    pub struct WasmAuthoringStore {
''',
)

replace_once(
    "crates/noon-web/src/authoring_mobject.rs",
    '''        #[wasm_bindgen(getter, js_name = semanticGeneration)]
        pub fn semantic_generation(&self) -> u32 {
            self.id.generation()
        }
    }
''',
    '''        #[wasm_bindgen(getter, js_name = semanticGeneration)]
        pub fn semantic_generation(&self) -> u32 {
            self.id.generation()
        }

        #[wasm_bindgen(js_name = bindRetainedNativeText)]
        pub fn bind_retained_native_text(
            &self,
            text: &mut WasmRetainedNativeTextAuthoringHandle,
        ) -> Result<(), JsValue> {
            text.bind_family_identity(self.id).map_err(js_error)
        }
    }
''',
)

replace_once(
    "crates/noon-web/src/authoring_mobject.rs",
    '''        #[wasm_bindgen(js_name = includeMobject)]
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
''',
    '''        #[wasm_bindgen(js_name = includeMobject)]
        pub fn include_mobject(
            &mut self,
            member: &WasmAuthoringMobjectHandle,
        ) -> Result<(), JsValue> {
            let id = self.mobject_member_id(member)?;
            self.plan
                .accept_member_bounds(id, member.0.layout_bounds())
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = includeRetainedNativeText)]
        pub fn include_retained_native_text(
            &mut self,
            member: &WasmAuthoringFamilyMemberHandle,
            text: &WasmRetainedNativeTextAuthoringHandle,
        ) -> Result<(), JsValue> {
            let id = retained_native_member_id(
                &self.semantics,
                member,
                text,
                "family arrange",
            )?;
            self.plan
                .accept_member_bounds(id, Some(text.family_layout_bounds()))
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = includeFamily)]
''',
)

replace_once(
    "crates/noon-web/src/authoring_mobject.rs",
    '''    impl WasmAuthoringFamilyLayout {
        fn ensure_complete(&self) -> Result<(), JsValue> {
''',
    '''    impl WasmAuthoringFamilyLayout {
        fn include_leaf_bounds(
            &mut self,
            id: SemanticNodeId,
            bounds: Option<Bounds2D64>,
        ) -> Result<(), JsValue> {
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
            if let Some(bounds) = bounds {
                self.include_bounds(bounds);
            }
            self.next_leaf += 1;
            Ok(())
        }

        fn ensure_complete(&self) -> Result<(), JsValue> {
''',
)

replace_once(
    "crates/noon-web/src/authoring_mobject.rs",
    '''        #[wasm_bindgen(js_name = includeMobject)]
        pub fn include_mobject(
            &mut self,
            member: &WasmAuthoringMobjectHandle,
        ) -> Result<(), JsValue> {
            let store = member.1.as_ref().ok_or_else(|| {
                JsValue::from_str(
                    "family layout member is not attached to a shared authoring store",
                )
            })?;
            if !Rc::ptr_eq(&self.semantics, store) {
                return Err(JsValue::from_str(
                    "family layout and mobject belong to different authoring stores",
                ));
            }
            let id = member.2.ok_or_else(|| {
                JsValue::from_str("family layout member has no semantic identity")
            })?;
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
''',
    '''        #[wasm_bindgen(js_name = includeMobject)]
        pub fn include_mobject(
            &mut self,
            member: &WasmAuthoringMobjectHandle,
        ) -> Result<(), JsValue> {
            let store = member.1.as_ref().ok_or_else(|| {
                JsValue::from_str(
                    "family layout member is not attached to a shared authoring store",
                )
            })?;
            if !Rc::ptr_eq(&self.semantics, store) {
                return Err(JsValue::from_str(
                    "family layout and mobject belong to different authoring stores",
                ));
            }
            let id = member.2.ok_or_else(|| {
                JsValue::from_str("family layout member has no semantic identity")
            })?;
            self.include_leaf_bounds(id, member.0.layout_bounds())
        }

        #[wasm_bindgen(js_name = includeRetainedNativeText)]
        pub fn include_retained_native_text(
            &mut self,
            member: &WasmAuthoringFamilyMemberHandle,
            text: &WasmRetainedNativeTextAuthoringHandle,
        ) -> Result<(), JsValue> {
            let id = retained_native_member_id(
                &self.semantics,
                member,
                text,
                "family layout",
            )?;
            self.include_leaf_bounds(id, Some(text.family_layout_bounds()))
        }

        #[wasm_bindgen(getter, js_name = centerX)]
''',
)

replace_once(
    "crates/noon-web/src/authoring_mobject.rs",
    '''        #[wasm_bindgen(js_name = applyMobject)]
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
''',
    '''        #[wasm_bindgen(js_name = applyMobject)]
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

        #[wasm_bindgen(js_name = applyRetainedNativeText)]
        pub fn apply_retained_native_text(
            &mut self,
            member: &WasmAuthoringFamilyMemberHandle,
            text: &mut WasmRetainedNativeTextAuthoringHandle,
        ) -> Result<(), JsValue> {
            let id = retained_native_member_id(
                &self.semantics,
                member,
                text,
                "family translation",
            )?;
            self.translation
                .apply_with(id, |delta| text.apply_family_translation(delta.0, delta.1))
                .map_err(js_error)
        }

        pub fn finish(&self) -> Result<(), JsValue> {
''',
)

replace_once(
    "web/python/_manim_typst.py",
    '''        self._line_spacing = float(line_spacing)
        self._initialize_retained(str(text), float(font_size), handle, color, opacity)

    @property
    def text(self) -> str:
''',
    '''        self._line_spacing = float(line_spacing)
        self._initialize_retained(str(text), float(font_size), handle, color, opacity)
        if self._semantic_family_member_handle is not None:
            self._semantic_family_member_handle.bindRetainedNativeText(self._retained_handle)

    @property
    def text(self) -> str:
''',
)

replace_once(
    "web/python/_manim_semantic_handles.py",
    '''def _shared_family_layout_session(value: object, *, mutation: bool = False):
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
''',
    '''def _retained_family_layout_handles(value: object):
    identity = getattr(value, "_semantic_family_member_handle", None)
    retained = getattr(value, "_retained_handle", None)
    if identity is None or retained is None:
        return None
    if not _has_shared_layout_queries(retained):
        raise NotImplementedError(
            "Typst/MathTypst family layout requires Rust-owned retained layout bounds"
        )
    return identity, retained


def _family_layout_leaf_adapter(value: object, *, mutation: bool = False):
    resolver = _mutation_handle_for if mutation else _handle_for
    handle = resolver(value)
    if handle is not None:
        return "mobject", handle
    retained = _retained_family_layout_handles(value)
    if retained is not None:
        return "retained_native_text", retained
    return None


def _shared_family_layout_session(value: object, *, mutation: bool = False):
    if not isinstance(value, _compat.Group):
        return None
    family_handle = getattr(value, "_semantic_family_handle", None)
    if family_handle is None or not hasattr(family_handle, "layoutSession"):
        return None
    leaves = _compat._leaf_mobjects(value)
    leaf_adapters = [
        _family_layout_leaf_adapter(member, mutation=mutation) for member in leaves
    ]
    if not all(adapter is not None for adapter in leaf_adapters):
        return None
    session = family_handle.layoutSession()
    for adapter in leaf_adapters:
        assert adapter is not None
        kind, payload = adapter
        if kind == "mobject":
            session.includeMobject(payload)
        else:
            identity, retained = payload
            session.includeRetainedNativeText(identity, retained)
    return session, leaves, leaf_adapters


def _apply_family_translation(
    self: _compat.Group,
    translation: object,
    leaves: list[_base.Mobject],
    leaf_adapters: list[object],
) -> _compat.Group:
    for member, adapter in zip(leaves, leaf_adapters):
        kind, payload = adapter
        if kind == "mobject":
            translation.applyMobject(payload)
            _sync_bound_transform(member, payload)
        else:
            identity, retained = payload
            translation.applyRetainedNativeText(identity, retained)
    translation.finish()
    return self
''',
)

replace_once(
    "web/python/_manim_semantic_handles.py",
    '''        elif isinstance(member, _base.Mobject):
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
''',
    '''        elif isinstance(member, _base.Mobject):
            adapter = _family_layout_leaf_adapter(member, mutation=True)
            if adapter is None:
                return _ORIGINAL_GROUP_ARRANGE(
                    self, direction=direction, buff=buff, center=center
                )
            kind, payload = adapter
            if kind == "mobject":
                arrangement.includeMobject(payload)
            else:
                identity, retained = payload
                arrangement.includeRetainedNativeText(identity, retained)
            prepared.append((member, [member], [adapter]))
        else:
            return _ORIGINAL_GROUP_ARRANGE(self, direction=direction, buff=buff, center=center)

    for member, leaves, leaf_adapters in prepared:
        translation = arrangement.nextTranslation()
        _apply_family_translation(member, translation, leaves, leaf_adapters)
''',
)

replace_once(
    "web/python/_manim_semantic_handles.py",
    '''    # Group/VGroup wrapper traversal remains host-language metadata, but the shared
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
''',
    '''    # Group/VGroup wrapper traversal remains host-language metadata, but the shared
    # family graph independently derives the expected recursive leaf sequence and
    # rejects any wrapper divergence. Rust owns the actual aggregate bounds math.
    if isinstance(value, _compat.Group):
        shared = _shared_family_layout_session(value)
        if shared is not None:
            session = shared[0]
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
        handle = _handle_for(member)
        if handle is not None:
            bounds = _layout_bounds(member)
        else:
            retained = _retained_family_layout_handles(member)
            if retained is not None:
                retained_handle = retained[1]
                bounds = (
                    _base.Vec2(
                        float(retained_handle.criticalX(-1.0, 0.0)),
                        float(retained_handle.criticalY(0.0, -1.0)),
                    ),
                    _base.Vec2(
                        float(retained_handle.criticalX(1.0, 0.0)),
                        float(retained_handle.criticalY(0.0, 1.0)),
                    ),
                )
            else:
                bounds = _base._bounds(member._current_raw())
        if bounds is not None:
            present.append(bounds)
''',
)

print("retained family layout patch applied")
