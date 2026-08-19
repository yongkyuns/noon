"""Minimal Noon authoring API for the browser Pyodide worker.

The module only builds versioned IR documents. It does not schedule frames,
touch the canvas, or own runtime state.
"""

from __future__ import annotations

import json
import math
from dataclasses import dataclass
from typing import Any

FORMAT_VERSION = 1


def _finite_number(name: str, value: Any) -> float:
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise TypeError(f"{name} must be a number")
    result = float(value)
    if not math.isfinite(result):
        raise ValueError(f"{name} must be finite")
    return result


def _identifier(name: str, value: Any) -> int:
    if isinstance(value, bool) or not isinstance(value, int):
        raise TypeError(f"{name} must be an integer")
    if value < 0:
        raise ValueError(f"{name} must be non-negative")
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


class PatchBatch:
    """Ordered collection of semantic mutations for a persistent Noon runtime."""

    def __init__(self, sequence: int) -> None:
        self.sequence = _identifier("sequence", sequence)
        self._patches: list[dict[str, Any]] = []

    def set_style(
        self,
        object_id: int,
        *,
        fill: Color | None,
        stroke: Color | None,
        stroke_width: float,
        opacity: float = 1.0,
    ) -> PatchBatch:
        self._patches.append(
            {
                "set_style": {
                    "object": _identifier("object_id", object_id),
                    "style": {
                        "fill": None if fill is None else fill.to_ir(),
                        "stroke": None if stroke is None else stroke.to_ir(),
                        "stroke_width": _finite_number(
                            "stroke_width", stroke_width
                        ),
                        "opacity": _finite_number("opacity", opacity),
                    },
                }
            }
        )
        return self

    def set_transform(
        self,
        object_id: int,
        *,
        translation: tuple[float, float] = (0.0, 0.0),
        rotation: float = 0.0,
        scale: tuple[float, float] = (1.0, 1.0),
    ) -> PatchBatch:
        if len(translation) != 2 or len(scale) != 2:
            raise ValueError("translation and scale must each contain two values")
        self._patches.append(
            {
                "set_transform": {
                    "object": _identifier("object_id", object_id),
                    "transform": {
                        "translation": {
                            "x": _finite_number("translation.x", translation[0]),
                            "y": _finite_number("translation.y", translation[1]),
                        },
                        "rotation": _finite_number("rotation", rotation),
                        "scale": {
                            "x": _finite_number("scale.x", scale[0]),
                            "y": _finite_number("scale.y", scale[1]),
                        },
                    },
                }
            }
        )
        return self

    def to_document(self) -> dict[str, Any]:
        return {
            "version": FORMAT_VERSION,
            "sequence": self.sequence,
            "patches": list(self._patches),
        }

    def to_json(self) -> str:
        return json.dumps(self.to_document(), separators=(",", ":"), allow_nan=False)
