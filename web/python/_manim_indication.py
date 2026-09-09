"""Thin Manim compatibility request for Noon's shared PassingFlash semantic."""

from __future__ import annotations

import math
from typing import Any

import _manim_compat as _compat
import _manim_semantic_handles as _semantic_handles




class ShowPassingFlash:
    """Show a moving window over one exact analytic Line, then remove it."""

    def __init__(
        self,
        mobject: object,
        time_width: float = 0.1,
        **kwargs: Any,
    ) -> None:
        if isinstance(mobject, _compat.Group):
            raise NotImplementedError(
                "ShowPassingFlash family semantics remain partial"
            )
        if not isinstance(mobject, _compat.VMobject):
            raise TypeError("ShowPassingFlash only works for VMobjects")
        if not _semantic_handles._require_typed_manim_line(mobject):
            raw = mobject._current_raw()
            if "line" not in raw.geometry:
                raise NotImplementedError(
                    "ShowPassingFlash currently qualifies the exact Line subset; "
                    "general VMobject path windows remain partial"
                )
        width = float(time_width)
        if not math.isfinite(width) or width <= 0.0:
            raise NotImplementedError(
                "ShowPassingFlash currently requires a finite positive time_width"
            )
        if bool(kwargs.get("reverse_rate_function", False)):
            raise NotImplementedError(
                "ShowPassingFlash reverse_rate_function=True remains partial"
            )
        if "lag_ratio" in kwargs and not math.isclose(
            float(kwargs["lag_ratio"]), 0.0, abs_tol=1e-15
        ):
            raise NotImplementedError(
                "ShowPassingFlash currently requires lag_ratio=0"
            )
        if "path_arc" in kwargs:
            raise TypeError("ShowPassingFlash does not accept path_arc")
        if not bool(kwargs.pop("introducer", True)):
            raise NotImplementedError("ShowPassingFlash requires introducer=True")
        if not bool(kwargs.pop("remover", True)):
            raise NotImplementedError("ShowPassingFlash requires remover=True")

        self.mobject = mobject
        self.target = mobject
        self.time_width = width
        self.remover = True
        self.introducer = True
        self.anim_args = dict(kwargs)
