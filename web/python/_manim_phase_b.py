"""Phase-B glue for Manim source compatibility.

Kept separate from the compatibility surface while Phase B is under active development.
"""

from __future__ import annotations

from typing import Any

import noon as _base
import _manim_compat as _compat


class _GenericAnimationBuilder(_compat._CompatAnimationBuilder, _base._AnimationBuilder):
    """Make the generic proxy recognizable by the existing Noon play lowerer."""


# The property installed by _manim_compat resolves this module global at call time,
# so replacing the class preserves the generic proxy while also satisfying the
# low-level Scene.play isinstance check for Noon's animation builder.
_compat._CompatAnimationBuilder = _GenericAnimationBuilder


def _bind_raw(
    scene: _compat.Scene,
    member: _base.Mobject,
    *,
    key: str | None = None,
) -> None:
    """Bind one public wrapper using the canonical low-level Scene emitter."""

    raw_object = _base._ir.Scene.add(scene, member._current_raw(), key=key)
    member._bind(scene, raw_object)


def _scene_add(
    self: _compat.Scene,
    *mobjects: object,
    key: str | None = None,
) -> _base.Mobject | _compat.Scene:
    if not mobjects:
        return self

    leaves = [
        member
        for value in mobjects
        for member in _compat._leaf_mobjects(value)
    ]
    if key is not None and len(leaves) != 1:
        raise ValueError("an explicit key can only be used when adding one Mobject")

    for index, member in enumerate(leaves):
        newly_bound = member._scene is None
        if newly_bound:
            _bind_raw(self, member, key=key if index == 0 else None)
        elif member._scene is not self:
            raise ValueError("Mobject already belongs to another Scene")

        assert member._object is not None
        tracks = self._ensure_lifecycle_timeline_available(
            member._object, self._cursor, "Scene.add target"
        )
        if newly_bound and self._cursor > 0.0:
            self._add_presence_track(
                member._object,
                False,
                True,
                self._cursor,
                key=f"@scene-add:{member._object.id}:{self._cursor:g}",
            )
        elif tracks and not self._presence_at(member._object, self._cursor):
            self._add_presence_track(
                member._object,
                False,
                True,
                self._cursor,
                key=f"@scene-add:{member._object.id}:{self._cursor:g}",
            )

    for value in mobjects:
        self._register_top_level(value)

    # Preserve Noon's established one-object return as a backwards-compatible
    # extension. Typical Manim source ignores Scene.add's return value.
    return leaves[0] if len(leaves) == 1 else self


def _bind_introducer_target(self: _compat.Scene, target: object) -> None:
    if isinstance(target, _compat.Group):
        for member in _compat._leaf_mobjects(target):
            if member._scene is None:
                _bind_raw(self, member)
            elif member._scene is not self:
                raise ValueError("Mobject already belongs to another Scene")
        self._register_top_level(target)
        return

    if isinstance(target, _base.Mobject):
        if target._scene is None:
            _bind_raw(self, target)
        elif target._scene is not self:
            raise ValueError("Mobject already belongs to another Scene")
        self._register_top_level(target)


_compat.Scene.add = _scene_add
_compat.Scene._bind_introducer_target = _bind_introducer_target
