"""Public Noon authoring API.

The public surface favors Manim-like semantic vocabulary. Browser authoring installs
the shared Rust semantic scene facade used for execution.
"""

from __future__ import annotations

import json
import math
from dataclasses import dataclass
from typing import Any, Callable, Iterable, Iterator

import _noon_ir as _ir

VectorPath = _ir.VectorPath
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


def _callback_operations():
    # The adapter selects a staged callback view or the normal semantic handle.
    import _manim_updaters
    return _manim_updaters


class Mobject:
    """Python identity wrapper; shared Rust handles own all semantic state."""

    def __init__(self, raw: _ir.Mobject) -> None:
        raise RuntimeError("Mobject construction requires the shared Rust authoring host")

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
        raise RuntimeError("Mobject copy requires the shared Rust authoring host")

    def _bind(self, scene: Scene, obj: _ir.Object) -> None:
        if self._scene is not None and self._scene is not scene:
            raise ValueError("Mobject already belongs to another Scene")
        self._scene = scene
        self._object = obj

    def _bind_to_scene(self, scene: Scene, *, key: str | None = None) -> _ir.Object:
        return _scene_operations()._bind_mobject(self, scene, key=key)

    def _current_raw(self):
        return _callback_operations()._canonical_current_raw(self)

    def _apply(self, raw: object) -> Mobject:
        return _callback_operations()._canonical_apply(self, raw)

    def get_center(self) -> Vec2:
        return _callback_operations()._canonical_get_center(self)

    @property
    def width(self) -> float:
        raise RuntimeError("Mobject layout requires the shared Rust authoring host")

    @property
    def height(self) -> float:
        raise RuntimeError("Mobject layout requires the shared Rust authoring host")

    def shift(self, direction: object) -> Mobject:
        return _callback_operations()._canonical_shift(self, direction)

    def move_to(self, point: object, *args: object, **kwargs: object) -> Mobject:
        return _callback_operations()._canonical_move_to(self, point, *args, **kwargs)

    def center(self) -> Mobject:
        return self.move_to(ORIGIN)

    def set_x(self, x: float, direction: object = ORIGIN) -> Mobject:
        return _callback_operations()._canonical_set_x(self, x, direction)

    def set_y(self, y: float, direction: object = ORIGIN) -> Mobject:
        return _callback_operations()._canonical_set_y(self, y, direction)

    def scale(self, *args: object, **kwargs: object) -> Mobject:
        return _callback_operations()._canonical_scale(self, *args, **kwargs)

    def rotate(self, *args: object, **kwargs: object) -> Mobject:
        return _callback_operations()._canonical_rotate(self, *args, **kwargs)

    def set_color(self, color: Color) -> Mobject:
        return _callback_operations()._canonical_set_color(self, color)

    def set_fill(self, color: Color | None = None, opacity: float | None = None) -> Mobject:
        return _callback_operations()._canonical_set_fill(self, color, opacity)

    def set_stroke(self, color: Color | None = None, width: float | None = None) -> Mobject:
        return _callback_operations()._canonical_set_stroke(self, color, width)

    def set_opacity(self, opacity: float) -> Mobject:
        return _callback_operations()._canonical_set_opacity(self, opacity)

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
        raise RuntimeError("Mobject layout requires the shared Rust authoring host")

    def align_to(
        self,
        other: Mobject,
        direction: Vec2 | tuple[float, float] = ORIGIN,
    ) -> Mobject:
        raise RuntimeError("Mobject layout requires the shared Rust authoring host")

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
        raise RuntimeError("Mobject layout requires the shared Rust authoring host")

    @property
    def animate(self) -> _AnimationBuilder:
        return _AnimationBuilder(self)


    def add_updater(self, update_function: Callable[..., Any], index: int | None = None, call_updater: bool = False) -> Mobject:
        return _callback_operations().add_updater(self, update_function, index, call_updater)

    def remove_updater(self, update_function: Callable[..., Any]) -> Mobject:
        return _callback_operations().remove_updater(self, update_function)

    def clear_updaters(self, recursive: bool = True) -> Mobject:
        return _callback_operations().clear_updaters(self, recursive)

    def get_updaters(self) -> list[Callable[..., Any]]:
        return _callback_operations().get_updaters(self)

    def has_updaters(self) -> bool:
        return _callback_operations().has_updaters(self)


class Group:
    """Lightweight authoring collection; it does not add runtime hierarchy."""

    def __init__(self, *mobjects: Mobject) -> None:
        raise RuntimeError("Mobject construction requires the shared Rust authoring host")

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
        raise RuntimeError("Mobject layout requires the shared Rust authoring host")

    def shift(self, direction: Vec2 | tuple[float, float]) -> Group:
        raise RuntimeError("Mobject edits require the shared Rust authoring host")

    def arrange(
        self,
        direction: Vec2 | tuple[float, float] = RIGHT,
        buff: float = DEFAULT_MOBJECT_TO_MOBJECT_BUFFER,
        center: bool = True,
    ) -> Group:
        raise RuntimeError("family layout requires the shared Rust authoring host")

    def arrange_in_grid(
        self,
        rows: int | None = None,
        cols: int | None = None,
        buff: float | tuple[float, float] = MED_SMALL_BUFF,
    ) -> Group:
        raise RuntimeError("family layout requires the shared Rust authoring host")


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


def _scene_operations():
    """Load the shared host adapter lazily, after public wrapper classes exist."""
    try:
        import _manim_scene
    except ModuleNotFoundError as error:
        if error.name != "js":
            raise
        raise RuntimeError("Scene operations require the shared Rust authoring host") from None
    return _manim_scene


class Scene:
    """Python authoring facade; semantic operations require the shared Rust host."""

    def __init__(self) -> None:
        # Derived wrapper/export identities only. These rows never carry scene
        # content, painter order, animation tracks, or runtime state.
        self._owner = object()
        self._objects: list[dict[str, Any]] = []
        self._object_keys: dict[int, str] = {}
        self._object_key_ids: dict[str, int] = {}
        self._object_positions: dict[int, int] = {}
        self._next_object_id = 0

    def add(self, *mobjects: Mobject, **kwargs: Any) -> Scene:
        raise RuntimeError("Scene membership requires the shared Rust authoring host")

    def _bind_camera_frame(self, mobject: Mobject) -> Any:
        return _scene_operations()._bind_camera_frame(self, mobject)

    def play(self, *args, **kwargs) -> Any:
        return _scene_operations()._play(self, *args, **kwargs)

    def wait(self, duration: float = 1.0) -> Any:
        return _scene_operations()._canonical_wait(self, duration)

    def declare_wait(self, duration: float = 1.0) -> Scene:
        return _scene_operations()._declare_wait(self, duration)

    @property
    def time(self) -> float:
        return _scene_operations()._canonical_scene_time(self)

    def value_tracker(self, value: float = 0.0) -> Any:
        return _scene_operations()._canonical_value_tracker(self, value)

    def bind_position(
        self, mobject: object, tracker: object,
        direction: object = None, offset: object = None,
    ) -> Scene:
        return _scene_operations()._canonical_bind_position(self, mobject, tracker, direction, offset)

    def pointer_position_signal(self) -> Any:
        return _scene_operations()._canonical_pointer_position_signal(self)

    def pointer_button_signal(self, button: int = 0, initial: bool = False) -> Any:
        return _scene_operations()._canonical_pointer_button_signal(self, button, initial)

    def key_state_signal(self, code: str, initial: bool = False) -> Any:
        return _scene_operations()._canonical_key_state_signal(self, code, initial)

    def viewport_size_signal(self) -> Any:
        return _scene_operations()._canonical_viewport_size_signal(self)

    def wheel_delta_signal(self) -> Any:
        return _scene_operations()._canonical_wheel_delta_signal(self)

    def gesture_delta_signal(self, name: str) -> Any:
        return _scene_operations()._canonical_gesture_delta_signal(self, name)

    def control_signal(self, name: str, value: float = 0.0) -> Any:
        return _scene_operations()._canonical_control_signal(self, name, value)

    def pointer_down_events(self, button: int = 0) -> Any:
        return _scene_operations()._canonical_pointer_down_events(self, button)

    def pointer_up_events(self, button: int = 0) -> Any:
        return _scene_operations()._canonical_pointer_up_events(self, button)

    def key_press_events(self, code: str) -> Any:
        return _scene_operations()._canonical_key_press_events(self, code)

    def key_release_events(self, code: str) -> Any:
        return _scene_operations()._canonical_key_release_events(self, code)

    def wheel_events(self) -> Any:
        return _scene_operations()._canonical_wheel_events(self)

    def gesture_events(self, name: str) -> Any:
        return _scene_operations()._canonical_gesture_events(self, name)

    def control_commit_events(self, name: str) -> Any:
        return _scene_operations()._canonical_control_commit_events(self, name)

    def bind_rotation(self, mobject: object, tracker: object) -> Any:
        return _scene_operations()._canonical_bind_rotation_dispatch(self, mobject, tracker)

    def bind_opacity(self, mobject: object, tracker: object) -> Any:
        return _scene_operations()._canonical_bind_opacity_dispatch(self, mobject, tracker)

    def bind_presence(self, mobject: object, signal: object) -> Any:
        return _scene_operations()._canonical_bind_presence_dispatch(self, mobject, signal)

    def bind_appearance(self, mobject: object, tracker: object) -> Any:
        return _scene_operations()._canonical_bind_appearance_dispatch(self, mobject, tracker)

    def bind_reveal(self, mobject: object, tracker: object) -> Any:
        return _scene_operations()._canonical_bind_reveal_dispatch(self, mobject, tracker)

    def bind_morph(self, mobject: object, tracker: object) -> Any:
        return _scene_operations()._canonical_bind_morph_dispatch(self, mobject, tracker)

    def live_execution(self, duration: float | None = None) -> Any:
        return _scene_operations()._live_execution(self, duration)

    def declare_live_transform_to(
        self, source: Mobject, target: Mobject, *,
        run_time: float = 1.0, rate_func: object = "smooth",
    ) -> Any:
        return _scene_operations()._declare_live_transform_to(
            self, source, target, run_time=run_time, rate_func=rate_func,
        )


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
