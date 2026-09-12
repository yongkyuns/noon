"""Thin Manim DashedVMobject adapter backed by shared retained Rust geometry."""

from __future__ import annotations

import operator
from typing import Any

import noon as _base
import _manim_compat as _compat
import _manim_semantic_handles as _shared
from _noon_errors import engine_call


class DashedVMobject(_compat.VMobject):
    """Retained-path Manim DashedVMobject with no Python path reconstruction."""

    def __init__(
        self,
        vmobject: _compat.VMobject,
        num_dashes: int = 15,
        dashed_ratio: float = 0.5,
        dash_offset: float = 0,
        color: _base.Color = _base.WHITE,
        equal_lengths: bool = True,
        **kwargs: Any,
    ) -> None:
        if not isinstance(vmobject, _compat.VMobject):
            raise TypeError("DashedVMobject requires a VMobject source")
        if kwargs:
            names = ", ".join(sorted(kwargs))
            raise NotImplementedError(
                f"DashedVMobject constructor option(s) are not yet shared semantics: {names}"
            )

        count = operator.index(num_dashes)
        if not -(1 << 31) <= count < (1 << 31):
            raise ValueError("num_dashes must fit a signed 32-bit integer")
        ratio = _shared._ir._finite_number("dashed_ratio", dashed_ratio)
        if not 0.0 <= ratio <= 1.0:
            raise ValueError("dashed_ratio must be within [0, 1]")
        offset = _shared._ir._finite_number("dash_offset", dash_offset)
        equal = bool(equal_lengths)

        source = _shared._handle_for(vmobject)
        if source is None:
            raise RuntimeError("DashedVMobject requires retained shared Rust path geometry")
        context = _shared._live_mutation_context(vmobject) or _shared._live_constructor_context(
            "DashedVMobject"
        )
        if context is None:
            result = engine_call(
                source.dashedVmobject,
                count,
                ratio,
                offset,
                equal,
                operation="DashedVMobject",
            )
        else:
            result = engine_call(
                context.liveDashedVMobject,
                source,
                count,
                ratio,
                offset,
                equal,
                operation="DashedVMobject",
            )

        _shared._attach_shared_handle(self, result)
        if context is not None:
            self._canonical_live_target_context = context

        # Pinned Manim finishes by matching the source style, so the constructor
        # color does not become dash geometry authority. Rust copies source style.
        self.num_dashes = count
        self.dashed_ratio = ratio
        self.dash_offset = offset
        self.equal_lengths = equal
        self.color = color
