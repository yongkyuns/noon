from pathlib import Path
import runpy
import subprocess


try:
    runpy.run_path("scripts/apply_shared_layout_bounds_migration.py", run_name="__main__")
except SystemExit as error:
    if "web/python/test_manim_semantic_handle_layout_bounds.py" not in str(error):
        raise

# The first migration version intentionally stopped while trying to rewrite an old
# nested-string fake handle. Keep that existing regression unchanged; a dedicated
# shared-query regression covers the new no-snapshot contract instead.
subprocess.run(
    ["git", "checkout", "--", "web/python/test_manim_semantic_handle_layout_bounds.py"],
    check=True,
)


def replace_once(path: str, old: str, new: str) -> None:
    p = Path(path)
    text = p.read_text()
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected one replacement, found {count}")
    p.write_text(text.replace(old, new, 1))


path = "web/python/_manim_semantic_handles.py"
replace_once(
    path,
    '''def _handle_for(value: object):
    if not isinstance(value, _base.Mobject):
        return None
    if value._scene is not None or value._object is not None:
        return None
    return getattr(value, "_semantic_handle", None)


def _layout_bounds''',
    '''def _handle_for(value: object):
    if not isinstance(value, _base.Mobject):
        return None
    if value._scene is not None or value._object is not None:
        return None
    return getattr(value, "_semantic_handle", None)


def _has_shared_layout_queries(handle: object) -> bool:
    return handle is not None and all(
        hasattr(handle, name)
        for name in ("centerX", "centerY", "width", "height", "criticalX", "criticalY")
    )


def _layout_bounds''',
)
replace_once(
    path,
    '''def _layout_bounds(value: _base.Mobject) -> tuple[_base.Vec2, _base.Vec2] | None:
    """Read exact world-space layout bounds from a detached shared handle."""

    handle = _handle_for(value)
    if handle is None:
        return _base._bounds(value._current_raw())
    return (
        _base.Vec2(
            float(handle.criticalX(-1.0, 0.0)),
            float(handle.criticalY(0.0, -1.0)),
        ),
        _base.Vec2(
            float(handle.criticalX(1.0, 0.0)),
            float(handle.criticalY(0.0, 1.0)),
        ),
    )
''',
    '''def _layout_bounds(value: _base.Mobject) -> tuple[_base.Vec2, _base.Vec2] | None:
    """Read exact world-space layout bounds from a detached shared handle."""

    handle = _handle_for(value)
    if not _has_shared_layout_queries(handle):
        return _base._bounds(value._current_raw())
    return (
        _base.Vec2(
            float(handle.criticalX(-1.0, 0.0)),
            float(handle.criticalY(0.0, -1.0)),
        ),
        _base.Vec2(
            float(handle.criticalX(1.0, 0.0)),
            float(handle.criticalY(0.0, 1.0)),
        ),
    )
''',
)
replace_once(
    path,
    '''def _layout_center(value: _base.Mobject) -> _base.Vec2:
    handle = _handle_for(value)
    if handle is None:
        raw = value._current_raw()
        bounds = _base._bounds(raw)
        if bounds is not None:
            return (bounds[0] + bounds[1]) * 0.5
        translation = raw.transform["translation"]
        return _base.Vec2(float(translation["x"]), float(translation["y"]))
    return _base.Vec2(float(handle.centerX), float(handle.centerY))
''',
    '''def _layout_center(value: _base.Mobject) -> _base.Vec2:
    handle = _handle_for(value)
    if not _has_shared_layout_queries(handle):
        raw = value._current_raw()
        bounds = _base._bounds(raw)
        if bounds is not None:
            return (bounds[0] + bounds[1]) * 0.5
        translation = raw.transform["translation"]
        return _base.Vec2(float(translation["x"]), float(translation["y"]))
    return _base.Vec2(float(handle.centerX), float(handle.centerY))
''',
)
replace_once(
    path,
    '''def _width(self: _base.Mobject) -> float:
    handle = _handle_for(self)
    if handle is not None:
        return float(handle.width)
    bounds = _base._bounds(self._current_raw())
    return 0.0 if bounds is None else bounds[1].x - bounds[0].x


def _height(self: _base.Mobject) -> float:
    handle = _handle_for(self)
    if handle is not None:
        return float(handle.height)
    bounds = _base._bounds(self._current_raw())
    return 0.0 if bounds is None else bounds[1].y - bounds[0].y
''',
    '''def _width(self: _base.Mobject) -> float:
    handle = _handle_for(self)
    if _has_shared_layout_queries(handle):
        return float(handle.width)
    bounds = _base._bounds(self._current_raw())
    return 0.0 if bounds is None else bounds[1].x - bounds[0].x


def _height(self: _base.Mobject) -> float:
    handle = _handle_for(self)
    if _has_shared_layout_queries(handle):
        return float(handle.height)
    bounds = _base._bounds(self._current_raw())
    return 0.0 if bounds is None else bounds[1].y - bounds[0].y
''',
)
replace_once(
    path,
    '''def _critical(value: _base.Mobject, direction: _base.Vec2) -> _base.Vec2:
    handle = _handle_for(value)
    if handle is not None:
        return _base.Vec2(
            float(handle.criticalX(direction.x, direction.y)),
            float(handle.criticalY(direction.x, direction.y)),
        )
    return _base._critical(value._current_raw(), direction)
''',
    '''def _critical(value: _base.Mobject, direction: _base.Vec2) -> _base.Vec2:
    handle = _handle_for(value)
    if _has_shared_layout_queries(handle):
        return _base.Vec2(
            float(handle.criticalX(direction.x, direction.y)),
            float(handle.criticalY(direction.x, direction.y)),
        )
    return _base._critical(value._current_raw(), direction)
''',
)
replace_once(
    path,
    '''    if handle is None:
        return _ORIGINAL_ALIGN_ON_FRAME(self, direction, buff)
    handle.alignOnFrame(direction.x, direction.y, float(buff))
    return self
''',
    '''    if handle is None or not hasattr(handle, "alignOnFrame"):
        return _ORIGINAL_ALIGN_ON_FRAME(self, direction, buff)
    handle.alignOnFrame(direction.x, direction.y, float(buff))
    return self
''',
)

path = "compat/semantic-ownership-v1.json"
replace_once(
    path,
    '''    {
      "id": "mobject.layout-query",
      "surface": "Mobject.center/width/height/critical_point",
      "classification": "python-semantic-duplicate",
      "owner": {"language": "python", "path": "web/python/_manim_semantic_handles.py", "symbol": "_layout_bounds/_layout_center/_width/_height/_critical"},
      "shared_owner": {"language": "rust", "path": "crates/noon-web/src/authoring_mobject.rs", "symbol": "FrontendMobjectHandle::center/width/height/critical_point"},
      "reason": "Python materializes handle snapshots and evaluates the compatibility bounds contract locally instead of using the shared query result.",
      "replacement": "Finish #62 bounds integration and route these queries directly to shared semantic handles.",
      "migration_issue": "#61"
    },
''',
    '''    {
      "id": "mobject.layout-query",
      "surface": "Detached and animate-target Mobject.center/width/height/critical_point",
      "classification": "shared-rust",
      "owner": {"language": "rust", "path": "crates/noon-web/src/authoring_mobject.rs", "symbol": "FrontendMobjectHandle::center/width/height/critical_point"},
      "adapters": [{"language": "python", "path": "web/python/_manim_semantic_handles.py", "symbol": "_layout_bounds/_layout_center/_width/_height/_critical"}]
    },
    {
      "id": "mobject.scene-layout-query",
      "surface": "Scene-bound Mobject.center/width/height/critical_point",
      "classification": "python-semantic-duplicate",
      "owner": {"language": "python", "path": "web/python/_manim_semantic_handles.py", "symbol": "_layout_bounds/_layout_center/_width/_height/_critical fallbacks"},
      "shared_owner": {"language": "rust", "path": "crates/noon-web/src/authoring_mobject.rs", "symbol": "FrontendMobjectHandle::center/width/height/critical_point"},
      "reason": "Scene-bound wrappers do not yet retain a stable shared semantic handle, so layout queries still evaluate their scene snapshot in Python.",
      "replacement": "Bind scene-owned wrappers to stable shared handles and reuse the same Rust layout query path.",
      "migration_issue": "#61"
    },
''',
)
