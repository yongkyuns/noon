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


def _matching_shape_signature(geometry: dict[str, Any]) -> tuple[Any, ...]:
    if "circle" in geometry:
        return ("circle",)
    if "line" in geometry:
        return ("line",)
    if "rectangle" in geometry:
        size = geometry["rectangle"]["size"]
        width = float(size["x"])
        height = float(size["y"])
        ratio = min(width, height) / max(width, height)
        return ("rectangle", round(ratio, 12))
    if "vector_path" in geometry:
        canonical = json.dumps(
            geometry["vector_path"],
            sort_keys=True,
            separators=(",", ":"),
            allow_nan=False,
        )
        return ("vector_path", canonical)
    raise ValueError("matching shapes does not support this geometry")


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


@dataclass(frozen=True, slots=True)
class TransformMatchingShapes:
    """Pair scene objects by deterministic shape signature, then replace them."""

    sources: tuple[Object, ...] | list[Object]
    targets: tuple[Object, ...] | list[Object]
    key: str | None = None


@dataclass(frozen=True, slots=True)
class FadeIn:
    """Make an object present and animate renderer appearance toward fully visible."""

    target: Object
    key: str | None = None


@dataclass(frozen=True, slots=True)
class FadeOut:
    """Animate renderer appearance to zero, then remove the object from the scene."""

    target: Object
    key: str | None = None


class Scene:
    """Complete, versioned Noon scene document."""

    def __init__(self) -> None:
        self._owner = object()
        self._objects: list[dict[str, Any]] = []
        self._tracks: list[dict[str, Any]] = []
        self._object_keys: dict[int, str] = {}
        self._object_key_ids: dict[str, int] = {}
        self._object_positions: dict[int, int] = {}
        self._track_keys: dict[int, str] = {}
        self._next_object_id = 0
        self._next_painter_order = 0
        self._scheduled_transform_targets: dict[int, dict[str, Any]] = {}
        self._scheduled_transform_ends: dict[int, float] = {}
        self._scheduled_fade_ends: dict[int, float] = {}

    def _allocate_object(self, key: str | None = None) -> tuple[Object, int]:
        """Allocate one scene-global object identity and painter slot.

        Content backends may store their payloads in different authoring projections,
        but identity/order are scene concerns and therefore come from this one allocator.
        """
        object_id = self._next_object_id
        painter_order = self._next_painter_order
        authoring_key = _authoring_key("key", key, f"@object:{object_id}")
        if authoring_key in self._object_key_ids:
            raise ValueError(f"duplicate object key: {authoring_key}")
        self._object_keys[object_id] = authoring_key
        self._object_key_ids[authoring_key] = object_id
        self._next_object_id = object_id + 1
        self._next_painter_order += 1
        return Object(object_id, self._owner), painter_order

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
        *animations: Transform
        | ReplacementTransform
        | TransformFromCopy
        | TransformMatchingShapes
        | FadeIn
        | FadeOut,
        duration: float,
        start_time: float = 0.0,
        easing: str = "linear",
    ) -> Scene:
        if not animations:
            raise ValueError("play requires at least one animation")
        checkpoint = self._authoring_checkpoint()
        try:
            for animation in animations:
                if isinstance(animation, FadeIn):
                    self._schedule_fade(
                        animation.target,
                        fade_in=True,
                        key=animation.key,
                        duration=duration,
                        start_time=start_time,
                        easing=easing,
                    )
                elif isinstance(animation, FadeOut):
                    self._schedule_fade(
                        animation.target,
                        fade_in=False,
                        key=animation.key,
                        duration=duration,
                        start_time=start_time,
                        easing=easing,
                    )
                elif isinstance(animation, TransformMatchingShapes):
                    self._schedule_transform_matching_shapes(
                        animation,
                        duration=duration,
                        start_time=start_time,
                        easing=easing,
                    )
                elif isinstance(animation, TransformFromCopy):
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
                        "unsupported animation; expected Transform, ReplacementTransform, "
                        "TransformFromCopy, TransformMatchingShapes, FadeIn, or FadeOut"
                    )
        except Exception:
            self._restore_authoring_checkpoint(checkpoint)
            raise
        return self

    def _authoring_checkpoint(self) -> tuple[Any, ...]:
        return (
            len(self._objects),
            len(self._tracks),
            self._next_object_id,
            self._next_painter_order,
            dict(self._object_keys),
            dict(self._object_key_ids),
            dict(self._object_positions),
            dict(self._track_keys),
            dict(self._scheduled_transform_targets),
            dict(self._scheduled_transform_ends),
            dict(self._scheduled_fade_ends),
        )

    def _restore_authoring_checkpoint(self, checkpoint: tuple[Any, ...]) -> None:
        (
            object_count,
            track_count,
            next_object_id,
            next_painter_order,
            object_keys,
            object_key_ids,
            object_positions,
            track_keys,
            scheduled_transform_targets,
            scheduled_transform_ends,
            scheduled_fade_ends,
        ) = checkpoint
        del self._objects[object_count:]
        del self._tracks[track_count:]
        self._next_object_id = next_object_id
        self._next_painter_order = next_painter_order
        self._object_keys = object_keys
        self._object_key_ids = object_key_ids
        self._object_positions = object_positions
        self._track_keys = track_keys
        self._scheduled_transform_targets = scheduled_transform_targets
        self._scheduled_transform_ends = scheduled_transform_ends
        self._scheduled_fade_ends = scheduled_fade_ends

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

    def _schedule_transform_matching_shapes(
        self,
        animation: TransformMatchingShapes,
        *,
        duration: float,
        start_time: float,
        easing: str,
    ) -> None:
        if not isinstance(animation.sources, (tuple, list)) or not isinstance(
            animation.targets, (tuple, list)
        ):
            raise TypeError("matching sources and targets must be lists or tuples")
        sources = list(animation.sources)
        targets = list(animation.targets)
        if not sources or not targets:
            raise ValueError("matching sources and targets must be non-empty")

        def validate_collection(objects: list[Object], label: str) -> list[int]:
            ids: list[int] = []
            for obj in objects:
                if not isinstance(obj, Object) or obj._owner is not self._owner:
                    raise ValueError(
                        f"matching {label} objects must belong to this Scene"
                    )
                ids.append(obj.id)
            if len(ids) != len(set(ids)):
                raise ValueError(f"matching {label} objects must be unique")
            return ids

        source_ids = validate_collection(sources, "source")
        target_ids = validate_collection(targets, "target")
        if set(source_ids) & set(target_ids):
            raise ValueError("matching sources and targets must be disjoint")

        start = _finite_number("start_time", start_time)
        run_duration = _positive_number("duration", duration)
        end = start + run_duration

        source_signatures: list[tuple[Any, ...]] = []
        for source in sources:
            self._ensure_lifecycle_source_present(source, start, "matching source")
            self._ensure_snapshot_representable(source, end, "matching source")
            self._ensure_replacement_source_unoverridden(source, end)
            try:
                snapshot = self._snapshot_for_object_at(source, start)
            except ValueError as error:
                raise ValueError(
                    f"matching source cannot be snapshotted: {error}"
                ) from error
            source_signatures.append(_matching_shape_signature(snapshot["geometry"]))

        target_signatures: list[tuple[Any, ...]] = []
        for target in targets:
            self._ensure_lifecycle_target_available(target, start, "matching")
            self._ensure_snapshot_representable(target, end, "matching target")
            try:
                snapshot = self._snapshot_for_object_at(target, end)
            except ValueError as error:
                raise ValueError(
                    f"matching target cannot be snapshotted at handoff: {error}"
                ) from error
            target_signatures.append(_matching_shape_signature(snapshot["geometry"]))

        remaining_targets = list(zip(targets, target_signatures))
        pairs: list[tuple[Object, Object]] = []
        for source, signature in zip(sources, source_signatures):
            match_index = next(
                (
                    index
                    for index, (_, target_signature) in enumerate(remaining_targets)
                    if target_signature == signature
                ),
                None,
            )
            if match_index is None:
                raise ValueError(f"unmatched shape for source object {source.id}")
            target, _ = remaining_targets.pop(match_index)
            pairs.append((source, target))
        if remaining_targets:
            target, _ = remaining_targets[0]
            raise ValueError(f"unmatched shape for target object {target.id}")

        source_keys = [self._object_keys[source.id] for source in sources]
        target_keys = [self._object_keys[target.id] for target in targets]
        root_key = _authoring_key(
            "key",
            animation.key,
            f"@matching:{'|'.join(source_keys)}->{'|'.join(target_keys)}",
        )
        pair_keys = [f"{root_key}.match:{index}" for index in range(len(pairs))]
        existing_track_keys = set(self._track_keys.values())
        collision = next((key for key in pair_keys if key in existing_track_keys), None)
        if collision is not None:
            raise ValueError(f"duplicate track key: {collision}")

        for index, (source, target) in enumerate(pairs):
            self._schedule_replacement_transform(
                ReplacementTransform(source, target, key=pair_keys[index]),
                duration=run_duration,
                start_time=start,
                easing=easing,
            )

    def _validate_lifecycle_target(
        self,
        target: Object,
        *,
        start: float,
        end: float,
        label: str,
    ) -> dict[str, Any]:
        self._ensure_lifecycle_target_available(target, start, label)
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

        start = _finite_number("start_time", start_time)
        run_duration = _positive_number("duration", duration)
        end = start + run_duration
        self._ensure_lifecycle_source_present(source, start, "replacement source")
        self._ensure_snapshot_representable(source, end, "replacement source")
        self._ensure_replacement_source_unoverridden(source, end)
        target_snapshot = self._validate_lifecycle_target(
            target, start=start, end=end, label="replacement"
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

        start = _finite_number("start_time", start_time)
        run_duration = _positive_number("duration", duration)
        end = start + run_duration

        self._ensure_lifecycle_source_present(source, start, "copy source")
        self._ensure_snapshot_representable(source, start, "copy source")
        try:
            source_snapshot = self._snapshot_for_object_at(source, start)
        except ValueError as error:
            raise ValueError(f"copy source cannot be snapshotted: {error}") from error
        target_snapshot = self._validate_lifecycle_target(
            target, start=start, end=end, label="copy"
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

    def _schedule_fade(
        self,
        obj: Object,
        *,
        fade_in: bool,
        key: str | None,
        duration: float,
        start_time: float,
        easing: str,
    ) -> None:
        if not isinstance(obj, Object) or obj._owner is not self._owner:
            raise ValueError("faded object must belong to this Scene")
        start = _finite_number("start_time", start_time)
        run_duration = _positive_number("duration", duration)
        end = start + run_duration
        previous_end = self._scheduled_fade_ends.get(obj.id)
        if previous_end is not None and start < previous_end:
            raise ValueError("fade animations for one object must not overlap")

        tracks = self._ensure_lifecycle_timeline_available(obj, start, "fade target")
        if fade_in:
            if tracks and self._presence_at(obj, start):
                raise ValueError("fade-in target must be absent at animation start")
        elif not self._presence_at(obj, start):
            raise ValueError("fade-out target must be present at animation start")

        object_key = self._object_keys[obj.id]
        direction = "in" if fade_in else "out"
        root_key = _authoring_key(
            "key", key, f"@fade-{direction}:{object_key}:{start:g}"
        )
        from_ = self._appearance_at(obj, start)
        to = 1.0 if fade_in else 0.0

        if fade_in:
            self._add_presence_track(
                obj,
                False,
                True,
                start,
                key=f"{root_key}.show",
            )

        self._add_scalar_track(
            obj,
            "appearance",
            _unit_interval("appearance from", from_ if tracks else (0.0 if fade_in else from_)),
            to,
            start,
            run_duration,
            easing,
            root_key,
        )

        if not fade_in:
            self._add_presence_track(
                obj,
                True,
                False,
                end,
                key=f"{root_key}.hide",
            )
        self._scheduled_fade_ends[obj.id] = end

    def _presence_tracks(self, obj: Object) -> list[dict[str, Any]]:
        return sorted(
            (
                track
                for track in self._tracks
                if track["object"] == obj.id and track["property"] == "presence"
            ),
            key=lambda track: (track["timing"]["start_time"], track["id"]),
        )

    def _presence_at(self, obj: Object, time: float) -> bool:
        tracks = self._presence_tracks(obj)
        if not tracks:
            return True
        state = tracks[0]["values"]["bool"]["from"]
        for track in tracks:
            if track["timing"]["start_time"] > time:
                break
            state = track["values"]["bool"]["to"]
        return state

    def _appearance_at(self, obj: Object, time: float) -> float:
        track = self._latest_track_at(obj, "appearance", time)
        if track is None:
            return 1.0
        progress = _track_progress(track["timing"], time)
        values = track["values"]["scalar"]
        return max(0.0, min(1.0, _lerp(values["from"], values["to"], progress)))

    def _ensure_lifecycle_timeline_available(
        self, obj: Object, start: float, label: str
    ) -> list[dict[str, Any]]:
        tracks = self._presence_tracks(obj)
        if tracks and tracks[-1]["timing"]["start_time"] > start:
            raise ValueError(
                f"{label} has a future lifecycle event; lifecycle operations must be authored chronologically"
            )
        return tracks

    def _ensure_lifecycle_source_present(
        self, source: Object, start: float, label: str
    ) -> None:
        self._ensure_lifecycle_timeline_available(source, start, label)
        if not self._presence_at(source, start):
            raise ValueError(f"{label} must be present at animation start")

    def _ensure_lifecycle_target_available(
        self, target: Object, start: float, label: str
    ) -> None:
        tracks = self._ensure_lifecycle_timeline_available(
            target, start, f"{label} target"
        )
        if tracks and self._presence_at(target, start):
            raise ValueError(f"{label} target must be absent before handoff")

    def _add_presence_track(
        self,
        obj: Object,
        from_: bool,
        to: bool,
        time: float,
        *,
        key: str | None = None,
    ) -> None:
        existing = self._presence_tracks(obj)
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

    def animate_appearance(
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
            obj,
            "appearance",
            _unit_interval("from", from_),
            _unit_interval("to", to),
            start_time,
            duration,
            easing,
            key,
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
        if not isinstance(obj, Object) or obj._owner is not self._owner:
            raise ValueError("object must belong to this Scene")
        position = self._object_positions.get(obj.id)
        if position is None:
            raise ValueError(f"object {obj.id} is not geometry-backed")
        stored = self._objects[position]
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
            and track["property"] in {"reveal", "morph"}
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
        obj, _ = self._allocate_object(key)
        stored = copy.deepcopy(snapshot)
        stored["id"] = obj.id
        self._object_positions[obj.id] = len(self._objects)
        self._objects.append(stored)
        return obj

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
        stroke_width_mode: str = "scale_with_object",
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
                    "stroke_width_mode": _stroke_width_mode(stroke_width_mode),
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
                if object_id in self._object_positions
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
        stroke_width_mode: str = "scale_with_object",
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
                        "stroke_width": _finite_number("stroke_width", stroke_width),
                        "stroke_width_mode": _stroke_width_mode(stroke_width_mode),
                        "stroke_join": _stroke_join(stroke_join),
                        "stroke_cap": _stroke_cap(stroke_cap),
                        "opacity": _finite_number("opacity", opacity),
                    },
                }
            }
        )
        return self

    def set_geometry(self, object_id: int, geometry: dict[str, Any]) -> PatchBatch:
        if not isinstance(geometry, dict) or len(geometry) != 1:
            raise TypeError("geometry must be a single-variant Noon geometry dictionary")
        self._patches.append(
            {
                "set_geometry": {
                    "object": _identifier("object_id", object_id),
                    "geometry": geometry,
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
