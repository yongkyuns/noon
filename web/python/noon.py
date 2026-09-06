"""Public Noon authoring API.

The public surface favors Manim-like semantic vocabulary while ``SceneSpec`` is the
canonical mixed-content producer contract. Legacy geometry and retained-text documents
remain bounded compatibility projections while #367 retires the split authoring path.
"""

from __future__ import annotations

import copy
import json
import math
from dataclasses import dataclass
from typing import Any, Iterable, Iterator

import _noon_ir as _ir

FORMAT_VERSION = _ir.FORMAT_VERSION
VectorPath = _ir.VectorPath
PatchBatch = _ir.PatchBatch
Color = _ir.Color


class Vec2(tuple):
    """Tuple-compatible 2D vector used by every public frontend concept."""

    __slots__ = ()

    def __new__(cls, x: float = 0.0, y: float = 0.0) -> Vec2:
        return tuple.__new__(cls, (float(x), float(y)))

    def __getnewargs__(self) -> tuple[float, float]:
        """Return scalar constructor arguments for tuple/pickle reconstruction."""

        return (self.x, self.y)

    def __copy__(self) -> Vec2:
        """Immutable vectors can be shared by shallow copies."""

        return self

    def __deepcopy__(self, memo: dict[int, Any]) -> Vec2:
        """Immutable vectors can be shared without tuple reconstruction."""

        memo[id(self)] = self
        return self

    @property
    def x(self) -> float:
        return self[0]

    @property
    def y(self) -> float:
        return self[1]

    def __add__(self, other: object) -> Vec2:
        rhs = _as_vec2(other)
        return Vec2(self.x + rhs.x, self.y + rhs.y)

    def __sub__(self, other: object) -> Vec2:
        rhs = _as_vec2(other)
        return Vec2(self.x - rhs.x, self.y - rhs.y)

    def __neg__(self) -> Vec2:
        return Vec2(-self.x, -self.y)

    def __mul__(self, scalar: float) -> Vec2:
        factor = float(scalar)
        return Vec2(self.x * factor, self.y * factor)

    def __rmul__(self, scalar: float) -> Vec2:
        return self * scalar

    def __truediv__(self, scalar: float) -> Vec2:
        divisor = float(scalar)
        if divisor == 0.0:
            raise ZeroDivisionError("cannot divide Vec2 by zero")
        return Vec2(self.x / divisor, self.y / divisor)

    def length(self) -> float:
        return math.hypot(self.x, self.y)

    def normalized(self) -> Vec2:
        length = self.length()
        if length == 0.0:
            raise ValueError("direction must be non-zero")
        return self / length


def _as_vec2(value: object) -> Vec2:
    if isinstance(value, Vec2):
        return value
    if isinstance(value, (tuple, list)) and len(value) == 2:
        return Vec2(value[0], value[1])
    raise TypeError("expected a Vec2 or a two-value tuple/list")


ORIGIN = Vec2(0.0, 0.0)
UP = Vec2(0.0, 1.0)
DOWN = Vec2(0.0, -1.0)
LEFT = Vec2(-1.0, 0.0)
RIGHT = Vec2(1.0, 0.0)
UL = UP + LEFT
UR = UP + RIGHT
DL = DOWN + LEFT
DR = DOWN + RIGHT

PI = math.pi
TAU = math.tau
DEGREES = TAU / 360.0

SMALL_BUFF = 0.1
MED_SMALL_BUFF = 0.25
MED_LARGE_BUFF = 0.5
LARGE_BUFF = 1.0
DEFAULT_MOBJECT_TO_EDGE_BUFFER = MED_LARGE_BUFF
DEFAULT_MOBJECT_TO_MOBJECT_BUFFER = MED_SMALL_BUFF
DEFAULT_FRAME_HEIGHT = 8.0
DEFAULT_FRAME_WIDTH = DEFAULT_FRAME_HEIGHT * 16.0 / 9.0


def _hex_color(value: int) -> Color:
    if isinstance(value, bool) or not isinstance(value, int):
        raise TypeError("hex color must be an integer")
    if value < 0 or value > 0xFFFFFF:
        raise ValueError("hex color must be between 0x000000 and 0xFFFFFF")
    return Color(
        ((value >> 16) & 0xFF) / 255.0,
        ((value >> 8) & 0xFF) / 255.0,
        (value & 0xFF) / 255.0,
    )


def color_from_hex(value: str | int) -> Color:
    """Create a Noon color from ``#RRGGBB`` or ``0xRRGGBB``."""

    if isinstance(value, str):
        text = value.strip()
        if text.startswith("#"):
            text = text[1:]
        if len(text) != 6:
            raise ValueError("hex color string must contain exactly six digits")
        try:
            value = int(text, 16)
        except ValueError as error:
            raise ValueError("invalid hex color string") from error
    return _hex_color(value)


# Manim Community default palette. Base names alias the C shade.
WHITE = _hex_color(0xFFFFFF)
BLACK = _hex_color(0x000000)
BLUE_A = _hex_color(0xC7E9F1)
BLUE_B = _hex_color(0x9CDCEB)
BLUE_C = _hex_color(0x58C4DD)
BLUE_D = _hex_color(0x29ABCA)
BLUE_E = _hex_color(0x236B8E)
BLUE = BLUE_C
TEAL_A = _hex_color(0xACEAD7)
TEAL_B = _hex_color(0x76DDC0)
TEAL_C = _hex_color(0x5CD0B3)
TEAL_D = _hex_color(0x55C1A7)
TEAL_E = _hex_color(0x49A88F)
TEAL = TEAL_C
GREEN_A = _hex_color(0xC9E2AE)
GREEN_B = _hex_color(0xA6CF8C)
GREEN_C = _hex_color(0x83C167)
GREEN_D = _hex_color(0x77B05D)
GREEN_E = _hex_color(0x699C52)
GREEN = GREEN_C
YELLOW_A = _hex_color(0xFFF1B6)
YELLOW_B = _hex_color(0xFFEA94)
YELLOW_C = _hex_color(0xF7D96F)
YELLOW_D = _hex_color(0xF4D345)
YELLOW_E = _hex_color(0xE8C11C)
YELLOW = YELLOW_C
GOLD = _hex_color(0xF0AC5F)
RED_A = _hex_color(0xF7A1A3)
RED_B = _hex_color(0xFF8080)
RED_C = _hex_color(0xFC6255)
RED_D = _hex_color(0xE65A4C)
RED_E = _hex_color(0xCF5044)
RED = RED_C
MAROON = _hex_color(0xC55F73)
PURPLE_A = _hex_color(0xCAA3E8)
PURPLE_B = _hex_color(0xB189C6)
PURPLE_C = _hex_color(0x9A72AC)
PURPLE_D = _hex_color(0x715582)
PURPLE_E = _hex_color(0x644172)
PURPLE = PURPLE_C
ORANGE = _hex_color(0xFF862F)
PINK = _hex_color(0xD147BD)
LIGHT_PINK = _hex_color(0xDC75CD)
GRAY_A = GREY_A = _hex_color(0xDDDDDD)
GRAY_B = GREY_B = _hex_color(0xBBBBBB)
GRAY_C = GREY_C = _hex_color(0x888888)
GRAY_D = GREY_D = _hex_color(0x444444)
GRAY_E = GREY_E = _hex_color(0x222222)
GRAY = GREY = GRAY_C


def _raw_mobject(raw: _ir.Mobject) -> _ir.Mobject:
    return _ir.Mobject(
        geometry=copy.deepcopy(raw.geometry),
        transform=copy.deepcopy(raw.transform),
        style=copy.deepcopy(raw.style),
    )


def _bounds(raw: _ir.Mobject) -> tuple[Vec2, Vec2] | None:
    geometry = raw.geometry
    points: list[Vec2] = []
    if "circle" in geometry:
        radius = float(geometry["circle"]["radius"])
        points = [Vec2(-radius, -radius), Vec2(radius, radius)]
    elif "rectangle" in geometry:
        size = geometry["rectangle"]["size"]
        half = Vec2(float(size["x"]) / 2.0, float(size["y"]) / 2.0)
        points = [-half, half]
    elif "line" in geometry:
        line = geometry["line"]
        points = [
            Vec2(line["start"]["x"], line["start"]["y"]),
            Vec2(line["end"]["x"], line["end"]["y"]),
        ]
    elif "vector_path" in geometry:
        for command in geometry["vector_path"]["commands"]:
            if command == "close":
                continue
            payload = next(iter(command.values()))
            for key in ("to", "control", "control1", "control2"):
                if key in payload:
                    point = payload[key]
                    points.append(Vec2(point["x"], point["y"]))
    if not points:
        return None

    min_x = min(point.x for point in points)
    max_x = max(point.x for point in points)
    min_y = min(point.y for point in points)
    max_y = max(point.y for point in points)
    local_corners = (
        Vec2(min_x, min_y),
        Vec2(min_x, max_y),
        Vec2(max_x, min_y),
        Vec2(max_x, max_y),
    )
    transform = raw.transform
    scale = Vec2(transform["scale"]["x"], transform["scale"]["y"])
    translation = Vec2(
        transform["translation"]["x"], transform["translation"]["y"]
    )
    rotation = float(transform["rotation"])
    sine = math.sin(rotation)
    cosine = math.cos(rotation)

    def world(point: Vec2) -> Vec2:
        x = point.x * scale.x
        y = point.y * scale.y
        return Vec2(
            x * cosine - y * sine + translation.x,
            x * sine + y * cosine + translation.y,
        )

    world_points = [world(point) for point in local_corners]
    return (
        Vec2(
            min(point.x for point in world_points),
            min(point.y for point in world_points),
        ),
        Vec2(
            max(point.x for point in world_points),
            max(point.y for point in world_points),
        ),
    )


def _center(raw: _ir.Mobject) -> Vec2:
    bounds = _bounds(raw)
    if bounds is None:
        translation = raw.transform["translation"]
        return Vec2(translation["x"], translation["y"])
    return (bounds[0] + bounds[1]) * 0.5


def _critical(raw: _ir.Mobject, direction: Vec2) -> Vec2:
    bounds = _bounds(raw)
    if bounds is None:
        return _center(raw)
    minimum, maximum = bounds
    center = (minimum + maximum) * 0.5
    return Vec2(
        minimum.x if direction.x < 0 else maximum.x if direction.x > 0 else center.x,
        minimum.y if direction.y < 0 else maximum.y if direction.y > 0 else center.y,
    )


class Mobject:
    """Thin Python handle around one canonical Noon object snapshot."""

    def __init__(self, raw: _ir.Mobject) -> None:
        self._raw = _raw_mobject(raw)
        self._scene: Scene | None = None
        self._object: _ir.Object | None = None

    @property
    def geometry(self) -> dict[str, Any]:
        return self._current_raw().geometry

    @property
    def transform(self) -> dict[str, Any]:
        return self._current_raw().transform

    @property
    def style(self) -> dict[str, Any]:
        return self._current_raw().style

    @property
    def id(self) -> int:
        if self._object is None:
            raise AttributeError("detached Mobject has no scene object id")
        return self._object.id

    def to_ir(self) -> dict[str, Any]:
        return self._current_raw().to_ir()

    def copy(self) -> Mobject:
        return Mobject(self._current_raw())

    def _bind(self, scene: Scene, obj: _ir.Object) -> None:
        if self._scene is not None and self._scene is not scene:
            raise ValueError("Mobject already belongs to another Scene")
        self._scene = scene
        self._object = obj

    def _bind_to_scene(self, scene: Scene, *, key: str | None = None) -> _ir.Object:
        obj = _ir.Scene.add(scene, self._current_raw(), key=key)
        self._bind(scene, obj)
        return obj

    def _scene_lifecycle_state(
        self, scene: Scene, time: float
    ) -> tuple[bool, bool, bool]:
        if self._scene is not scene or self._object is None:
            raise ValueError("Mobject must belong to this Scene")
        tracks = scene._presence_tracks(self._object)
        has_future = any(float(track["timing"]["start_time"]) > time for track in tracks)
        return bool(tracks), scene._presence_at(self._object, time), has_future

    def _record_scene_presence(
        self,
        scene: Scene,
        from_: bool,
        to: bool,
        time: float,
        *,
        key: str | None = None,
    ) -> None:
        if self._scene is not scene or self._object is None:
            raise ValueError("Mobject must belong to this Scene")
        scene._add_presence_track(self._object, from_, to, time, key=key)

    def _is_present_in_scene(self, scene: Scene, time: float) -> bool:
        if self._scene is not scene or self._object is None:
            return False
        return self._scene_lifecycle_state(scene, time)[1]

    def _current_raw(self) -> _ir.Mobject:
        if self._scene is None or self._object is None:
            return self._raw
        return self._scene._raw_snapshot(self._object)

    def _apply(self, raw: _ir.Mobject) -> Mobject:
        if self._scene is None or self._object is None:
            self._raw = _raw_mobject(raw)
        else:
            self._scene._replace_static_snapshot(self._object, raw)
        return self

    def get_center(self) -> Vec2:
        return _center(self._current_raw())

    @property
    def width(self) -> float:
        bounds = _bounds(self._current_raw())
        return 0.0 if bounds is None else bounds[1].x - bounds[0].x

    @property
    def height(self) -> float:
        bounds = _bounds(self._current_raw())
        return 0.0 if bounds is None else bounds[1].y - bounds[0].y

    def shift(self, direction: Vec2 | tuple[float, float]) -> Mobject:
        raw = _raw_mobject(self._current_raw())
        offset = _as_vec2(direction)
        raw.transform["translation"]["x"] += offset.x
        raw.transform["translation"]["y"] += offset.y
        return self._apply(raw)

    def move_to(self, point: Vec2 | tuple[float, float]) -> Mobject:
        return self.shift(_as_vec2(point) - self.get_center())

    def center(self) -> Mobject:
        return self.move_to(ORIGIN)

    def set_x(self, x: float) -> Mobject:
        center = self.get_center()
        return self.shift(Vec2(float(x) - center.x, 0.0))

    def set_y(self, y: float) -> Mobject:
        center = self.get_center()
        return self.shift(Vec2(0.0, float(y) - center.y))

    def scale(self, factor: float | tuple[float, float]) -> Mobject:
        raw = _raw_mobject(self._current_raw())
        if isinstance(factor, (tuple, list, Vec2)):
            value = _as_vec2(factor)
        else:
            value = Vec2(float(factor), float(factor))
        raw.transform["scale"]["x"] *= value.x
        raw.transform["scale"]["y"] *= value.y
        return self._apply(raw)

    def rotate(self, angle: float) -> Mobject:
        raw = _raw_mobject(self._current_raw())
        raw.transform["rotation"] += float(angle)
        return self._apply(raw)

    def set_color(self, color: Color) -> Mobject:
        raw = _raw_mobject(self._current_raw())
        if raw.style["fill"] is not None:
            raw.style["fill"] = color.to_ir()
        if raw.style["stroke"] is not None:
            raw.style["stroke"] = color.to_ir()
        if raw.style["fill"] is None and raw.style["stroke"] is None:
            raw.style["fill"] = color.to_ir()
        return self._apply(raw)

    def set_fill(self, color: Color | None = None, opacity: float | None = None) -> Mobject:
        raw = _raw_mobject(self._current_raw())
        raw.style["fill"] = None if color is None else color.to_ir()
        if opacity is not None:
            raw.style["opacity"] = float(opacity)
        return self._apply(raw)

    def set_stroke(
        self, color: Color | None = None, width: float | None = None
    ) -> Mobject:
        raw = _raw_mobject(self._current_raw())
        raw.style["stroke"] = None if color is None else color.to_ir()
        if width is not None:
            raw.style["stroke_width"] = float(width)
        return self._apply(raw)

    def set_opacity(self, opacity: float) -> Mobject:
        raw = _raw_mobject(self._current_raw())
        raw.style["opacity"] = float(opacity)
        return self._apply(raw)

    def set_object_opacity(self, opacity: float) -> Mobject:
        """Set Noon's object-composite opacity independently of paint opacity.

        Manim ``VMobject.set_opacity`` controls the enabled fill and stroke paint
        alpha channels. This explicit Noon operation controls the separate opacity
        multiplier applied to the complete object through the shared semantic handle.
        """
        del opacity
        raise NotImplementedError(
            "set_object_opacity requires Noon's shared semantic authoring handle"
        )

    def next_to(
        self,
        other: Mobject | Vec2 | tuple[float, float],
        direction: Vec2 | tuple[float, float] = RIGHT,
        buff: float = DEFAULT_MOBJECT_TO_MOBJECT_BUFFER,
    ) -> Mobject:
        axis = _as_vec2(direction).normalized()
        self_point = _critical(self._current_raw(), -axis)
        if isinstance(other, Mobject):
            target_point = _critical(other._current_raw(), axis)
        else:
            target_point = _as_vec2(other)
        return self.shift(target_point - self_point + axis * float(buff))

    def align_to(
        self,
        other: Mobject,
        direction: Vec2 | tuple[float, float] = ORIGIN,
    ) -> Mobject:
        axis = _as_vec2(direction)
        delta = _critical(other._current_raw(), axis) - _critical(
            self._current_raw(), axis
        )
        return self.shift(
            Vec2(delta.x if axis.x else 0.0, delta.y if axis.y else 0.0)
        )

    def to_edge(
        self,
        edge: Vec2 | tuple[float, float] = LEFT,
        buff: float = DEFAULT_MOBJECT_TO_EDGE_BUFFER,
    ) -> Mobject:
        return self._align_on_frame(_as_vec2(edge), float(buff))

    def to_corner(
        self,
        corner: Vec2 | tuple[float, float] = DL,
        buff: float = DEFAULT_MOBJECT_TO_EDGE_BUFFER,
    ) -> Mobject:
        return self._align_on_frame(_as_vec2(corner), float(buff))

    def _align_on_frame(self, direction: Vec2, buff: float) -> Mobject:
        point = _critical(self._current_raw(), direction)
        target = Vec2(
            math.copysign(DEFAULT_FRAME_WIDTH / 2.0, direction.x)
            if direction.x
            else point.x,
            math.copysign(DEFAULT_FRAME_HEIGHT / 2.0, direction.y)
            if direction.y
            else point.y,
        )
        shift = Vec2(
            target.x - point.x - (direction.x * buff if direction.x else 0.0),
            target.y - point.y - (direction.y * buff if direction.y else 0.0),
        )
        return self.shift(shift)

    @property
    def animate(self) -> _AnimationBuilder:
        return _AnimationBuilder(self)


class Group:
    """Lightweight authoring collection; it does not add runtime hierarchy."""

    def __init__(self, *mobjects: Mobject) -> None:
        if not all(isinstance(mobject, Mobject) for mobject in mobjects):
            raise TypeError("Group members must be Mobjects")
        self.submobjects = list(mobjects)

    def __iter__(self) -> Iterator[Mobject]:
        return iter(self.submobjects)

    def __len__(self) -> int:
        return len(self.submobjects)

    def __getitem__(self, index: int) -> Mobject:
        return self.submobjects[index]

    def add(self, *mobjects: Mobject) -> Group:
        self.submobjects.extend(mobjects)
        return self

    def get_center(self) -> Vec2:
        if not self.submobjects:
            return ORIGIN
        mins: list[Vec2] = []
        maxes: list[Vec2] = []
        for mobject in self.submobjects:
            bounds = _bounds(mobject._current_raw())
            if bounds is not None:
                mins.append(bounds[0])
                maxes.append(bounds[1])
        if not mins:
            return ORIGIN
        return Vec2(
            (min(point.x for point in mins) + max(point.x for point in maxes)) / 2.0,
            (min(point.y for point in mins) + max(point.y for point in maxes)) / 2.0,
        )

    def shift(self, direction: Vec2 | tuple[float, float]) -> Group:
        for mobject in self.submobjects:
            mobject.shift(direction)
        return self

    def arrange(
        self,
        direction: Vec2 | tuple[float, float] = RIGHT,
        buff: float = DEFAULT_MOBJECT_TO_MOBJECT_BUFFER,
        center: bool = True,
    ) -> Group:
        if not self.submobjects:
            return self
        axis = _as_vec2(direction)
        for previous, current in zip(self.submobjects, self.submobjects[1:]):
            current.next_to(previous, axis, buff)
        if center:
            self.shift(-self.get_center())
        return self

    def arrange_in_grid(
        self,
        rows: int | None = None,
        cols: int | None = None,
        buff: float | tuple[float, float] = MED_SMALL_BUFF,
    ) -> Group:
        count = len(self.submobjects)
        if count == 0:
            return self
        if rows is None and cols is None:
            cols = math.ceil(math.sqrt(count))
            rows = math.ceil(count / cols)
        elif rows is None:
            assert cols is not None
            rows = math.ceil(count / cols)
        elif cols is None:
            cols = math.ceil(count / rows)
        if rows <= 0 or cols <= 0:
            raise ValueError("rows and cols must be positive")
        if isinstance(buff, (tuple, list, Vec2)):
            gap = _as_vec2(buff)
        else:
            gap = Vec2(float(buff), float(buff))
        cell_width = max((mobject.width for mobject in self.submobjects), default=0.0) + gap.x
        cell_height = max((mobject.height for mobject in self.submobjects), default=0.0) + gap.y
        for index, mobject in enumerate(self.submobjects):
            row = index // cols
            col = index % cols
            x = (col - (cols - 1) / 2.0) * cell_width
            y = ((rows - 1) / 2.0 - row) * cell_height
            mobject.move_to(Vec2(x, y))
        return self


class VGroup(Group):
    pass


def _wrap(raw: _ir.Mobject) -> Mobject:
    return Mobject(raw)


def Circle(radius: float = 1.0, *, color: Color | None = None, **kwargs: Any) -> Mobject:
    result = _wrap(_ir.Circle(radius, **kwargs))
    return result if color is None else result.set_color(color)


def Rectangle(
    width: float = 2.0,
    height: float = 1.0,
    *,
    color: Color | None = None,
    **kwargs: Any,
) -> Mobject:
    result = _wrap(_ir.Rectangle(width, height, **kwargs))
    return result if color is None else result.set_color(color)


def Square(
    side_length: float = 2.0, *, color: Color | None = None, **kwargs: Any
) -> Mobject:
    return Rectangle(side_length, side_length, color=color, **kwargs)


def Line(
    start: Vec2 | tuple[float, float] = LEFT,
    end: Vec2 | tuple[float, float] = RIGHT,
    *,
    color: Color | None = None,
    **kwargs: Any,
) -> Mobject:
    result = _wrap(_ir.Line(_as_vec2(start), _as_vec2(end), **kwargs))
    return result if color is None else result.set_color(color)


def Path(path: VectorPath, *, color: Color | None = None, **kwargs: Any) -> Mobject:
    result = _wrap(_ir.Path(path, **kwargs))
    return result if color is None else result.set_color(color)


@dataclass(frozen=True, slots=True)
class Transform:
    source: Mobject | _ir.Object
    target: Mobject | _ir.Mobject | VectorPath
    key: str | None = None


@dataclass(frozen=True, slots=True)
class ReplacementTransform:
    source: Mobject | _ir.Object
    target: Mobject | _ir.Object
    key: str | None = None


@dataclass(frozen=True, slots=True)
class TransformFromCopy:
    source: Mobject | _ir.Object
    target: Mobject | _ir.Object
    key: str | None = None


@dataclass(frozen=True, slots=True)
class TransformMatchingShapes:
    sources: Iterable[Mobject | _ir.Object]
    targets: Iterable[Mobject | _ir.Object]
    key: str | None = None


@dataclass(frozen=True, slots=True)
class Create:
    """Progressively draw a shape without changing its steady-state geometry."""

    target: Mobject | _ir.Object
    key: str | None = None


@dataclass(frozen=True, slots=True)
class Uncreate(Create):
    """Manim-style Create in reverse, optionally removing the target at completion."""

    reverse_rate_function: bool = True
    remover: bool = True


@dataclass(frozen=True, slots=True)
class FadeIn:
    target: Mobject | _ir.Object
    key: str | None = None


@dataclass(frozen=True, slots=True)
class FadeOut:
    target: Mobject | _ir.Object
    key: str | None = None


class _AnimationBuilder:
    """Transient target-state builder used by ``mobject.animate``."""

    def __init__(self, source: Mobject) -> None:
        if source._scene is None or source._object is None:
            raise ValueError("animate requires a Mobject that belongs to a Scene")
        self.source = source
        self.target = source.copy()

    def shift(self, direction: Vec2 | tuple[float, float]) -> _AnimationBuilder:
        self.target.shift(direction)
        return self

    def move_to(self, point: Vec2 | tuple[float, float]) -> _AnimationBuilder:
        self.target.move_to(point)
        return self

    def scale(self, factor: float | tuple[float, float]) -> _AnimationBuilder:
        self.target.scale(factor)
        return self

    def rotate(self, angle: float) -> _AnimationBuilder:
        self.target.rotate(angle)
        return self

    def set_color(self, color: Color) -> _AnimationBuilder:
        self.target.set_color(color)
        return self

    def set_fill(
        self, color: Color | None = None, opacity: float | None = None
    ) -> _AnimationBuilder:
        self.target.set_fill(color, opacity)
        return self

    def set_stroke(
        self, color: Color | None = None, width: float | None = None
    ) -> _AnimationBuilder:
        self.target.set_stroke(color, width)
        return self

    def set_opacity(self, opacity: float) -> _AnimationBuilder:
        self.target.set_opacity(opacity)
        return self

    def set_object_opacity(self, opacity: float) -> _AnimationBuilder:
        self.target.set_object_opacity(opacity)
        return self


class Scene(_ir.Scene):
    """High-level scene facade whose authoritative mixed output is ``SceneSpec``."""

    def __init__(self) -> None:
        super().__init__()
        self._cursor = 0.0

    @property
    def time(self) -> float:
        return self._cursor

    def to_scene_spec(self) -> dict[str, Any]:
        """Finalize this authored scene through Rust's canonical mixed-scene adapter.

        Content adapters may still maintain legacy projections during #367 migration,
        but callers consume one validated ``SceneSpec``. The Rust adapter remains the
        authority for geometry/text ordering, text source lowering, retained tracks,
        and family-animation transport.
        """
        try:
            from js import noonCanonicalSceneSpecJson as canonicalize
        except ImportError as error:
            raise RuntimeError(
                "canonical SceneSpec finalization requires the Noon browser Rust bridge"
            ) from error

        retained_document = getattr(self, "retained_document", None)
        if retained_document is None:
            raise RuntimeError(
                "canonical SceneSpec finalization requires retained authoring compatibility"
            )
        legacy_json = json.dumps(
            self.to_document(), separators=(",", ":"), allow_nan=False
        )
        retained_json = json.dumps(
            retained_document(), separators=(",", ":"), allow_nan=False
        )
        return json.loads(str(canonicalize(legacy_json, retained_json)))

    def _raw_object(self, value: Mobject | _ir.Object) -> _ir.Object:
        if isinstance(value, _ir.Object):
            return value
        if not isinstance(value, Mobject) or value._scene is not self or value._object is None:
            raise ValueError("Mobject must belong to this Scene")
        return value._object

    def _raw_target(self, value: Mobject | _ir.Mobject | VectorPath) -> _ir.Mobject | VectorPath:
        if isinstance(value, Mobject):
            return value._current_raw()
        return value

    def _raw_snapshot(self, obj: _ir.Object) -> _ir.Mobject:
        snapshot = self._snapshot_for_object_at(obj, self._cursor)
        return _ir.Mobject(
            geometry=snapshot["geometry"],
            transform=snapshot["transform"],
            style=snapshot["style"],
        )

    def _replace_static_snapshot(self, obj: _ir.Object, raw: _ir.Mobject) -> None:
        if any(track["object"] == obj.id for track in self._tracks):
            raise ValueError(
                "direct Mobject mutation after animation authoring is ambiguous; use mobject.animate"
            )
        position = self._object_positions.get(obj.id)
        if position is None:
            raise ValueError(f"object {obj.id} is not geometry-backed")
        stored = self._objects[position]
        stored["geometry"] = copy.deepcopy(raw.geometry)
        stored["transform"] = copy.deepcopy(raw.transform)
        stored["style"] = copy.deepcopy(raw.style)

    def add(self, *mobjects: Mobject | Group, key: str | None = None) -> Mobject | Scene:
        if not mobjects:
            return self
        flattened: list[Mobject] = []
        for value in mobjects:
            if isinstance(value, Group):
                flattened.extend(value.submobjects)
            elif isinstance(value, Mobject):
                flattened.append(value)
            else:
                raise TypeError("Scene.add expects Mobjects or Groups")
        if key is not None and len(flattened) != 1:
            raise ValueError("an explicit key can only be used when adding one Mobject")
        for index, mobject in enumerate(flattened):
            if mobject._scene is self:
                continue
            mobject._bind_to_scene(self, key=key if index == 0 else None)
        return flattened[0] if len(flattened) == 1 else self

    def circle(self, radius: float, *, key: str | None = None, **kwargs: Any) -> Mobject:
        return self.add(Circle(radius, **kwargs), key=key)  # type: ignore[return-value]

    def rectangle(
        self, width: float, height: float, *, key: str | None = None, **kwargs: Any
    ) -> Mobject:
        return self.add(Rectangle(width, height, **kwargs), key=key)  # type: ignore[return-value]

    def square(
        self, side_length: float = 2.0, *, key: str | None = None, **kwargs: Any
    ) -> Mobject:
        return self.add(Square(side_length, **kwargs), key=key)  # type: ignore[return-value]

    def line(
        self,
        start: Vec2 | tuple[float, float],
        end: Vec2 | tuple[float, float],
        *,
        key: str | None = None,
        **kwargs: Any,
    ) -> Mobject:
        return self.add(Line(start, end, **kwargs), key=key)  # type: ignore[return-value]

    def path(
        self, path: VectorPath, *, key: str | None = None, **kwargs: Any
    ) -> Mobject:
        return self.add(Path(path, **kwargs), key=key)  # type: ignore[return-value]

    def _schedule_create(
        self,
        animation: Create,
        *,
        duration: float,
        start_time: float,
        easing: str,
    ) -> None:
        obj = self._raw_object(animation.target)
        start = float(start_time)
        run_duration = float(duration)
        if not math.isfinite(start) or start < 0.0:
            raise ValueError("start_time must be finite and non-negative")
        if not math.isfinite(run_duration) or run_duration <= 0.0:
            raise ValueError("duration must be finite and positive")
        end = start + run_duration

        snapshot = self._snapshot_for_object_at(obj, start)
        geometry = snapshot["geometry"]
        if not any(name in geometry for name in ("circle", "rectangle", "line", "vector_path")):
            raise ValueError("Create supports Circle, Rectangle/Square, Line, and VectorPath")

        presence_tracks = self._ensure_lifecycle_timeline_available(obj, start, "Create target")
        if presence_tracks and self._presence_at(obj, start):
            raise ValueError("Create target must be absent at animation start")

        for track in self._tracks:
            if track["object"] != obj.id or track["property"] != "reveal":
                continue
            track_start = track["timing"]["start_time"]
            track_end = track_start + track["timing"]["duration"]
            if track_start < end and start < track_end:
                raise ValueError("Create/reveal animations for one object must not overlap")

        object_key = self._object_keys[obj.id]
        root_key = animation.key or f"@create:{object_key}:{start:g}"
        self._add_presence_track(
            obj,
            False,
            True,
            start,
            key=f"{root_key}.show",
        )
        self._add_scalar_track(
            obj,
            "reveal",
            0.0,
            1.0,
            start,
            run_duration,
            easing,
            root_key,
        )

        # Re-creating an object after FadeOut should not inherit appearance=0.
        # Switching to a new track at the Create start is an intentional exact
        # reset; ordinary first-time Create needs no appearance track at all.
        if self._appearance_at(obj, start) != 1.0:
            self._add_scalar_track(
                obj,
                "appearance",
                1.0,
                1.0,
                start,
                run_duration,
                "linear",
                f"{root_key}.appearance",
            )

    def _schedule_uncreate(
        self,
        animation: Uncreate,
        *,
        duration: float,
        start_time: float,
        easing: str,
    ) -> None:
        obj = self._raw_object(animation.target)
        start = float(start_time)
        run_duration = float(duration)
        if not math.isfinite(start) or start < 0.0:
            raise ValueError("start_time must be finite and non-negative")
        if not math.isfinite(run_duration) or run_duration <= 0.0:
            raise ValueError("duration must be finite and positive")
        end = start + run_duration

        snapshot = self._snapshot_for_object_at(obj, start)
        geometry = snapshot["geometry"]
        if not any(name in geometry for name in ("circle", "rectangle", "line", "vector_path")):
            raise ValueError("Uncreate supports Circle, Rectangle/Square, Line, and VectorPath")

        self._ensure_lifecycle_timeline_available(obj, start, "Uncreate target")
        if not self._presence_at(obj, start):
            raise ValueError("Uncreate target must be present at animation start")

        for track in self._tracks:
            if track["object"] != obj.id or track["property"] != "reveal":
                continue
            track_start = track["timing"]["start_time"]
            track_end = track_start + track["timing"]["duration"]
            if track_start < end and start < track_end:
                raise ValueError("Create/reveal animations for one object must not overlap")

        object_key = self._object_keys[obj.id]
        root_key = animation.key or f"@uncreate:{object_key}:{start:g}"
        reverse = bool(animation.reverse_rate_function)
        self._add_scalar_track(
            obj,
            "reveal",
            1.0 if reverse else 0.0,
            0.0 if reverse else 1.0,
            start,
            run_duration,
            easing,
            root_key,
        )
        if animation.remover:
            self._add_presence_track(
                obj,
                True,
                False,
                end,
                key=f"{root_key}.remove",
            )

    def play(
        self,
        *animations: Any,
        duration: float | None = None,
        run_time: float | None = None,
        start_time: float | None = None,
        easing: str = "linear",
    ) -> Scene:
        if not animations:
            raise ValueError("play requires at least one animation")
        if duration is not None and run_time is not None:
            raise ValueError("use either duration or run_time, not both")
        actual_duration = 1.0 if duration is None and run_time is None else (
            float(run_time) if run_time is not None else float(duration)
        )
        actual_start = self._cursor if start_time is None else float(start_time)
        lowered: list[Any] = []
        creates: list[Create] = []
        uncreates: list[Uncreate] = []
        for animation in animations:
            if isinstance(animation, _AnimationBuilder):
                lowered.append(
                    _ir.Transform(
                        self._raw_object(animation.source),
                        animation.target._current_raw(),
                    )
                )
            elif isinstance(animation, Transform):
                lowered.append(
                    _ir.Transform(
                        self._raw_object(animation.source),
                        self._raw_target(animation.target),
                        animation.key,
                    )
                )
            elif isinstance(animation, ReplacementTransform):
                lowered.append(
                    _ir.ReplacementTransform(
                        self._raw_object(animation.source),
                        self._raw_object(animation.target),
                        animation.key,
                    )
                )
            elif isinstance(animation, TransformFromCopy):
                lowered.append(
                    _ir.TransformFromCopy(
                        self._raw_object(animation.source),
                        self._raw_object(animation.target),
                        animation.key,
                    )
                )
            elif isinstance(animation, TransformMatchingShapes):
                lowered.append(
                    _ir.TransformMatchingShapes(
                        [self._raw_object(value) for value in animation.sources],
                        [self._raw_object(value) for value in animation.targets],
                        animation.key,
                    )
                )
            elif isinstance(animation, Uncreate):
                uncreates.append(animation)
            elif isinstance(animation, Create):
                creates.append(animation)
            elif isinstance(animation, FadeIn):
                lowered.append(_ir.FadeIn(self._raw_object(animation.target), animation.key))
            elif isinstance(animation, FadeOut):
                lowered.append(_ir.FadeOut(self._raw_object(animation.target), animation.key))
            else:
                # Keep the existing low-level escape hatch available.
                lowered.append(animation)

        checkpoint = self._authoring_checkpoint()
        try:
            if lowered:
                super().play(
                    *lowered,
                    duration=actual_duration,
                    start_time=actual_start,
                    easing=easing,
                )
            for animation in creates:
                self._schedule_create(
                    animation,
                    duration=actual_duration,
                    start_time=actual_start,
                    easing=easing,
                )
            for animation in uncreates:
                self._schedule_uncreate(
                    animation,
                    duration=actual_duration,
                    start_time=actual_start,
                    easing=easing,
                )
        except Exception:
            self._restore_authoring_checkpoint(checkpoint)
            raise

        self._cursor = max(self._cursor, actual_start + actual_duration)
        return self

    def wait(self, duration: float = 1.0) -> Scene:
        duration = float(duration)
        if not math.isfinite(duration) or duration < 0.0:
            raise ValueError("wait duration must be finite and non-negative")
        self._cursor += duration
        return self

    def animate_position(self, obj: Mobject | _ir.Object, *args: Any, **kwargs: Any) -> Scene:
        super().animate_position(self._raw_object(obj), *args, **kwargs)
        return self

    def animate_rotation(self, obj: Mobject | _ir.Object, *args: Any, **kwargs: Any) -> Scene:
        super().animate_rotation(self._raw_object(obj), *args, **kwargs)
        return self

    def animate_opacity(self, obj: Mobject | _ir.Object, *args: Any, **kwargs: Any) -> Scene:
        super().animate_opacity(self._raw_object(obj), *args, **kwargs)
        return self

    def animate_appearance(self, obj: Mobject | _ir.Object, *args: Any, **kwargs: Any) -> Scene:
        super().animate_appearance(self._raw_object(obj), *args, **kwargs)
        return self

    def animate_reveal(self, obj: Mobject | _ir.Object, *args: Any, **kwargs: Any) -> Scene:
        super().animate_reveal(self._raw_object(obj), *args, **kwargs)
        return self

    def animate_morph(
        self, obj: Mobject | _ir.Object, target: VectorPath, *args: Any, **kwargs: Any
    ) -> Scene:
        super().animate_morph(self._raw_object(obj), target, *args, **kwargs)
        return self


Object = Mobject

__all__ = [
    "BLACK",
    "BLUE",
    "BLUE_A",
    "BLUE_B",
    "BLUE_C",
    "BLUE_D",
    "BLUE_E",
    "Circle",
    "Color",
    "Create",
    "Uncreate",
    "DEGREES",
    "DEFAULT_FRAME_HEIGHT",
    "DEFAULT_FRAME_WIDTH",
    "DEFAULT_MOBJECT_TO_EDGE_BUFFER",
    "DEFAULT_MOBJECT_TO_MOBJECT_BUFFER",
    "DL",
    "DOWN",
    "DR",
    "FadeIn",
    "FadeOut",
    "GOLD",
    "GRAY",
    "GRAY_A",
    "GRAY_B",
    "GRAY_C",
    "GRAY_D",
    "GRAY_E",
    "GREEN",
    "GREEN_A",
    "GREEN_B",
    "GREEN_C",
    "GREEN_D",
    "GREEN_E",
    "Group",
    "GREY",
    "GREY_A",
    "GREY_B",
    "GREY_C",
    "GREY_D",
    "GREY_E",
    "LEFT",
    "LIGHT_PINK",
    "Line",
    "MAROON",
    "Mobject",
    "Object",
    "ORANGE",
    "ORIGIN",
    "PI",
    "PINK",
    "PURPLE",
    "PURPLE_A",
    "PURPLE_B",
    "PURPLE_C",
    "PURPLE_D",
    "PURPLE_E",
    "PatchBatch",
    "Path",
    "RED",
    "RED_A",
    "RED_B",
    "RED_C",
    "RED_D",
    "RED_E",
    "RIGHT",
    "Rectangle",
    "ReplacementTransform",
    "Scene",
    "Square",
    "TAU",
    "TEAL",
    "TEAL_A",
    "TEAL_B",
    "TEAL_C",
    "TEAL_D",
    "TEAL_E",
    "Transform",
    "TransformFromCopy",
    "TransformMatchingShapes",
    "UL",
    "UP",
    "UR",
    "VGroup",
    "Vec2",
    "VectorPath",
    "WHITE",
    "YELLOW",
    "YELLOW_A",
    "YELLOW_B",
    "YELLOW_C",
    "YELLOW_D",
    "YELLOW_E",
    "color_from_hex",
]
