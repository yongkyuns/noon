from pathlib import Path


def replace_once(text: str, before: str, after: str, label: str) -> str:
    count = text.count(before)
    if count != 1:
        raise SystemExit(f"{label}: expected one match, found {count}")
    return text.replace(before, after, 1)


# Shared Rust/WASM object-state operations.
rust_path = Path("crates/noon-web/src/authoring_mobject.rs")
rust = rust_path.read_text()
rust = replace_once(
    rust,
    '''    pub fn set_opacity(&mut self, opacity: f64) -> Result<(), String> {
        let alpha = unit_alpha("opacity", opacity)?;
        if let Some(fill) = &mut self.snapshot.style.fill {
            fill.alpha = alpha;
        }
        if let Some(stroke) = &mut self.snapshot.style.stroke {
            stroke.alpha = alpha;
        }
        Ok(())
    }

    pub fn next_to_handle(
''',
    '''    pub fn set_opacity(&mut self, opacity: f64) -> Result<(), String> {
        let alpha = unit_alpha("opacity", opacity)?;
        if let Some(fill) = &mut self.snapshot.style.fill {
            fill.alpha = alpha;
        }
        if let Some(stroke) = &mut self.snapshot.style.stroke {
            stroke.alpha = alpha;
        }
        Ok(())
    }

    pub fn become_handle(&mut self, other: &Self) {
        self.snapshot = other.snapshot.clone();
    }

    pub fn replace_handle(
        &mut self,
        other: &Self,
        dim_to_match: u32,
        stretch: bool,
    ) -> Result<(), String> {
        if dim_to_match > 1 {
            return Err("replace currently supports width (0) or height (1) in the 2D authoring model".to_owned());
        }
        let target_center = other.center();
        let source_width = self.width();
        let source_height = self.height();
        let target_width = other.width();
        let target_height = other.height();

        if stretch {
            if source_width == 0.0 || source_height == 0.0 {
                return Err("cannot stretch-replace an object with zero width or height".to_owned());
            }
            self.scale(target_width / source_width, target_height / source_height)?;
        } else {
            let (source_length, target_length) = if dim_to_match == 0 {
                (source_width, target_width)
            } else {
                (source_height, target_height)
            };
            if source_length == 0.0 {
                return Err("cannot replace along a zero-length dimension".to_owned());
            }
            let factor = target_length / source_length;
            self.scale(factor, factor)?;
        }
        self.move_to(target_center.0, target_center.1)
    }

    pub fn next_to_handle(
''',
    "rust state methods",
)
rust = replace_once(
    rust,
    '''        #[wasm_bindgen(js_name = setOpacity)]
        pub fn set_opacity(&mut self, opacity: f64) -> Result<(), JsValue> {
            self.0.set_opacity(opacity).map_err(js_error)
        }

        #[wasm_bindgen(js_name = nextToHandle)]
''',
    '''        #[wasm_bindgen(js_name = setOpacity)]
        pub fn set_opacity(&mut self, opacity: f64) -> Result<(), JsValue> {
            self.0.set_opacity(opacity).map_err(js_error)
        }

        #[wasm_bindgen(js_name = becomeHandle)]
        pub fn become_handle(&mut self, other: &WasmAuthoringMobjectHandle) {
            self.0.become_handle(&other.0);
        }

        #[wasm_bindgen(js_name = replaceHandle)]
        pub fn replace_handle(
            &mut self,
            other: &WasmAuthoringMobjectHandle,
            dim_to_match: u32,
            stretch: bool,
        ) -> Result<(), JsValue> {
            self.0
                .replace_handle(&other.0, dim_to_match, stretch)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = nextToHandle)]
''',
    "wasm state methods",
)
rust = replace_once(
    rust,
    '''    #[test]
    fn json_round_trip_preserves_wire_snapshot() {
''',
    '''    #[test]
    fn become_and_replace_keep_state_inside_shared_handle() {
        let mut source = FrontendMobjectHandle::from_snapshot(snapshot(GeometryRef::circle(0.5)));
        source.shift(-2.0, 0.5).unwrap();
        let mut target =
            FrontendMobjectHandle::from_snapshot(snapshot(GeometryRef::rectangle(2.0, 1.0)));
        target.shift(1.0, -0.25).unwrap();

        source.become_handle(&target);
        assert_eq!(source.snapshot(), target.snapshot());

        let mut replacement =
            FrontendMobjectHandle::from_snapshot(snapshot(GeometryRef::circle(0.25)));
        replacement.replace_handle(&target, 0, false).unwrap();
        assert!((replacement.width() - 2.0).abs() < 1e-6);
        assert!((replacement.height() - 2.0).abs() < 1e-6);
        assert!((replacement.center().0 - 1.0).abs() < 1e-6);
        assert!((replacement.center().1 + 0.25).abs() < 1e-6);

        let mut stretched =
            FrontendMobjectHandle::from_snapshot(snapshot(GeometryRef::circle(0.25)));
        stretched.replace_handle(&target, 0, true).unwrap();
        assert!((stretched.width() - 2.0).abs() < 1e-6);
        assert!((stretched.height() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn json_round_trip_preserves_wire_snapshot() {
''',
    "rust state tests",
)
rust_path.write_text(rust)


# Public Manim-compatible state semantics. These run under native differential tests;
# the browser semantic-handle layer overrides become/replace with zero-serialization fast paths.
compat_path = Path("web/python/_manim_compat.py")
compat = compat_path.read_text()
compat = replace_once(
    compat,
    '''\ndef install() -> None:\n''',
    '''\ndef _state_target(self: _BaseMobject, mobject: _BaseMobject, *, match_height: bool, match_width: bool, match_depth: bool, match_center: bool, stretch: bool) -> _BaseMobject:\n    if not isinstance(mobject, _BaseMobject):\n        raise TypeError("state target must be a Mobject")\n    if match_depth:\n        raise NotImplementedError("depth matching requires the shared 2.5D family model")\n    if not (match_height or match_width or match_center or stretch):\n        return mobject\n    target = mobject.copy()\n    if stretch:\n        if target.width == 0.0 or target.height == 0.0:\n            raise ValueError("cannot stretch a zero-width or zero-height target")\n        target.scale((self.width / target.width, self.height / target.height))\n    else:\n        if match_height:\n            if target.height == 0.0:\n                raise ValueError("cannot match height from a zero-height target")\n            target.scale(self.height / target.height)\n        if match_width:\n            if target.width == 0.0:\n                raise ValueError("cannot match width from a zero-width target")\n            target.scale(self.width / target.width)\n    if match_center:\n        target.move_to(self.get_center())\n    return target\n\n\ndef _mobject_generate_target(self: _BaseMobject, use_deepcopy: bool = False) -> _BaseMobject:\n    # Noon's copy already performs a deep semantic clone. In the browser the payload\n    # remains in the Rust/WASM handle, so both Manim modes avoid Python snapshot ownership.\n    del use_deepcopy\n    self.target = None\n    self.target = self.copy()\n    return self.target\n\n\ndef _mobject_save_state(self: _BaseMobject) -> _BaseMobject:\n    if hasattr(self, "saved_state"):\n        self.saved_state = None\n    self.saved_state = self.copy()\n    return self\n\n\ndef _mobject_restore(self: _BaseMobject) -> _BaseMobject:\n    if not hasattr(self, "saved_state") or self.saved_state is None:\n        raise Exception("Trying to restore without having saved")\n    return self.become(self.saved_state)\n\n\ndef _mobject_become(\n    self: _BaseMobject,\n    mobject: _BaseMobject,\n    match_height: bool = False,\n    match_width: bool = False,\n    match_depth: bool = False,\n    match_center: bool = False,\n    stretch: bool = False,\n) -> _BaseMobject:\n    target = _state_target(\n        self,\n        mobject,\n        match_height=match_height,\n        match_width=match_width,\n        match_depth=match_depth,\n        match_center=match_center,\n        stretch=stretch,\n    )\n    return self._apply(_base._raw_mobject(target._current_raw()))\n\n\ndef _mobject_replace(\n    self: _BaseMobject, mobject: _BaseMobject, dim_to_match: int = 0, stretch: bool = False\n) -> _BaseMobject:\n    if not isinstance(mobject, _BaseMobject):\n        raise TypeError("replacement target must be a Mobject")\n    if dim_to_match not in (0, 1):\n        raise NotImplementedError("replace currently supports width (0) or height (1)")\n    if stretch:\n        if self.width == 0.0 or self.height == 0.0:\n            raise ValueError("cannot stretch-replace an object with zero width or height")\n        self.scale((mobject.width / self.width, mobject.height / self.height))\n    else:\n        source_length = self.width if dim_to_match == 0 else self.height\n        target_length = mobject.width if dim_to_match == 0 else mobject.height\n        if source_length == 0.0:\n            raise ValueError("cannot replace along a zero-length dimension")\n        self.scale(target_length / source_length)\n    self.move_to(mobject.get_center())\n    return self\n\n\ndef install() -> None:\n''',
    "compat state functions",
)
compat = replace_once(
    compat,
    '''    _base._as_vec2 = _as_vec2\n    _BaseMobject.animate = property(lambda self: _CompatAnimationBuilder(self))\n''',
    '''    _base._as_vec2 = _as_vec2\n    _BaseMobject.animate = property(lambda self: _CompatAnimationBuilder(self))\n    _BaseMobject.generate_target = _mobject_generate_target\n    _BaseMobject.save_state = _mobject_save_state\n    _BaseMobject.restore = _mobject_restore\n    _BaseMobject.become = _mobject_become\n    _BaseMobject.replace = _mobject_replace\n''',
    "compat state install",
)
compat_path.write_text(compat)


# Browser fast paths for become/replace and safe semantic copies of saved_state/target wrappers.
handles_path = Path("web/python/_manim_semantic_handles.py")
handles = handles_path.read_text()
handles = replace_once(
    handles,
    '''_ORIGINAL_ALIGN_ON_FRAME = _base.Mobject._align_on_frame\n\n_ORIGINAL_SET_FILL = _compat.VMobject.set_fill\n''',
    '''_ORIGINAL_ALIGN_ON_FRAME = _base.Mobject._align_on_frame\n_ORIGINAL_BECOME = _base.Mobject.become\n_ORIGINAL_REPLACE = _base.Mobject.replace\n\n_ORIGINAL_SET_FILL = _compat.VMobject.set_fill\n''',
    "capture state originals",
)
handles = replace_once(
    handles,
    '''    for name, value in self.__dict__.items():\n        if name not in {"_raw", "_scene", "_object", "_semantic_handle"}:\n            setattr(clone, name, copy.deepcopy(value))\n    return clone\n''',
    '''    for name, value in self.__dict__.items():\n        if name not in {"_raw", "_scene", "_object", "_semantic_handle"}:\n            if isinstance(value, _base.Mobject):\n                setattr(clone, name, value.copy())\n            else:\n                setattr(clone, name, copy.deepcopy(value))\n    return clone\n''',
    "semantic copy state wrappers",
)
handles = replace_once(
    handles,
    '''\ndef _critical(value: _base.Mobject, direction: _base.Vec2) -> _base.Vec2:\n''',
    '''\ndef _become(\n    self: _base.Mobject,\n    mobject: _base.Mobject,\n    match_height: bool = False,\n    match_width: bool = False,\n    match_depth: bool = False,\n    match_center: bool = False,\n    stretch: bool = False,\n) -> _base.Mobject:\n    handle = _handle_for(self)\n    other_handle = _handle_for(mobject)\n    if (\n        handle is not None\n        and other_handle is not None\n        and not (match_height or match_width or match_depth or match_center or stretch)\n    ):\n        handle.becomeHandle(other_handle)\n        return self\n    return _ORIGINAL_BECOME(\n        self,\n        mobject,\n        match_height=match_height,\n        match_width=match_width,\n        match_depth=match_depth,\n        match_center=match_center,\n        stretch=stretch,\n    )\n\n\ndef _replace(\n    self: _base.Mobject,\n    mobject: _base.Mobject,\n    dim_to_match: int = 0,\n    stretch: bool = False,\n) -> _base.Mobject:\n    handle = _handle_for(self)\n    other_handle = _handle_for(mobject)\n    if handle is not None and other_handle is not None:\n        if dim_to_match not in (0, 1):\n            raise NotImplementedError("replace currently supports width (0) or height (1)")\n        handle.replaceHandle(other_handle, int(dim_to_match), bool(stretch))\n        return self\n    return _ORIGINAL_REPLACE(self, mobject, dim_to_match=dim_to_match, stretch=stretch)\n\n\ndef _critical(value: _base.Mobject, direction: _base.Vec2) -> _base.Vec2:\n''',
    "semantic state fast paths",
)
handles = replace_once(
    handles,
    '''    _base.Mobject.set_color = _set_color\n    _base.Mobject.next_to = _next_to\n''',
    '''    _base.Mobject.set_color = _set_color\n    _base.Mobject.become = _become\n    _base.Mobject.replace = _replace\n    _base.Mobject.next_to = _next_to\n''',
    "semantic state install",
)
handles_path.write_text(handles)


# Differential fixtures for target/save/restore/become/replace observable behavior.
diff_path = Path("scripts/manim-differential.py")
diff = diff_path.read_text()
state_fixtures = '''\n\ndef _noon_generate_target() -> Any:\n    source = noon.Circle(radius=0.4).shift(noon.LEFT * 0.6)\n    target = source.generate_target().shift(noon.RIGHT * 1.5).scale(1.5)\n    return {"source": _object_observation(source), "target": _object_observation(target)}\n\n\ndef _manim_generate_target() -> Any:\n    source = manim.Circle(radius=0.4).shift(manim.LEFT * 0.6)\n    target = source.generate_target().shift(manim.RIGHT * 1.5).scale(1.5)\n    return {"source": _object_observation(source), "target": _object_observation(target)}\n\n\ndef _noon_save_restore() -> Any:\n    obj = noon.Rectangle(width=1.2, height=0.6).shift(noon.LEFT * 0.5)\n    obj.save_state().shift(noon.RIGHT * 2.0).scale(1.75).restore()\n    return _object_observation(obj)\n\n\ndef _manim_save_restore() -> Any:\n    obj = manim.Rectangle(width=1.2, height=0.6).shift(manim.LEFT * 0.5)\n    obj.save_state().shift(manim.RIGHT * 2.0).scale(1.75).restore()\n    return _object_observation(obj)\n\n\ndef _noon_become() -> Any:\n    source = noon.Circle(radius=0.4).shift(noon.LEFT)\n    target = noon.Rectangle(width=1.6, height=0.8).shift(noon.RIGHT * 1.25 + noon.UP * 0.4)\n    source.become(target)\n    return _object_observation(source)\n\n\ndef _manim_become() -> Any:\n    source = manim.Circle(radius=0.4).shift(manim.LEFT)\n    target = manim.Rectangle(width=1.6, height=0.8).shift(manim.RIGHT * 1.25 + manim.UP * 0.4)\n    source.become(target)\n    return _object_observation(source)\n\n\ndef _noon_replace_width() -> Any:\n    source = noon.Circle(radius=0.25)\n    target = noon.Rectangle(width=2.0, height=1.0).shift(noon.RIGHT * 0.8 + noon.DOWN * 0.3)\n    source.replace(target)\n    return _object_observation(source)\n\n\ndef _manim_replace_width() -> Any:\n    source = manim.Circle(radius=0.25)\n    target = manim.Rectangle(width=2.0, height=1.0).shift(manim.RIGHT * 0.8 + manim.DOWN * 0.3)\n    source.replace(target)\n    return _object_observation(source)\n\n\ndef _noon_replace_stretch() -> Any:\n    source = noon.Circle(radius=0.25)\n    target = noon.Rectangle(width=2.0, height=1.0).shift(noon.LEFT * 0.7 + noon.UP * 0.2)\n    source.replace(target, stretch=True)\n    return _object_observation(source)\n\n\ndef _manim_replace_stretch() -> Any:\n    source = manim.Circle(radius=0.25)\n    target = manim.Rectangle(width=2.0, height=1.0).shift(manim.LEFT * 0.7 + manim.UP * 0.2)\n    source.replace(target, stretch=True)\n    return _object_observation(source)\n'''
diff = replace_once(diff, "\n\nFIXTURES = [\n", state_fixtures + "\n\nFIXTURES = [\n", "differential state functions")
diff = replace_once(
    diff,
    '''    Fixture("vgroup_rotate", _noon_vgroup_rotate, _manim_vgroup_rotate),\n]\n''',
    '''    Fixture("vgroup_rotate", _noon_vgroup_rotate, _manim_vgroup_rotate),\n    Fixture("generate_target", _noon_generate_target, _manim_generate_target),\n    Fixture("save_restore", _noon_save_restore, _manim_save_restore),\n    Fixture("become", _noon_become, _manim_become),\n    Fixture("replace_width", _noon_replace_width, _manim_replace_width),\n    Fixture("replace_stretch", _noon_replace_stretch, _manim_replace_stretch),\n]\n''',
    "differential state fixture list",
)
diff_path.write_text(diff)
