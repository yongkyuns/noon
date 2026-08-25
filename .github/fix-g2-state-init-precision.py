from pathlib import Path

path = Path("web/python/_manim_semantic_handles.py")
text = path.read_text()
before = '''def _init(self: _base.Mobject, raw: _ir.Mobject) -> None:\n    _ORIGINAL_INIT(self, raw)\n    if _create_handle is not None:\n        self._semantic_handle = _create_handle(_snapshot_json(raw))\n        # The handle is now authoritative for detached state. Keeping a second Python\n        # snapshot here would recreate exactly the ownership split #61 is removing.\n        self._raw = None\n'''
after = '''def _init(self: _base.Mobject, raw: _ir.Mobject) -> None:\n    _ORIGINAL_INIT(self, raw)\n    if _create_handle is not None:\n        # Preserve exact Python authoring opacity before the wire/render snapshot\n        # lowers color alpha to f32. The semantic handle owns the f64 API contract.\n        fill = raw.style.get("fill")\n        stroke = raw.style.get("stroke")\n        fill_opacity = None if fill is None else float(fill["alpha"])\n        stroke_opacity = None if stroke is None else float(stroke["alpha"])\n        self._semantic_handle = _create_handle(_snapshot_json(raw))\n        if fill_opacity is not None:\n            self._semantic_handle.setFillOpacity(fill_opacity)\n        if stroke_opacity is not None:\n            self._semantic_handle.setStrokeOpacity(stroke_opacity)\n        # The handle is now authoritative for detached state. Keeping a second Python\n        # snapshot here would recreate exactly the ownership split #61 is removing.\n        self._raw = None\n'''
count = text.count(before)
if count != 1:
    raise SystemExit(f"constructor precision patch expected one match, found {count}")
path.write_text(text.replace(before, after, 1))
