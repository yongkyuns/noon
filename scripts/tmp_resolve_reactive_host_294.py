from pathlib import Path

geometry = Path("web/python/_manim_geometry.py")
text = geometry.read_text()
match_points = '''\n\ndef match_points(self: _base.Mobject, mobject: object) -> _base.Mobject:\n    if not isinstance(mobject, _base.Mobject):\n        raise TypeError("match_points expects a Mobject")\n    source = self._current_raw()\n    target = mobject._current_raw()\n    source_kind = next(iter(source.geometry), None)\n    target_kind = next(iter(target.geometry), None)\n    if source_kind != target_kind or source_kind not in {"line", "vector_path"}:\n        raise NotImplementedError(\n            "match_points currently supports Line/VMobject-path pairs with matching geometry kinds"\n        )\n    # Manim stores transformed VMobject points directly. Noon separates affine placement,\n    # so copying the target point state also copies its affine placement while source style\n    # (notably MovingDots' red line color) remains untouched. _ir snapshots are immutable,\n    # therefore construct one replacement value rather than mutating a frozen dataclass.\n    raw = _base._ir.Mobject(\n        geometry=copy.deepcopy(target.geometry),\n        transform=copy.deepcopy(target.transform),\n        style=copy.deepcopy(source.style),\n    )\n    return self._apply(raw)\n'''
if "def match_points(self:" not in text:
    marker = "\n\ndef install() -> None:\n"
    if marker not in text:
        raise SystemExit("geometry install marker missing")
    text = text.replace(marker, match_points + marker, 1)
if "_base.Mobject.match_points = match_points" not in text:
    marker = "    _compat._bounds_for = _bounds_for\n"
    if marker not in text:
        raise SystemExit("geometry bounds hook marker missing")
    text = text.replace(marker, marker + "    _base.Mobject.match_points = match_points\n", 1)
geometry.write_text(text)

updaters = Path("web/python/_manim_updaters.py")
text = updaters.read_text()
old_geometry = '''            if before.geometry != after.geometry:\n                raise NotImplementedError(\n                    "host updaters cannot mutate geometry yet; use transform/style "\n                    "mutations or native reactive expressions"\n                )\n'''
new_geometry = '''            if before.geometry != after.geometry:\n                batch.set_geometry(object_id, after.geometry)\n'''
if old_geometry in text:
    text = text.replace(old_geometry, new_geometry, 1)
elif new_geometry not in text:
    raise SystemExit("updater geometry mutation marker missing")

context_marker = "    _ACTIVE_CONTEXTS[scene_key] = context\n"
context_block = '''    _ACTIVE_CONTEXTS[scene_key] = context\n\n    # Keep the updater adapter usable by native Python tests and non-reactive scenes:\n    # importing the reactive facade eagerly would require Pyodide's `js` bridge even\n    # when this callback phase has no signals. Only enter the ValueTracker signal\n    # context when the runtime actually supplied signal values.\n    reactive = None\n    if frame.get("signals"):\n        import _manim_reactive as reactive\n\n        reactive._enter_callback_signal_values(frame)\n'''
if "reactive._enter_callback_signal_values(frame)" not in text:
    if context_marker not in text:
        raise SystemExit("updater callback context marker missing")
    text = text.replace(context_marker, context_block, 1)

finally_marker = '''    finally:\n        _ACTIVE_CONTEXTS.pop(scene_key, None)\n'''
finally_block = '''    finally:\n        if reactive is not None:\n            reactive._leave_callback_signal_values()\n        _ACTIVE_CONTEXTS.pop(scene_key, None)\n'''
if "reactive._leave_callback_signal_values()" not in text:
    if finally_marker not in text:
        raise SystemExit("updater callback finally marker missing")
    text = text.replace(finally_marker, finally_block, 1)
updaters.write_text(text)
