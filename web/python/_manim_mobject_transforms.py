"""Thin Manim Mobject transform adapters over shared Noon semantics."""

from __future__ import annotations

from typing import Any

import noon as _base
import _manim_compat as _compat


def _center(self: _base.Mobject) -> _base.Mobject:
    """Pinned ManimCE v0.21 ``Mobject.center`` for Noon's 2D plane.

    Centering is placement syntax, not a second geometry implementation. ``move_to``
    already resolves the object's shared layout center and shifts through the common
    semantic handle path, so keep this adapter as a zero-math composition.
    """

    return self.move_to(_base.ORIGIN)


def _rotate_about_origin(
    self: _base.Mobject,
    angle: float,
    axis: object = _compat.OUT,
    **kwargs: Any,
) -> _base.Mobject:
    """Pinned ManimCE v0.21 ``Mobject.rotate_about_origin`` for Noon's 2D plane.

    Keep the compatibility method as syntax-only composition. ``rotate`` already
    delegates pivoted rotation to the shared Rust/WASM semantic handle, so this
    adapter must not duplicate transform or pivot math in Python.
    """

    return self.rotate(angle, axis=axis, about_point=_base.ORIGIN, **kwargs)


_base.Mobject.center = _center
_compat.Group.center = _center
_base.Mobject.rotate_about_origin = _rotate_about_origin
_compat.Group.rotate_about_origin = _rotate_about_origin
