"""Thin Manim DecimalNumber wrappers over the typed Rust semantic object."""
from __future__ import annotations
import math
from _noon_errors import engine_call
from _manim_typst import _RetainedTextMobject, _live_text_context
import _manim_semantic_handles as _semantic
import _manim_compat as _compat
import noon as _base

try:
    from js import noonCreateAuthoringDecimalNumberHandle as _create_decimal, noonNumericFromMobject as _from_mobject
except ImportError:
    _create_decimal = _from_mobject = None
try:
    from js import noonCreateAuthoringVariableHandle as _create_variable
except ImportError:
    _create_variable = None

class DecimalNumber(_RetainedTextMobject):
    @classmethod
    def _from_numeric_handle(cls, handle, context):
        number = object.__new__(cls)
        if context is not None:
            number._canonical_live_target_context = context
        number._numeric_handle = handle
        semantic = engine_call(handle.mobject)
        number._initialize_text(
            str(engine_call(handle.text)),
            float(engine_call(handle.fontSize, _semantic._live_mutation_context(number))),
            semantic,
            _base.WHITE,
            1.0,
        )
        return number
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
    def get_value(self) -> float:
        context = _semantic._live_mutation_context(self) if self._scene is not None else None
        return float(engine_call(self._numeric_handle.value, context))
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
    def get_value(self) -> int:
        context = _semantic._live_mutation_context(self) if self._scene is not None else None
        return int(engine_call(self._numeric_handle.integerValue, context))


class Variable(_compat.VGroup):
    """Shared Rust `label = value` composite driven by one ValueTracker."""

    def __init__(self, var, label, var_type=DecimalNumber, **kwargs):
        from _manim_latex import MathTex
        from _manim_reactive import ValueTracker, _current_authoring_scene
        from _manim_semantic_handles import (
            _attach_shared_family,
            _family_wrapper_key,
            _live_constructor_context,
        )

        if _create_variable is None:
            raise RuntimeError("Variable requires Noon's shared Rust authoring runtime")
        if var_type not in (DecimalNumber, Integer):
            raise NotImplementedError("Variable var_type must be DecimalNumber or Integer")
        if not isinstance(label, str):
            raise TypeError("Variable label must be a string")
        places = kwargs.pop("num_decimal_places", 2)
        if var_type is Integer:
            places = 0
        include_sign = bool(kwargs.pop("include_sign", False))
        commas = bool(kwargs.pop("group_with_commas", True))
        ellipsis = bool(kwargs.pop("show_ellipsis", False))
        unit = kwargs.pop("unit", None)
        font_size = float(kwargs.pop("font_size", 48.0))
        if kwargs:
            raise NotImplementedError(
                "unsupported Variable option(s): " + ", ".join(sorted(kwargs))
            )
        context = _live_constructor_context("Variable", allow_unstarted=True)
        if context is None:
            raise RuntimeError("Variable requires a canonical Scene authoring context")
        handle = engine_call(
            _create_variable, label, float(var), int(places), include_sign,
            commas, ellipsis, unit, font_size, context,
        )
        self._variable_handle = handle
        self.label = MathTex._from_semantic_handle(engine_call(handle.label), context)
        numeric_type = Integer if var_type is Integer else DecimalNumber
        self.value = numeric_type._from_numeric_handle(engine_call(handle.value), context)
        scene = _current_authoring_scene()
        self.tracker = ValueTracker._from_canonical(
            scene, context, engine_call(handle.tracker)
        )
        equals_handle = engine_call(handle.equals)
        self.equals = next(
            part for part in self.label
            if (part._semantic_handle.semanticSlot == equals_handle.semanticSlot
                and part._semantic_handle.semanticGeneration == equals_handle.semanticGeneration)
        )
        _attach_shared_family(self, engine_call(handle.family), context)
        self._semantic_member_wrappers = {
            _family_wrapper_key(self.label): self.label,
            _family_wrapper_key(self.value): self.value,
        }
