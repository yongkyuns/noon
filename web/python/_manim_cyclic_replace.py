"""ManimCE CyclicReplace/Swap adapter over ordinary deterministic Transforms.

This frontend slice owns only call-shape normalization and detached target preparation.
Each direct member lowers to Noon's already-qualified ordinary Transform path, so curved
translation continues to use the shared Rust ArcVec2 track and renderer-independent
playback. Activation-relative target capture remains owned by roadmap issue #1557.
"""

from __future__ import annotations

import math
from typing import Any

import noon as _base
import _manim_compat as _compat
import _manim_rate_functions as _rate_functions
from _manim_animate import Transform
from _manim_composition import AnimationGroup


def _direct_members(mobjects: tuple[object, ...]) -> tuple[_base.Mobject, ...]:
    """Normalize Manim's variadic and single-Group CyclicReplace call shapes."""

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


class CyclicReplace(AnimationGroup):
    """Move each direct member to the next member's center along one shared arc.

    The current adapter prepares detached targets from the authored state visible when
    the request is constructed, then delegates interpolation and playback to ordinary
    Transform leaves. Issue #1557 retains the activation-relative target-capture work
    needed for exact parity when CyclicReplace follows an earlier child in Succession.
    """

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

        options = dict(kwargs)
        rate_func = options.pop("rate_func", _rate_functions.smooth)
        run_time = options.pop("run_time", None)
        lag_ratio = float(options.pop("lag_ratio", 0.0))
        if options:
            unsupported = ", ".join(sorted(options))
            raise NotImplementedError(
                f"unsupported CyclicReplace option(s): {unsupported}"
            )

        targets = tuple(member.copy() for member in members)
        destinations = (*members[1:], members[0])
        for target, destination in zip(targets, destinations):
            target.move_to(destination)

        transforms = tuple(
            Transform(
                source,
                target,
                path_arc=arc,
                rate_func=_rate_functions.linear,
            )
            for source, target in zip(members, targets)
        )
        super().__init__(
            *transforms,
            run_time=run_time,
            rate_func=rate_func,
            lag_ratio=lag_ratio,
        )

        self.mobjects = members
        self.path_arc = arc


class Swap(CyclicReplace):
    """ManimCE's two-object name for CyclicReplace semantics."""

    pass
