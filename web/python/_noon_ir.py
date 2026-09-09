"""Inert Python values, argument coercions, and detached geometry fallback.

No Scene, timeline, or execution lives here. The remaining detached geometry
snapshot adapter is deletion-owned by #61.
"""

from __future__ import annotations

import copy
import math
from dataclasses import dataclass
from typing import Any



def _finite_number(name: str, value: Any) -> float:
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise TypeError(f"{name} must be a number")
    result = float(value)
    if not math.isfinite(result):
        raise ValueError(f"{name} must be finite")
    return result




def _vec2(name: str, value: tuple[float, float]) -> dict[str, float]:
    if not isinstance(value, (tuple, list)) or len(value) != 2:
        raise ValueError(f"{name} must contain two values")
    return {
        "x": _finite_number(f"{name}.x", value[0]),
        "y": _finite_number(f"{name}.y", value[1]),
    }


def _positive_number(name: str, value: Any) -> float:
    result = _finite_number(name, value)
    if result <= 0.0:
        raise ValueError(f"{name} must be positive")
    return result


def _unit_interval(name: str, value: Any) -> float:
    result = _finite_number(name, value)
    if not 0.0 <= result <= 1.0:
        raise ValueError(f"{name} must be between 0 and 1")
    return result


def _stroke_join(value: Any) -> str:
    if not isinstance(value, str):
        raise TypeError("stroke_join must be a string")
    if value not in {"round", "miter", "bevel"}:
        raise ValueError("stroke_join must be round, miter, or bevel")
    return value


def _stroke_cap(value: Any) -> str:
    if not isinstance(value, str):
        raise TypeError("stroke_cap must be a string")
    if value not in {"round", "butt", "square"}:
        raise ValueError("stroke_cap must be round, butt, or square")
    return value


def _authoring_key(name: str, value: str | None, fallback: str) -> str:
    if value is None:
        return fallback
    if not isinstance(value, str):
        raise TypeError(f"{name} must be a string")
    if not value.strip():
        raise ValueError(f"{name} must not be empty")
    return value








@dataclass(frozen=True, slots=True)
class Color:
    red: float
    green: float
    blue: float
    alpha: float = 1.0

    def __post_init__(self) -> None:
        for component in ("red", "green", "blue", "alpha"):
            object.__setattr__(
                self,
                component,
                _finite_number(component, getattr(self, component)),
            )

    def to_ir(self) -> dict[str, float]:
        return {
            "red": self.red,
            "green": self.green,
            "blue": self.blue,
            "alpha": self.alpha,
        }


@dataclass(frozen=True, slots=True)
class Mobject:
    """Detached semantic object snapshot usable as a Transform target."""

    geometry: dict[str, Any]
    transform: dict[str, Any]
    style: dict[str, Any]

    def to_ir(self) -> dict[str, Any]:
        return {
            "geometry": copy.deepcopy(self.geometry),
            "transform": copy.deepcopy(self.transform),
            "style": copy.deepcopy(self.style),
        }


@dataclass(frozen=True, slots=True)
class Object:
    """Stable reference to an object owned by one Scene."""

    id: int
    _owner: object


class VectorPath:
    """Renderer-independent vector path command builder."""

    def __init__(self) -> None:
        self._commands: list[Any] = []

    def move_to(self, to: tuple[float, float]) -> VectorPath:
        self._commands.append({"move_to": {"to": _vec2("to", to)}})
        return self

    def line_to(self, to: tuple[float, float]) -> VectorPath:
        self._commands.append({"line_to": {"to": _vec2("to", to)}})
        return self

    def quadratic_to(
        self, control: tuple[float, float], to: tuple[float, float]
    ) -> VectorPath:
        self._commands.append(
            {
                "quadratic_to": {
                    "control": _vec2("control", control),
                    "to": _vec2("to", to),
                }
            }
        )
        return self

    def cubic_to(
        self,
        control1: tuple[float, float],
        control2: tuple[float, float],
        to: tuple[float, float],
    ) -> VectorPath:
        self._commands.append(
            {
                "cubic_to": {
                    "control1": _vec2("control1", control1),
                    "control2": _vec2("control2", control2),
                    "to": _vec2("to", to),
                }
            }
        )
        return self

    def close(self) -> VectorPath:
        self._commands.append("close")
        return self

    def to_ir(self) -> dict[str, Any]:
        return {"commands": list(self._commands)}


def _stroke_width_mode(value: object) -> str:
    if value not in {"scale_with_object", "screen_space"}:
        raise ValueError("stroke_width_mode must be scale_with_object or screen_space")
    return str(value)


def _make_mobject(
    geometry: dict[str, Any],
    *,
    position: tuple[float, float] = (0.0, 0.0),
    rotation: float = 0.0,
    scale: tuple[float, float] = (1.0, 1.0),
    fill: Color | None = Color(1.0, 1.0, 1.0),
    stroke: Color | None = None,
    stroke_width: float = 1.0,
    stroke_width_mode: str = "scale_with_object",
    stroke_join: str = "round",
    stroke_cap: str = "round",
    opacity: float = 1.0,
) -> Mobject:
    if fill is not None and not isinstance(fill, Color):
        raise TypeError("fill must be a Color or None")
    if stroke is not None and not isinstance(stroke, Color):
        raise TypeError("stroke must be a Color or None")
    width = _finite_number("stroke_width", stroke_width)
    if width < 0.0:
        raise ValueError("stroke_width must be non-negative")
    return Mobject(
        geometry=copy.deepcopy(geometry),
        transform={
            "translation": _vec2("position", position),
            "rotation": _finite_number("rotation", rotation),
            "scale": _vec2("scale", scale),
        },
        style={
            "fill": None if fill is None else fill.to_ir(),
            "stroke": None if stroke is None else stroke.to_ir(),
            "stroke_width": width,
            "stroke_width_mode": _stroke_width_mode(stroke_width_mode),
            "stroke_join": _stroke_join(stroke_join),
            "stroke_cap": _stroke_cap(stroke_cap),
            "opacity": _finite_number("opacity", opacity),
        },
    )


def Circle(radius: float, **kwargs: Any) -> Mobject:
    return _make_mobject(
        {"circle": {"radius": _positive_number("radius", radius)}},
        **kwargs,
    )


def Rectangle(width: float, height: float, **kwargs: Any) -> Mobject:
    return _make_mobject(
        {
            "rectangle": {
                "size": {
                    "x": _positive_number("width", width),
                    "y": _positive_number("height", height),
                }
            }
        },
        **kwargs,
    )


def Line(
    start: tuple[float, float],
    end: tuple[float, float],
    **kwargs: Any,
) -> Mobject:
    kwargs.setdefault("fill", None)
    kwargs.setdefault("stroke", Color(1.0, 1.0, 1.0))
    kwargs.setdefault("stroke_width", 0.1)
    return _make_mobject(
        {"line": {"start": _vec2("start", start), "end": _vec2("end", end)}},
        **kwargs,
    )


def Path(path: VectorPath, **kwargs: Any) -> Mobject:
    if not isinstance(path, VectorPath):
        raise TypeError("path must be a VectorPath")
    kwargs.setdefault("stroke_width", 0.1)
    return _make_mobject({"vector_path": path.to_ir()}, **kwargs)
