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


@dataclass(frozen=True, slots=True)
class Transform:
    """Transform one scene object toward a target shape.

    The first implementation supports VectorPath targets. Scene.play lowers
    this authoring object into deterministic Noon IR; Python is not used during
    frame playback.
    """

    source: Object
    target: VectorPath
    key: str | None = None


class Scene:
    """Complete, versioned Noon scene document."""

    def __init__(self) -> None:
        self._owner = object()
        self._objects: list[dict[str, Any]] = []
        self._tracks: list[dict[str, Any]] = []
        self._object_keys: dict[int, str] = {}
        self._track_keys: dict[int, str] = {}

    def circle(
        self,
        radius: float,
        *,
        position: tuple[float, float] = (0.0, 0.0),
        rotation: float = 0.0,
        scale: tuple[float, float] = (1.0, 1.0),
        fill: Color | None = Color(1.0, 1.0, 1.0),
        stroke: Color | None = None,
        stroke_width: float = 1.0,
        opacity: float = 1.0,
        key: str | None = None,
    ) -> Object:
        return self._add_object(
            {"circle": {"radius": _positive_number("radius", radius)}},
            position=position,
            rotation=rotation,
            scale=scale,
            fill=fill,
            stroke=stroke,
            stroke_width=stroke_width,
            opacity=opacity,
            key=key,
        )

    def rectangle(
        self,
        width: float,
        height: float,
        *,
        position: tuple[float, float] = (0.0, 0.0),
        rotation: float = 0.0,
        scale: tuple[float, float] = (1.0, 1.0),
        fill: Color | None = Color(1.0, 1.0, 1.0),
        stroke: Color | None = None,
        stroke_width: float = 1.0,
        opacity: float = 1.0,
        key: str | None = None,
    ) -> Object:
        return self._add_object(
            {
                "rectangle": {
                    "size": {
                        "x": _positive_number("width", width),
                        "y": _positive_number("height", height),
                    }
                }
            },
            position=position,
            rotation=rotation,
            scale=scale,
            fill=fill,
            stroke=stroke,
            stroke_width=stroke_width,
            opacity=opacity,
            key=key,
        )

    def line(
        self,
        start: tuple[float, float],
        end: tuple[float, float],
        *,
        position: tuple[float, float] = (0.0, 0.0),
        rotation: float = 0.0,
        scale: tuple[float, float] = (1.0, 1.0),
        stroke: Color | None = Color(1.0, 1.0, 1.0),
        stroke_width: float = 0.1,
        opacity: float = 1.0,
        key: str | None = None,
    ) -> Object:
        return self._add_object(
            {"line": {"start": _vec2("start", start), "end": _vec2("end", end)}},
            position=position,
            rotation=rotation,
            scale=scale,
            fill=None,
            stroke=stroke,
            stroke_width=stroke_width,
            opacity=opacity,
            key=key,
        )

    def path(
        self,
        path: VectorPath,
        *,
        position: tuple[float, float] = (0.0, 0.0),
        rotation: float = 0.0,
        scale: tuple[float, float] = (1.0, 1.0),
        fill: Color | None = Color(1.0, 1.0, 1.0),
        stroke: Color | None = None,
        stroke_width: float = 0.1,
        opacity: float = 1.0,
        key: str | None = None,
    ) -> Object:
        if not isinstance(path, VectorPath):
            raise TypeError("path must be a VectorPath")
        return self._add_object(
            {"vector_path": path.to_ir()},
            position=position,
            rotation=rotation,
            scale=scale,
            fill=fill,
            stroke=stroke,
            stroke_width=stroke_width,
            opacity=opacity,
            key=key,
        )

    def play(
        self,
        *animations: Transform,
        duration: float,
        start_time: float = 0.0,
        easing: str = "linear",
    ) -> Scene:
        if not animations:
            raise ValueError("play requires at least one animation")
        for animation in animations:
            if not isinstance(animation, Transform):
                raise TypeError("unsupported animation; expected Transform")
            self._schedule_transform(
                animation,
                duration=duration,
                start_time=start_time,
                easing=easing,
            )
        return self

    def _schedule_transform(
        self,
        animation: Transform,
        *,
        duration: float,
        start_time: float,
        easing: str,
    ) -> None:
        obj = animation.source
        target = animation.target
        if not isinstance(obj, Object) or obj._owner is not self._owner:
            raise ValueError("transformed object must belong to this Scene")
        if not isinstance(target, VectorPath):
            raise TypeError("Transform target must currently be a VectorPath")
        geometry = self._objects[obj.id]["geometry"]
        source = geometry.get("vector_path")
        if source is None:
            raise ValueError("the current Transform renderer supports vector paths only")
        if "morph_target" in source:
            raise ValueError("a path can currently have one geometric Transform target")
        source["morph_target"] = target.to_ir()
        self._add_scalar_track(
            obj,
            "morph",
            0.0,
            1.0,
            start_time,
            duration,
            easing,
            animation.key,
        )

    def animate_position(
        self,
        obj: Object,
        from_: tuple[float, float],
        to: tuple[float, float],
        *,
        duration: float,
        start_time: float = 0.0,
        easing: str = "linear",
        key: str | None = None,
    ) -> Scene:
        self._add_track(
            obj,
            "position",
            {"vec2": {"from": _vec2("from", from_), "to": _vec2("to", to)}},
            start_time,
            duration,
            easing,
            key,
        )
        return self

    def animate_rotation(
        self,
        obj: Object,
        from_: float,
        to: float,
        *,
        duration: float,
        start_time: float = 0.0,
        easing: str = "linear",
        key: str | None = None,
    ) -> Scene:
        self._add_scalar_track(
            obj, "rotation", from_, to, start_time, duration, easing, key
        )
        return self

    def animate_opacity(
        self,
        obj: Object,
        from_: float,
        to: float,
        *,
        duration: float,
        start_time: float = 0.0,
        easing: str = "linear",
        key: str | None = None,
    ) -> Scene:
        self._add_scalar_track(
            obj, "opacity", from_, to, start_time, duration, easing, key
        )
        return self

    def animate_reveal(
        self,
        obj: Object,
        from_: float = 0.0,
        to: float = 1.0,
        *,
        duration: float,
        start_time: float = 0.0,
        easing: str = "linear",
        key: str | None = None,
    ) -> Scene:
        self._add_scalar_track(
            obj,
            "reveal",
            _unit_interval("from", from_),
            _unit_interval("to", to),
            start_time,
            duration,
            easing,
            key,
        )
        return self

    def animate_morph(
        self,
        obj: Object,
        target: VectorPath,
        *,
        duration: float,
        start_time: float = 0.0,
        easing: str = "linear",
        key: str | None = None,
    ) -> Scene:
        return self.play(
            Transform(obj, target, key=key),
            duration=duration,
            start_time=start_time,
            easing=easing,
        )

    def _add_object(
        self,
        geometry: dict[str, Any],
        *,
        position: tuple[float, float],
        rotation: float,
        scale: tuple[float, float],
        fill: Color | None,
        stroke: Color | None,
        stroke_width: float,
        opacity: float,
        key: str | None,
    ) -> Object:
        if fill is not None and not isinstance(fill, Color):
            raise TypeError("fill must be a Color or None")
        if stroke is not None and not isinstance(stroke, Color):
            raise TypeError("stroke must be a Color or None")
        width = _finite_number("stroke_width", stroke_width)
        if width < 0.0:
            raise ValueError("stroke_width must be non-negative")

        object_id = len(self._objects)
        authoring_key = _authoring_key("key", key, f"@object:{object_id}")
        if authoring_key in self._object_keys.values():
            raise ValueError(f"duplicate object key: {authoring_key}")
        self._object_keys[object_id] = authoring_key
        self._objects.append(
            {
                "id": object_id,
                "geometry": geometry,
                "transform": {
                    "translation": _vec2("position", position),
                    "rotation": _finite_number("rotation", rotation),
                    "scale": _vec2("scale", scale),
                },
                "style": {
                    "fill": None if fill is None else fill.to_ir(),
                    "stroke": None if stroke is None else stroke.to_ir(),
                    "stroke_width": width,
                    "opacity": _finite_number("opacity", opacity),
                },
            }
        )
        return Object(object_id, self._owner)

    def _add_scalar_track(
        self,
        obj: Object,
        property_name: str,
        from_: float,
        to: float,
        start_time: float,
        duration: float,
        easing: str,
        key: str | None,
    ) -> None:
        self._add_track(
            obj,
            property_name,
            {
                "scalar": {
                    "from": _finite_number("from", from_),
                    "to": _finite_number("to", to),
                }
            },
            start_time,
            duration,
            easing,
            key,
        )

    def _add_track(
        self,
        obj: Object,
        property_name: str,
        values: dict[str, Any],
        start_time: float,
        duration: float,
        easing: str,
        key: str | None,
    ) -> None:
        if not isinstance(obj, Object) or obj._owner is not self._owner:
            raise ValueError("animated object must belong to this Scene")
        if easing not in {"linear", "ease_in_out_cubic"}:
            raise ValueError(f"unsupported easing: {easing}")
        start = _finite_number("start_time", start_time)
        if start < 0.0:
            raise ValueError("start_time must be non-negative")
        track_id = len(self._tracks)
        authoring_key = _authoring_key("key", key, f"@track:{track_id}")
        if authoring_key in self._track_keys.values():
            raise ValueError(f"duplicate track key: {authoring_key}")
        self._track_keys[track_id] = authoring_key
        self._tracks.append(
            {
                "id": track_id,
                "object": obj.id,
                "property": property_name,
                "values": values,
                "timing": {
                    "start_time": start,
                    "duration": _positive_number("duration", duration),
                    "easing": easing,
                },
            }
        )

    def to_document(self) -> dict[str, Any]:
        return {
            "version": FORMAT_VERSION,
            "objects": list(self._objects),
            "tracks": list(self._tracks),
        }

    def identity_document(self) -> dict[str, list[dict[str, Any]]]:
        return {
            "objects": [
                {"id": object_id, "key": key}
                for object_id, key in self._object_keys.items()
            ],
            "tracks": [
                {"id": track_id, "key": key}
                for track_id, key in self._track_keys.items()
            ],
        }

    def to_json(self) -> str:
        return json.dumps(self.to_document(), separators=(",", ":"), allow_nan=False)


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