from pathlib import Path


def replace_once(path: str, old: str, new: str, label: str) -> None:
    p = Path(path)
    text = p.read_text()
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected one match, found {count}")
    p.write_text(text.replace(old, new, 1))


# _noon_ir.Mobject snapshots are frozen dataclasses. Build a replacement snapshot
# instead of mutating the copied instance in place; preserve source style exactly.
replace_once(
    "web/python/_manim_geometry.py",
    """    raw = _base._raw_mobject(source)\n    raw.geometry = copy.deepcopy(target.geometry)\n    # Manim stores transformed VMobject points directly. Noon separates affine placement,\n    # so copying the target point state also copies its affine placement while source style\n    # (notably MovingDots' red line color) remains untouched.\n    raw.transform = copy.deepcopy(target.transform)\n    return self._apply(raw)\n""",
    """    # Manim stores transformed VMobject points directly. Noon separates affine placement,\n    # so copying the target point state also copies its affine placement while source style\n    # (notably MovingDots' red line color) remains untouched. _ir snapshots are immutable,\n    # therefore construct one replacement value rather than mutating a frozen dataclass.\n    raw = _base._ir.Mobject(\n        geometry=copy.deepcopy(target.geometry),\n        transform=copy.deepcopy(target.transform),\n        style=copy.deepcopy(source.style),\n    )\n    return self._apply(raw)\n""",
    "immutable match_points snapshot",
)


# The Manim facade subclasses noon.Scene, whose high-level add() now accepts Noon
# Mobject handles. The compatibility layer already flattened/validated its own Manim
# handles, so bind the detached raw snapshot through the canonical low-level Scene
# document API rather than recursively entering the newer high-level add() contract.
replace_once(
    "web/python/_manim_compat.py",
    """                raw_object = super().add(\n                    member._current_raw(), key=key if index == 0 else None\n                )\n""",
    """                raw_object = _ir.Scene.add(\n                    self, member._current_raw(), key=key if index == 0 else None\n                )\n""",
    "compat Scene.add raw binding",
)
