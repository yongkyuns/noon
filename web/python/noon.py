"""Minimal Noon authoring API for the browser Pyodide worker.

The module only builds versioned IR documents. It does not schedule frames,
touch the canvas, or own runtime state.
"""

from __future__ import annotations

import copy
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


def _track_progress(timing: dict[str, Any], time: float) -> float:
    raw = max(
        0.0,
        min(1.0, (time - timing["start_time"]) / timing["duration"]),
    )
    easing = timing["easing"]
    if easing == "linear":
        return raw
    if easing == "ease_in_out_cubic":
        if raw < 0.5:
            return 4.0 * raw * raw * raw
        return 1.0 - ((-2.0 * raw + 2.0) ** 3) / 2.0
    raise ValueError(f"unsupported easing: {easing}")


def _lerp(from_: float, to: float, progress: float) -> float:
    return from_ + (to - from_) * progress


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


def _make_mobject(
    geometry: dict[str, Any],
    *,
    position: tuple[float, float] = (0.0, 0.0),
    rotation: float = 0.0,
    scale: tuple[float, float] = (1.0, 1.0),
    fill: Color | None = Color(1.0, 1.0, 1.0),
    stroke: Color | None = None,
    stroke_width: float = 1.0,
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


@dataclass(frozen=True, slots=True)
class Transform:
    """Atomically transform one scene object toward a detached target snapshot."""

    source: Object
    target: Mobject | VectorPath
    key: str | None = None


@dataclass(frozen=True, slots=True)
class ReplacementTransform:
    """Transform a source into another stable scene object, then swap presence."""

    source: Object
    target: Object
    key: str | None = None


@dataclass(frozen=True, slots=True)
class TransformFromCopy:
    """Transform a transient copy of source into target while source remains."""

    source: Object
    target: Object
    key: str | None = None


class Scene:
    """Complete, versioned Noon scene document."""

    def __init__(self) -> None:
        self._owner = object()
        self._objects: list[dict[str, Any]] = []
        self._tracks: list[dict[str, Any]] = []
        self._object_keys: dict[int, str] = {}
        self._track_keys: dict[int, str] = {}
        self._scheduled_transform_targets: dict[int, dict[str, Any]] = {}
        self._scheduled_transform_ends: dict[int, float] = {}
        self._lifecycle_objects: set[int] = set()

    def add(self, mobject: Mobject, *, key: str | None = None) -> Object:
        if not isinstance(mobject, Mobject):
            raise TypeError("add expects a detached Mobject")
        return self._append_snapshot(mobject.to_ir(), key)

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
        stroke_join: str = "round",
        stroke_cap: str = "round",
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
            stroke_join=stroke_join,
            stroke_cap=stroke_cap,
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
        stroke_join: str = "round",
        stroke_cap: str = "round",
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
            stroke_join=stroke_join,
            stroke_cap=stroke_cap,
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
        stroke_join: str = "round",
        stroke_cap: str = "round",
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
            stroke_join=stroke_join,
            stroke_cap=stroke_cap,
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
        stroke_join: str = "round",
        stroke_cap: str = "round",
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
            stroke_join=stroke_join,
            stroke_cap=stroke_cap,
            opacity=opacity,
            key=key,
        )

    def play(
        self,
        *animations: Transform | ReplacementTransform | TransformFromCopy,
        duration: float,
        start_time: float = 0.0,
        easing: str = "linear",
    ) -> Scene:
        if not animations:
            raise ValueError("play requires at least one animation")
        checkpoint = self._authoring_checkpoint()
        try:
            for animation in animations:
                if isinstance(animation, TransformFromCopy):
                    self._schedule_transform_from_copy(
                        animation,
                        duration=duration,
                        start_time=start_time,
                        easing=easing,
                    )
                elif isinstance(animation, ReplacementTransform):
                    self._schedule_replacement_transform(
                        animation,
                        duration=duration,
                        start_time=start_time,
                        easing=easing,
                    )
                elif isinstance(animation, Transform):
                    self._schedule_transform(
                        animation,
                        duration=duration,
                        start_time=start_time,
                        easing=easing,
                    )
                else:
                    raise TypeError(
                        "unsupported animation; expected Transform, ReplacementTransform, or TransformFromCopy"
                    )
        except Exception:
            self._restore_authoring_checkpoint(checkpoint)
            raise
        return self

    def _authoring_checkpoint(self) -> tuple[Any, ...]:
        return (
            len(self._objects),
            len(self._tracks),
            dict(self._scheduled_transform_targets),
            dict(self._scheduled_transform_ends),
            set(self._lifecycle_objects),
        )

    def _restore_authoring_checkpoint(self, checkpoint: tuple[Any, ...]) -> None:
        (
            object_count,
            track_count,
            scheduled_transform_targets,
            scheduled_transform_ends,
            lifecycle_objects,
        ) = checkpoint
        for object_id in range(object_count, len(self._objects)):
            self._object_keys.pop(object_id, None)
        for track_id in range(track_count, len(self._tracks)):
            self._track_keys.pop(track_id, None)
        del self._objects[object_count:]
        del self._tracks[track_count:]
        self._scheduled_transform_targets = scheduled_transform_targets
        self._scheduled_transform_ends = scheduled_transform_ends
        self._lifecycle_objects = lifecycle_objects

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

        start = _finite_number("start_time", start_time)
        run_duration = _positive_number("duration", duration)
        previous_end = self._scheduled_transform_ends.get(obj.id)
        if previous_end is not None and start < previous_end:
            raise ValueError("generic Transform tracks for one object must not overlap")

        source_snapshot = self._snapshot_for_object_at(obj, start)
        if isinstance(target, VectorPath):
            target_snapshot = copy.deepcopy(source_snapshot)
            target_snapshot["geometry"] = {"vector_path": target.to_ir()}
        elif isinstance(target, Mobject):
            target_snapshot = target.to_ir()
        else:
            raise TypeError("Transform target must be a detached Mobject or VectorPath")

        self._add_track(
            obj,
            "transform",
            {
                "object": {
                    "from": source_snapshot,
                    "to": target_snapshot,
                }
            },
            start,
            run_duration,
            easing,
            animation.key,
        )
        self._scheduled_transform_targets[obj.id] = copy.deepcopy(target_snapshot)
        self._scheduled_transform_ends[obj.id] = start + run_duration

    def _validate_lifecycle_target(
        self,
        target: Object,
        *,
        end: float,
        label: str,
    ) -> dict[str, Any]:
        self._ensure_snapshot_representable(target, end, f"{label} target")
        try:
            return self._snapshot_for_object_at(target, end)
        except ValueError as error:
            raise ValueError(
                f"{label} target cannot be snapshotted at handoff: {error}"
            ) from error

    def _schedule_replacement_transform(
        self,
        animation: ReplacementTransform,
        *,
        duration: float,
        start_time: float,
        easing: str,
    ) -> None:
        source = animation.source
        target = animation.target
        if not isinstance(source, Object) or source._owner is not self._owner:
            raise ValueError("replacement source must belong to this Scene")
        if not isinstance(target, Object) or target._owner is not self._owner:
            raise ValueError("replacement target must belong to this Scene")
        if source.id == target.id:
            raise ValueError("replacement source and target must be different objects")
        if source.id in self._lifecycle_objects or target.id in self._lifecycle_objects:
            raise ValueError("an object may participate in only one lifecycle replacement")

        start = _finite_number("start_time", start_time)
        run_duration = _positive_number("duration", duration)
        end = start + run_duration
        self._ensure_snapshot_representable(source, end, "replacement source")
        self._ensure_replacement_source_unoverridden(source, end)
        target_snapshot = self._validate_lifecycle_target(
            target, end=end, label="replacement"
        )
        detached_target = Mobject(
            geometry=target_snapshot["geometry"],
            transform=target_snapshot["transform"],
            style=target_snapshot["style"],
        )
        self._schedule_transform(
            Transform(source, detached_target, key=animation.key),
            duration=run_duration,
            start_time=start,
            easing=easing,
        )

        self._add_presence_track(source, True, False, end)
        self._add_presence_track(target, False, True, end)
        self._lifecycle_objects.update((source.id, target.id))

    def _schedule_transform_from_copy(
        self,
        animation: TransformFromCopy,
        *,
        duration: float,
        start_time: float,
        easing: str,
    ) -> None:
        source = animation.source
        target = animation.target
        if not isinstance(source, Object) or source._owner is not self._owner:
            raise ValueError("copy source must belong to this Scene")
        if not isinstance(target, Object) or target._owner is not self._owner:
            raise ValueError("copy target must belong to this Scene")
        if source.id == target.id:
            raise ValueError("copy source and target must be different objects")
        if source.id in self._lifecycle_objects or target.id in self._lifecycle_objects:
            raise ValueError("an object may participate in only one lifecycle animation")

        start = _finite_number("start_time", start_time)
        run_duration = _positive_number("duration", duration)
        end = start + run_duration

        self._ensure_snapshot_representable(source, start, "copy source")
        try:
            source_snapshot = self._snapshot_for_object_at(source, start)
        except ValueError as error:
            raise ValueError(f"copy source cannot be snapshotted: {error}") from error
        target_snapshot = self._validate_lifecycle_target(
            target, end=end, label="copy"
        )

        source_key = self._object_keys[source.id]
        target_key = self._object_keys[target.id]
        copy_key = (
            f"{animation.key}.copy"
            if animation.key is not None
            else f"@copy:{source_key}->{target_key}"
        )
        copy_object = self._append_snapshot(source_snapshot, copy_key)
        transform_key = (
            animation.key if animation.key is not None else f"{copy_key}.transform"
        )
        detached_target = Mobject(
            geometry=target_snapshot["geometry"],
            transform=target_snapshot["transform"],
            style=target_snapshot["style"],
        )
        self._schedule_transform(
            Transform(copy_object, detached_target, key=transform_key),
            duration=run_duration,
            start_time=start,
            easing=easing,
        )

        self._add_presence_track(
            copy_object,
            False,
            True,
            start,
            key=f"{copy_key}.show",
        )
        self._add_presence_track(
            copy_object,
            True,
            False,
            end,
            key=f"{copy_key}.hide",
        )
        self._add_presence_track(
            target,
            False,
            True,
            end,
            key=f"{copy_key}.target-show",
        )
        self._lifecycle_objects.update((source.id, target.id, copy_object.id))

    def _add_presence_track(
        self,
        obj: Object,
        from_: bool,
        to: bool,
        time: float,
        *,
        key: str | None = None,
    ) -> None:
        existing = [
            track
            for track in self._tracks
            if track["object"] == obj.id and track["property"] == "presence"
        ]
        if existing:
            previous = existing[-1]
            previous_time = previous["timing"]["start_time"]
            if time < previous_time:
                raise ValueError("presence events must be scheduled in chronological order")
            if previous["values"]["bool"]["to"] is not from_:
                raise ValueError("presence event chain must be continuous")
        self._add_track(
            obj,
            "presence",
            {"bool": {"from": from_, "to": to}},
            time,
            0.0,
            "linear",
            key,
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

    def _snapshot_for_object(self, obj: Object) -> dict[str, Any]:
        stored = self._objects[obj.id]
        return {
            "geometry": copy.deepcopy(stored["geometry"]),
            "transform": copy.deepcopy(stored["transform"]),
            "style": copy.deepcopy(stored["style"]),
        }

    def _latest_track_at(
        self, obj: Object, property_name: str, time: float
    ) -> dict[str, Any] | None:
        candidates = [
            track
            for track in self._tracks
            if track["object"] == obj.id
            and track["property"] == property_name
            and track["timing"]["start_time"] <= time
        ]
        if not candidates:
            return None
        return max(
            candidates,
            key=lambda track: (track["timing"]["start_time"], track["id"]),
        )

    def _snapshot_for_object_at(self, obj: Object, time: float) -> dict[str, Any]:
        snapshot = self._snapshot_for_object(obj)
        transform_track = self._latest_track_at(obj, "transform", time)
        if transform_track is not None:
            timing = transform_track["timing"]
            if time < timing["start_time"] + timing["duration"]:
                raise ValueError("object is inside an active generic Transform")
            snapshot = copy.deepcopy(transform_track["values"]["object"]["to"])

        position_track = self._latest_track_at(obj, "position", time)
        if position_track is not None:
            progress = _track_progress(position_track["timing"], time)
            values = position_track["values"]["vec2"]
            snapshot["transform"]["translation"] = {
                "x": _lerp(values["from"]["x"], values["to"]["x"], progress),
                "y": _lerp(values["from"]["y"], values["to"]["y"], progress),
            }

        rotation_track = self._latest_track_at(obj, "rotation", time)
        if rotation_track is not None:
            progress = _track_progress(rotation_track["timing"], time)
            values = rotation_track["values"]["scalar"]
            snapshot["transform"]["rotation"] = _lerp(
                values["from"], values["to"], progress
            )

        opacity_track = self._latest_track_at(obj, "opacity", time)
        if opacity_track is not None:
            progress = _track_progress(opacity_track["timing"], time)
            values = opacity_track["values"]["scalar"]
            snapshot["style"]["opacity"] = _lerp(
                values["from"], values["to"], progress
            )

        return snapshot

    def _ensure_snapshot_representable(
        self, obj: Object, time: float, label: str
    ) -> None:
        unsupported = [
            track["property"]
            for track in self._tracks
            if track["object"] == obj.id
            and track["property"] in {"presence", "reveal", "morph"}
            and track["timing"]["start_time"] <= time
        ]
        if unsupported:
            properties = ", ".join(sorted(set(unsupported)))
            raise ValueError(
                f"{label} has state not represented by ObjectSnapshot: {properties}"
            )

    def _ensure_replacement_source_unoverridden(
        self, source: Object, end: float
    ) -> None:
        blocking = [
            track["property"]
            for track in self._tracks
            if track["object"] == source.id
            and track["property"] in {"position", "rotation", "opacity"}
            and track["timing"]["start_time"] < end
        ]
        if blocking:
            properties = ", ".join(sorted(set(blocking)))
            raise ValueError(
                "replacement source has narrow-property state that overrides "
                f"Transform before handoff: {properties}"
            )

    def _append_snapshot(
        self, snapshot: dict[str, Any], key: str | None
    ) -> Object:
        object_id = len(self._objects)
        authoring_key = _authoring_key("key", key, f"@object:{object_id}")
        if authoring_key in self._object_keys.values():
            raise ValueError(f"duplicate object key: {authoring_key}")
        self._object_keys[object_id] = authoring_key
        stored = copy.deepcopy(snapshot)
        stored["id"] = object_id
        self._objects.append(stored)
        return Object(object_id, self._owner)

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
        stroke_join: str,
        stroke_cap: str,
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

        return self._append_snapshot(
            {
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
                    "stroke_join": _stroke_join(stroke_join),
                    "stroke_cap": _stroke_cap(stroke_cap),
                    "opacity": _finite_number("opacity", opacity),
                },
            },
            key,
        )

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
        run_duration = _finite_number("duration", duration)
        if property_name == "presence":
            if run_duration != 0.0:
                raise ValueError("presence events require zero duration")
        elif run_duration <= 0.0:
            raise ValueError("duration must be positive")
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
                    "duration": run_duration,
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
        stroke_join: str = "round",
        stroke_cap: str = "round",
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
                        "stroke_join": _stroke_join(stroke_join),
                        "stroke_cap": _stroke_cap(stroke_cap),
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
