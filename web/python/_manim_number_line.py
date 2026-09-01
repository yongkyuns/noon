"""ManimCE v0.21 NumberLine and UnitInterval over shared retained semantics."""

from __future__ import annotations

import json
import math
from typing import Any, Sequence

import noon as _base
import _manim_axes as _axes
import _manim_compat as _compat

_INSTALLED = False
_ORIGINAL_AXES_INIT = _axes.Axes.__init__
_ORIGINAL_NUMBER_LINE_FAMILY = _axes._NumberLineFamily
_NUMBER_LINE_FACTORY = None


def _factory():
    global _NUMBER_LINE_FACTORY
    if _NUMBER_LINE_FACTORY is not None:
        return _NUMBER_LINE_FACTORY
    _axes._require_shared_axes()
    assert _axes._create_axes_plan is not None
    request = {
        "x_range": [-1.0, 1.0, 1.0],
        "y_range": [-1.0, 1.0, 1.0],
        "x_length": 2.0,
        "y_length": 2.0,
        "tips": False,
        "axis_config": {"include_ticks": False},
        "x_axis_config": {},
        "y_axis_config": {},
    }
    _NUMBER_LINE_FACTORY = _axes._create_axes_plan(
        json.dumps(request, separators=(",", ":"), allow_nan=False)
    )
    if not hasattr(_NUMBER_LINE_FACTORY, "createNumberLinePlan"):
        raise RuntimeError("NumberLine requires Noon's shared NumberLine plan bridge")
    return _NUMBER_LINE_FACTORY


def _range(value: Sequence[float] | None) -> list[float]:
    if value is None:
        radius = float(getattr(_base, "DEFAULT_FRAME_WIDTH", 128.0 / 9.0)) / 2.0
        return [float(round(-radius)), float(round(radius)), 1.0]
    values = [float(component) for component in value]
    if len(values) == 2:
        values.append(1.0)
    if len(values) != 3 or not all(math.isfinite(component) for component in values):
        raise ValueError("NumberLine x_range must contain 2 or 3 finite values")
    if values[1] <= values[0]:
        raise ValueError("NumberLine x_range maximum must be greater than minimum")
    if values[2] <= 0.0:
        raise ValueError("NumberLine x_range step must be positive")
    return values


def _finite_non_negative(name: str, value: object) -> float:
    numeric = float(value)
    if not math.isfinite(numeric) or numeric < 0.0:
        raise ValueError(f"NumberLine {name} must be finite and non-negative")
    return numeric


def _positive_integer(name: str, value: object) -> int:
    numeric = float(value)
    if not math.isfinite(numeric) or numeric <= 0.0 or not numeric.is_integer():
        raise ValueError(f"NumberLine {name} must be a positive integer")
    return int(numeric)


def _query_plan(
    x_range: Sequence[float],
    length: float,
    rotation: float,
):
    request = {
        "x_range": [float(value) for value in x_range],
        "length": float(length),
        "rotation": float(rotation),
        "include_ticks": False,
        "tick_size": 0.1,
        "numbers_with_elongated_ticks": [],
        "longer_tick_multiple": 2,
        "exclude_origin_tick": False,
        "stroke_width": 2.0,
        "color": _axes._rgba(_base.WHITE),
    }
    return _factory().createNumberLinePlan(
        json.dumps(request, separators=(",", ":"), allow_nan=False)
    )


class NumberLine(_ORIGINAL_NUMBER_LINE_FAMILY):
    """Static linear retained NumberLine with scalar ManimCE v0.21 queries."""

    def __init__(
        self,
        x_range: Sequence[float] | dict[str, Any] | None = None,
        length: float | None = None,
        unit_size: float = 1.0,
        include_ticks: bool = True,
        tick_size: float = 0.1,
        numbers_with_elongated_ticks: Sequence[float] | None = None,
        longer_tick_multiple: int = 2,
        exclude_origin_tick: bool = False,
        rotation: float = 0.0,
        stroke_width: float = 2.0,
        include_tip: bool = False,
        tip_width: float | None = None,
        tip_height: float | None = None,
        tip_shape: object = None,
        include_numbers: bool = False,
        font_size: float = 36.0,
        label_direction: object = _base.DOWN,
        label_constructor: object = None,
        scaling: object = None,
        line_to_number_buff: float | None = None,
        decimal_number_config: dict[str, Any] | None = None,
        numbers_to_exclude: Sequence[float] | None = None,
        numbers_to_include: Sequence[float] | None = None,
        **kwargs: Any,
    ) -> None:
        # Internal Axes construction passes the already-lowered wire as the sole
        # positional argument. Public construction always passes a range/None.
        if isinstance(x_range, dict) and "line" in x_range and "ticks" in x_range:
            _ORIGINAL_NUMBER_LINE_FAMILY.__init__(self, x_range)
            self.x_range = None
            self._number_line_plan = None
            self.rotation = 0.0
            return

        del tip_width, tip_height, tip_shape, label_constructor
        if include_tip:
            raise NotImplementedError("NumberLine tips require the retained Arrow tip surface")
        if include_numbers or numbers_to_include is not None:
            raise NotImplementedError("NumberLine numbers require retained MathTex/number labels")
        if scaling is not None:
            raise NotImplementedError("NumberLine nonlinear/logarithmic scaling is not yet supported")
        color = kwargs.pop("color", _base.WHITE)
        if kwargs:
            unsupported = ", ".join(sorted(kwargs))
            raise NotImplementedError(f"unsupported NumberLine option(s): {unsupported}")

        resolved_range = _range(x_range)
        numeric_unit_size = float(unit_size)
        if not math.isfinite(numeric_unit_size) or numeric_unit_size <= 0.0:
            raise ValueError("NumberLine unit_size must be finite and positive")
        requested_length = None if length is None else float(length)
        if requested_length is not None and not math.isfinite(requested_length):
            raise ValueError("NumberLine length must be finite")
        resolved_length = (
            (resolved_range[1] - resolved_range[0]) * numeric_unit_size
            if requested_length is None or requested_length == 0.0
            else requested_length
        )
        if resolved_length <= 0.0:
            raise ValueError("NumberLine length must be positive when specified")
        numeric_rotation = float(rotation)
        if not math.isfinite(numeric_rotation):
            raise ValueError("NumberLine rotation must be finite")
        elongated = (
            []
            if numbers_with_elongated_ticks is None
            else [float(value) for value in numbers_with_elongated_ticks]
        )
        if not all(math.isfinite(value) for value in elongated):
            raise ValueError("NumberLine elongated tick values must be finite")
        request = {
            "x_range": resolved_range,
            "length": resolved_length,
            "rotation": numeric_rotation,
            "include_ticks": bool(include_ticks),
            "tick_size": _finite_non_negative("tick_size", tick_size),
            "numbers_with_elongated_ticks": elongated,
            "longer_tick_multiple": _positive_integer(
                "longer_tick_multiple", longer_tick_multiple
            ),
            "exclude_origin_tick": bool(exclude_origin_tick),
            "stroke_width": _finite_non_negative("stroke_width", stroke_width),
            "color": _axes._rgba(color),
        }
        plan = _factory().createNumberLinePlan(
            json.dumps(request, separators=(",", ":"), allow_nan=False)
        )
        wire = json.loads(str(plan.geometryJson()))
        _ORIGINAL_NUMBER_LINE_FAMILY.__init__(self, wire)

        self.x_range = list(resolved_range)
        self.x_min = resolved_range[0]
        self.x_max = resolved_range[1]
        self.x_step = resolved_range[2]
        self.length = requested_length
        self.unit_size = resolved_length / (resolved_range[1] - resolved_range[0])
        self.include_ticks = bool(include_ticks)
        self.tick_size = float(tick_size)
        self.numbers_with_elongated_ticks = list(elongated)
        self.longer_tick_multiple = int(longer_tick_multiple)
        self.exclude_origin_tick = bool(exclude_origin_tick)
        self.rotation = numeric_rotation
        self.stroke_width = float(stroke_width)
        self.include_tip = False
        self.font_size = float(font_size)
        self.include_numbers = False
        self.label_direction = label_direction
        self.line_to_number_buff = (
            float(getattr(_base, "MED_SMALL_BUFF", 0.25))
            if line_to_number_buff is None
            else float(line_to_number_buff)
        )
        self.decimal_number_config = (
            {"num_decimal_places": self._decimal_places_from_step(resolved_range[2])}
            if decimal_number_config is None
            else dict(decimal_number_config)
        )
        self.numbers_to_exclude = (
            [] if numbers_to_exclude is None else list(numbers_to_exclude)
        )
        self.numbers_to_include = numbers_to_include
        self._number_line_plan = plan

    def _require_plan(self):
        plan = self._number_line_plan
        if plan is None:
            raise RuntimeError("NumberLine is missing its authoritative shared query plan")
        return plan

    def number_to_point(self, number: float) -> _base.Vec2:
        result = json.loads(
            str(
                self._require_plan().numberToPointJson(
                    float(number), self._axis_snapshot_json()
                )
            )
        )
        return _base.Vec2(float(result[0]), float(result[1]))

    n2p = number_to_point

    def point_to_number(self, point: object) -> float:
        value = _compat._as_vec2(point)
        return float(
            self._require_plan().pointToNumber(
                float(value.x), float(value.y), self._axis_snapshot_json()
            )
        )

    p2n = point_to_number

    def get_tick_marks(self) -> _compat.VGroup:
        return self.ticks

    def get_unit_size(self) -> float:
        start = self.get_start()
        end = self.get_end()
        return (end - start).length() / (self.x_max - self.x_min)

    def get_unit_vector(self) -> _base.Vec2:
        delta = self.get_end() - self.get_start()
        length = delta.length()
        if length <= 0.0:
            raise ValueError("NumberLine has degenerate retained geometry")
        return delta * (self.unit_size / length)

    def rotate_about_zero(self, angle: float, axis: object = _compat.OUT, **kwargs: Any):
        return self.rotate_about_number(0.0, angle, axis, **kwargs)

    def rotate_about_number(
        self, number: float, angle: float, axis: object = _compat.OUT, **kwargs: Any
    ):
        return self.rotate(
            float(angle), axis, about_point=self.n2p(float(number)), **kwargs
        )

    def __matmul__(self, other: float) -> _base.Vec2:
        return self.n2p(float(other))

    def __rmatmul__(self, other: object) -> float:
        if isinstance(other, _base.Mobject):
            other = other.get_center()
        return self.p2n(other)

    def copy(self) -> NumberLine:
        clone = object.__new__(type(self))
        clone._line = self._line.copy()
        clone.ticks = self.ticks.copy()
        _compat.VGroup.__init__(clone, clone._line, clone.ticks)
        for name, value in self.__dict__.items():
            if name in {"_semantic_family_handle", "submobjects", "_line", "ticks"}:
                continue
            if name == "_number_line_plan":
                setattr(clone, name, value)
            elif isinstance(value, list):
                setattr(clone, name, list(value))
            elif isinstance(value, dict):
                setattr(clone, name, dict(value))
            else:
                setattr(clone, name, value)
        return clone

    @staticmethod
    def _decimal_places_from_step(step: float) -> int:
        step_str = str(step)
        if "." not in step_str:
            return 0
        return len(step_str.split(".")[-1])


class UnitInterval(NumberLine):
    def __init__(
        self,
        unit_size: float = 10.0,
        numbers_with_elongated_ticks: Sequence[float] | None = None,
        decimal_number_config: dict[str, Any] | None = None,
        **kwargs: Any,
    ) -> None:
        super().__init__(
            x_range=(0.0, 1.0, 0.1),
            unit_size=unit_size,
            numbers_with_elongated_ticks=(
                [0.0, 1.0]
                if numbers_with_elongated_ticks is None
                else numbers_with_elongated_ticks
            ),
            decimal_number_config=(
                {"num_decimal_places": 1}
                if decimal_number_config is None
                else decimal_number_config
            ),
            **kwargs,
        )


def _attach_axis_plan(
    axis: NumberLine,
    x_range: Sequence[float],
    length: float,
    rotation: float,
) -> None:
    axis.x_range = [float(component) for component in x_range]
    axis.x_min, axis.x_max, axis.x_step = axis.x_range
    axis.length = float(length)
    axis.unit_size = float(length) / (axis.x_max - axis.x_min)
    axis.rotation = float(rotation)
    axis._number_line_plan = _query_plan(axis.x_range, length, rotation)


def _axes_init(self: _axes.Axes, *args: Any, **kwargs: Any) -> None:
    # Make the existing Axes constructor resolve its internal wrapper to the public
    # NumberLine class, then attach range-aware query plans to the two retained axes.
    _ORIGINAL_AXES_INIT(self, *args, **kwargs)
    if not isinstance(self.x_axis, NumberLine) or not isinstance(self.y_axis, NumberLine):
        raise RuntimeError("Axes NumberLine wrapper installation did not take effect")
    _attach_axis_plan(self.x_axis, self.x_range, self.x_length, 0.0)
    _attach_axis_plan(self.y_axis, self.y_range, self.y_length, math.pi / 2.0)


def install() -> None:
    global _INSTALLED
    if _INSTALLED:
        return
    _axes._NumberLineFamily = NumberLine
    _axes.Axes.__init__ = _axes_init
    for name, value in {"NumberLine": NumberLine, "UnitInterval": UnitInterval}.items():
        setattr(_base, name, value)
        setattr(_compat, name, value)
        if name not in _base.__all__:
            _base.__all__.append(name)
    _INSTALLED = True
