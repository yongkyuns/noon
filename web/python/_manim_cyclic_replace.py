"""ManimCE CyclicReplace/Swap inert requests over shared Rust animation semantics."""

from __future__ import annotations

import math
from typing import Any

import noon as _base
import _manim_compat as _compat


def _direct_members(mobjects: tuple[object, ...]) -> tuple[_base.Mobject, ...]:
    if len(mobjects) == 1 and isinstance(mobjects[0], _compat.Group):
        mobjects = tuple(mobjects[0])
    if len(mobjects) < 2:
        raise ValueError("CyclicReplace requires at least two direct Mobjects")
    members: list[_base.Mobject] = []
    for mobject in mobjects:
        if isinstance(mobject, _compat.Group):
            raise NotImplementedError(
                "CyclicReplace currently supports a flat direct-member family only"
            )
        if not isinstance(mobject, _base.Mobject):
            raise TypeError("CyclicReplace members must be Mobjects")
        members.append(mobject)
    return tuple(members)


class CyclicReplace:
    """Inert cyclic center-translation request; activation semantics remain Rust-owned."""

    def __init__(
        self,
        *mobjects: object,
        path_arc: float = _base.PI / 2.0,
        **kwargs: Any,
    ) -> None:
        members = _direct_members(tuple(mobjects))
        arc = float(path_arc)
        if not math.isfinite(arc):
            raise ValueError("CyclicReplace path_arc must be finite")
        self.mobjects = members
        self.group = mobjects[0] if len(mobjects) == 1 and isinstance(mobjects[0], _compat.Group) else None
        self.path_arc = arc
        self.anim_args = dict(kwargs)
        self.anim_args["path_arc"] = arc


class Swap(CyclicReplace):
    """Two-object ManimCE alias with identical cyclic semantics."""

    pass
