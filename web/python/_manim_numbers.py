"""Thin Manim DecimalNumber wrappers over the typed Rust semantic object."""
from __future__ import annotations
import math
from _noon_errors import engine_call
from _manim_typst import _RetainedTextMobject, _live_text_context
import _manim_semantic_handles as _semantic
import noon as _base

try:
    from js import noonCreateAuthoringDecimalNumberHandle as _create_decimal, noonNumericFromMobject as _from_mobject
except ImportError:
    _create_decimal = _from_mobject = None

class DecimalNumber(_RetainedTextMobject):
    def _rebind_copied_semantic_handle(self):
        if _from_mobject is None: raise RuntimeError("DecimalNumber requires Noon's shared Rust authoring runtime")
        self._numeric_handle = engine_call(_from_mobject, self._semantic_handle)
    def __init__(self, number: float = 0, *, num_decimal_places: int = 2, include_sign: bool = False, group_with_commas: bool = True, show_ellipsis: bool = False, unit: str | None = None, font_size: float = 48.0, color: _base.Color = _base.WHITE, **kwargs):
        opacity = float(kwargs.pop("opacity", 1.0))
        if kwargs: raise NotImplementedError("unsupported DecimalNumber option(s): " + ", ".join(sorted(kwargs)))
        if _create_decimal is None: raise RuntimeError("DecimalNumber requires Noon's shared Rust authoring runtime")
        value = float(number)
        if not math.isfinite(value): raise ValueError("DecimalNumber value must be finite")
        if isinstance(num_decimal_places, bool) or not isinstance(num_decimal_places, int) or not 0 <= num_decimal_places <= 12: raise ValueError("num_decimal_places must be an integer from 0 through 12")
        if unit is not None and not isinstance(unit, str): raise TypeError("unit must be a string or None")
        context = _live_text_context()
        handle = engine_call(_create_decimal, value, num_decimal_places, bool(include_sign), bool(group_with_commas), bool(show_ellipsis), unit, float(font_size), context)
        if context is not None:
            self._canonical_live_target_context = context
        self._numeric_handle = handle
        semantic = engine_call(handle.mobject)
        self._initialize_text(engine_call(handle.text), float(font_size), semantic, color, opacity)
    @property
    def font_size(self) -> float:
        context = _semantic._live_mutation_context(self) if self._scene is not None else None
        return float(engine_call(self._numeric_handle.fontSize, context))
    def get_value(self) -> float: return float(engine_call(self._numeric_handle.value))
    def set_value(self, number: float):
        value = float(number)
        context = _semantic._live_mutation_context(self)
        if context is None: engine_call(self._numeric_handle.setValue, value)
        else: engine_call(self._numeric_handle.setValueLive, context, value)
        self._source = str(engine_call(self._numeric_handle.text)); return self
    def increment_value(self, delta: float):
        value = float(delta)
        context = _semantic._live_mutation_context(self)
        if context is None: engine_call(self._numeric_handle.incrementValue, value)
        else: engine_call(self._numeric_handle.incrementValueLive, context, value)
        self._source = str(engine_call(self._numeric_handle.text)); return self

class Integer(DecimalNumber):
    def __init__(self, number: float = 0, **kwargs): kwargs["num_decimal_places"] = 0; super().__init__(number, **kwargs)
    def get_value(self) -> int: return int(engine_call(self._numeric_handle.integerValue))
