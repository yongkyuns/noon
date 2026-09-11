"""Public Noon authoring API.

Public classes and methods delegate to shared Rust semantic operations. Python
owns authoring syntax, argument conversion, and wrapper identity.
"""

from __future__ import annotations

import math
from typing import Any, Callable

from _noon_errors import (
    NoonError,
    NoonErrorCause,
    NoonValueError,
    NoonForeignHandleError,
    NoonStaleHandleError,
    NoonMissingResourceError,
    NoonUnsupportedError,
    NoonPendingError,
    NoonStalePublicationError,
    NoonCallbackError,
    NoonOwnershipError,
)
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
    """Accept Noon's Vec2 plus common Manim 2D/3D vector inputs.

    Manim commonly represents 2D directions as three-component NumPy vectors. Noon
    remains 2D internally, so z=0 is accepted and non-zero z is rejected explicitly.
    """

    if isinstance(value, Vec2):
        return value

    try:
        length = len(value)  # type: ignore[arg-type]
    except (TypeError, AttributeError):
        length = None

    if length in (2, 3):
        try:
            x = float(value[0])  # type: ignore[index]
            y = float(value[1])  # type: ignore[index]
            if length == 3:
                z = float(value[2])  # type: ignore[index]
                if not math.isclose(z, 0.0, abs_tol=1e-12):
                    raise NotImplementedError(
                        "Noon currently supports 2D Manim vectors only; z must be 0"
                    )
            return Vec2(x, y)
        except (TypeError, ValueError, IndexError) as error:
            raise TypeError("expected a two- or three-component numeric vector") from error

    raise TypeError("expected a two- or three-component vector")


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


def _semantic_operations():
    import _manim_semantic_handles
    return _manim_semantic_handles


def _callback_operations():
    # The adapter selects a staged callback view or the normal semantic handle.
    import _manim_updaters
    return _manim_updaters


class Mobject:
    """Python identity wrapper; shared Rust handles own all semantic state."""

    def __init__(self) -> None:
        raise TypeError("Mobject is a base type; construct a Circle, Rectangle, Line, or Path")

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
        return _semantic_operations()._copy_mobject(self)

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

    def get_edge_center(self: Mobject, direction: object) -> Vec2:
        return self.get_critical_point(direction)

    def get_corner(self: Mobject, direction: object) -> Vec2:
        return self.get_critical_point(direction)

    def get_left(self: Mobject) -> Vec2:
        return self.get_critical_point(LEFT)

    def get_right(self: Mobject) -> Vec2:
        return self.get_critical_point(RIGHT)

    def get_top(self: Mobject) -> Vec2:
        return self.get_critical_point(UP)

    def get_bottom(self: Mobject) -> Vec2:
        return self.get_critical_point(DOWN)

    def get_coord(
        self: Mobject, dim: int, direction: object = ORIGIN
    ) -> float:
        if dim not in (0, 1):
            raise NotImplementedError("Noon currently exposes x/y authoring coordinates only")
        point = self.get_critical_point(direction)
        return float(point[dim])

    def get_x(self: Mobject, direction: object = ORIGIN) -> float:
        return self.get_coord(0, direction)

    def get_y(self: Mobject, direction: object = ORIGIN) -> float:
        return self.get_coord(1, direction)

    def scale_to_fit_width(self: Mobject, width: float, **kwargs: Any) -> Mobject:
        return self.rescale_to_fit(width, 0, stretch=False, **kwargs)

    def scale_to_fit_height(self: Mobject, height: float, **kwargs: Any) -> Mobject:
        return self.rescale_to_fit(height, 1, stretch=False, **kwargs)

    def stretch(self, factor: float, dim: int, *, about_point=None, about_edge=None) -> Mobject:
        from _manim_semantic_handles import _stretch
        return _stretch(self, factor, dim, about_point=about_point, about_edge=about_edge)

    def stretch_about_point(self, factor: float, dim: int, point: object) -> Mobject:
        return self.stretch(factor, dim, about_point=point)

    def stretch_to_fit_width(self: Mobject, width: float, **kwargs: Any) -> Mobject:
        return self.rescale_to_fit(width, 0, stretch=True, **kwargs)

    def stretch_to_fit_height(self: Mobject, height: float, **kwargs: Any) -> Mobject:
        return self.rescale_to_fit(height, 1, stretch=True, **kwargs)

    def match_width(
        self: Mobject, mobject: Mobject, **kwargs: Any
    ) -> Mobject:
        return self.match_dim_size(mobject, 0, **kwargs)

    def match_height(
        self: Mobject, mobject: Mobject, **kwargs: Any
    ) -> Mobject:
        return self.match_dim_size(mobject, 1, **kwargs)

    def rescale_to_fit(self, length: float, dim: int, stretch: bool = False, **kwargs: Any) -> Mobject:
        from _manim_semantic_handles import _rescale_to_fit
        return _rescale_to_fit(self, length, dim, stretch, **kwargs)

    def match_dim_size(self, mobject: Mobject, dim: int, **kwargs: Any) -> Mobject:
        from _manim_semantic_handles import _match_dim_size
        return _match_dim_size(self, mobject, dim, **kwargs)

    def generate_target(self, use_deepcopy: bool = False) -> Mobject:
        from _manim_compat import _mobject_generate_target
        return _mobject_generate_target(self, use_deepcopy)

    def save_state(self) -> Mobject:
        from _manim_compat import _mobject_save_state
        return _mobject_save_state(self)

    def restore(self) -> Mobject:
        from _manim_compat import _mobject_restore
        return _mobject_restore(self)

    def get_color(self) -> Color:
        from _manim_geometry import _mobject_get_color
        return _mobject_get_color(self)

    def match_points(self, mobject: object) -> Mobject:
        from _manim_geometry import match_points
        return match_points(self, mobject)

    def get_center(self) -> Vec2:
        return _callback_operations()._canonical_get_center(self)

    @property
    def width(self) -> float:
        return _semantic_operations()._width(self)

    @width.setter
    def width(self, value: float) -> None:
        _semantic_operations()._set_width_property(self, value)

    @property
    def height(self) -> float:
        return _semantic_operations()._height(self)

    @height.setter
    def height(self, value: float) -> None:
        _semantic_operations()._set_height_property(self, value)

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

    def set_coord(self, value: float, dim: int, direction: object = ORIGIN) -> Mobject:
        from _manim_shared_geometry import _set_coord
        return _set_coord(self, value, dim, direction)

    def match_coord(self, mobject: Mobject, dim: int, direction: object = ORIGIN) -> Mobject:
        from _manim_shared_geometry import _match_coord
        return _match_coord(self, mobject, dim, direction)

    def match_x(self, mobject: Mobject, direction: object = ORIGIN) -> Mobject:
        return self.match_coord(mobject, 0, direction)

    def match_y(self, mobject: Mobject, direction: object = ORIGIN) -> Mobject:
        return self.match_coord(mobject, 1, direction)

    @property
    def z_index(self) -> float:
        return _semantic_operations()._get_z_index(self)

    @z_index.setter
    def z_index(self, value: float) -> None:
        self.set_z_index(value, family=False)

    def set_z_index(self, z_index_value: float, family: bool = True) -> Mobject:
        return _semantic_operations()._set_z_index(self, z_index_value, family)

    def flip(self, axis=UP, *, about_point=None, about_edge=None) -> Mobject:
        return _semantic_operations()._flip(self, axis, about_point=about_point, about_edge=about_edge)

    def rotate_about_origin(
        self, angle: float, axis: object = (0.0, 0.0, 1.0), **kwargs: Any,
    ) -> Mobject:
        return self.rotate(angle, axis=axis, about_point=ORIGIN, **kwargs)

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
        """Set whole-object opacity independently of fill/stroke paint alpha."""
        return _callback_operations()._canonical_set_opacity(self, opacity)

    def next_to(
        self,
        mobject_or_point: object,
        direction: object = RIGHT,
        buff: float = DEFAULT_MOBJECT_TO_MOBJECT_BUFFER,
        aligned_edge: object = ORIGIN,
        submobject_to_align: object | None = None,
        index_of_submobject_to_align: int | None = None,
        coor_mask: object = (1.0, 1.0, 1.0),
    ) -> Mobject | Group:
        return _semantic_operations()._next_to(self, mobject_or_point, direction, buff, aligned_edge, submobject_to_align, index_of_submobject_to_align, coor_mask)

    def align_to(self, mobject_or_point: object, direction: object = ORIGIN) -> Mobject:
        return _semantic_operations()._align_to(self, mobject_or_point, direction)

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
        return _semantic_operations()._align_on_frame(self, direction, buff)

    @property
    def animate(self):
        from _manim_animate import _AlignedAnimationBuilder
        return _AlignedAnimationBuilder(self)


    def add_updater(
        self,
        update_function: Callable[..., Any],
        index: int | None = None,
        call_updater: bool = False,
    ) -> Mobject:
        return _callback_operations().add_updater(self, update_function, index, call_updater)

    def remove_updater(self, update_function: Callable[..., Any]) -> Mobject:
        return _callback_operations().remove_updater(self, update_function)

    def clear_updaters(self, recursive: bool = True) -> Mobject:
        return _callback_operations().clear_updaters(self, recursive)

    def get_updaters(self) -> list[Callable[..., Any]]:
        return _callback_operations().get_updaters(self)

    def has_updaters(self) -> bool:
        return _callback_operations().has_updaters(self)

    def _copy_for_animate_target(self) -> Mobject:
        return _semantic_operations()._target_mobject(self)

    def get_critical_point(self, direction: object) -> Vec2:
        return _semantic_operations()._get_critical_point(self, direction)

    def become(
        self,
        mobject: Mobject,
        match_height: bool = False,
        match_width: bool = False,
        match_depth: bool = False,
        match_center: bool = False,
        stretch: bool = False,
    ) -> Mobject:
        return _semantic_operations()._become(self, mobject, match_height, match_width, match_depth, match_center, stretch)

    def replace(self, mobject: Mobject, dim_to_match: int = 0, stretch: bool = False) -> Mobject:
        return _semantic_operations()._replace(self, mobject, dim_to_match, stretch)

    def __deepcopy__(self, memo):
        from _manim_compat import deepcopy_semantic_wrapper
        return deepcopy_semantic_wrapper(self, memo)


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
        # Derived host binding IDs map to the authoritative Rust handles.
        self._binding_handles: dict[int, object] = {}
        self._object_keys: dict[int, str] = {}
        self._object_key_ids: dict[str, int] = {}
        self._next_object_id = 0

    def setup(self) -> None:
        pass

    def construct(self) -> None:
        pass

    def tear_down(self) -> None:
        pass

    def _register_top_level(self, value: object) -> None:
        _scene_operations()._register_membership_wrappers(self, value)

    @property
    def mobjects(self) -> list[object]:
        return _scene_operations()._canonical_scene_mobjects(self)

    def _edit_membership(self, kind: str, values: tuple[object, ...] = (), *, key=None) -> None:
        _scene_operations()._canonical_edit_membership(self, kind, values, key=key)

    def add(self, *mobjects: object, key: str | None = None) -> Mobject | Scene:
        if not mobjects:
            return self
        self._edit_membership("add", mobjects, key=key)

        # Python returns the wrapper for a single leaf, or the Scene for a batch.
        from _manim_compat import _leaf_mobjects
        leaves = [member for value in mobjects for member in _leaf_mobjects(value)]
        return leaves[0] if len(leaves) == 1 else self

    def remove(self, *mobjects: object) -> Scene:
        self._edit_membership("remove", mobjects)
        return self

    def clear(self) -> Scene:
        self._edit_membership("clear")
        return self

    def replace(self, old_mobject: object, new_mobject: object) -> Scene:
        self._edit_membership("replace", (old_mobject, new_mobject))
        return self

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

# Public wrappers resolve from their defining modules without startup mutation.
_PUBLIC_EXPORTS = {
    "Transform": "_manim_animate",
    "ReplacementTransform": "_manim_animate",
    "TransformFromCopy": "_manim_animate",
    "TransformMatchingShapes": "_manim_animate",
    "Create": "_manim_animate",
    "Uncreate": "_manim_animate",
    "FadeIn": "_manim_animate",
    "FadeOut": "_manim_animate",
    "Indicate": "_manim_animate",
    "ScaleInPlace": "_manim_animate",
    "ShrinkToCenter": "_manim_animate",

    "linear": "_manim_rate_functions",
    "smooth": "_manim_rate_functions",
    "rush_into": "_manim_rate_functions",
    "rush_from": "_manim_rate_functions",
    "there_and_back": "_manim_rate_functions",
    "Add": "_manim_composition",
    "AnimationGroup": "_manim_composition",
    "LaggedStart": "_manim_composition",
    "LaggedStartMap": "_manim_composition",
    "Succession": "_manim_composition",
    "Wait": "_manim_composition",
    "Rotate": "_manim_rotate",
    "Rotating": "_manim_rotate",
    "FocusOn": "_manim_rotate",
    "ShowIncreasingSubsets": "_manim_lifecycle",
    "ShowSubmobjectsOneByOne": "_manim_lifecycle",
    "GrowFromPoint": "_manim_growing",
    "GrowFromCenter": "_manim_growing",
    "GrowFromEdge": "_manim_growing",
    "SpinInFromNothing": "_manim_growing",
    "DrawBorderThenFill": "_manim_draw_border_then_fill",
    "ShowPassingFlash": "_manim_indication",
    "ValueTracker": "_manim_reactive",
    "NativeVectorSignal": "_manim_reactive",
    "NativeBoolSignal": "_manim_reactive",
    "DashedLine": "_manim_dashed_line",
    "MovingCameraScene": "_manim_camera",
    "Write": "_manim_family_creation",
    "Unwrite": "_manim_family_creation",
    "VMobject": "_manim_compat",
    "Circle": "_manim_compat",
    "Rectangle": "_manim_compat",
    "Square": "_manim_compat",
    "Line": "_manim_compat",
    "Path": "_manim_compat",
    "Group": "_manim_compat",
    "VGroup": "_manim_compat",
    "MoveToTarget": "_manim_compat",
    "OUT": "_manim_compat",
    "IN": "_manim_compat",
    "Arc": "_manim_arc",
    "ArcBetweenPoints": "_manim_arc",
    "Arrow": "_manim_arrow",
    "Vector": "_manim_arrow",
    "DoubleArrow": "_manim_arrow",
    "Elbow": "_manim_shared_geometry",
    "RoundedRectangle": "_manim_shared_geometry",
    "SurroundingRectangle": "_manim_shared_geometry",
    "BackgroundRectangle": "_manim_shared_geometry",
    "Underline": "_manim_shared_geometry",
    "Union": "_manim_shared_geometry",
    "Intersection": "_manim_shared_geometry",
    "Difference": "_manim_shared_geometry",
    "Exclusion": "_manim_shared_geometry",

    "AnnularSector": "_manim_shared_geometry",
    "Sector": "_manim_shared_geometry",
    "Annulus": "_manim_shared_geometry",
    "Dot": "_manim_geometry",
    "Ellipse": "_manim_geometry",
    "Triangle": "_manim_geometry",
    "ApplyMethod": "_manim_geometry",
    "DEFAULT_DOT_RADIUS": "_manim_geometry",
    "PURE_YELLOW": "_manim_geometry",
    "Text": "_manim_typst",
    "Typst": "_manim_typst",
    "MathTypst": "_manim_typst",
}


def __getattr__(name: str):
    module = _PUBLIC_EXPORTS.get(name)
    if module is not None:
        from importlib import import_module
        return getattr(import_module(module), name)
    raise AttributeError(f"module {__name__!r} has no attribute {name!r}")


def __dir__():
    return sorted(set(globals()) | set(__all__))


__all__ = [
    "NoonError",
    "NoonErrorCause",
    "NoonValueError",
    "NoonForeignHandleError",
    "NoonStaleHandleError",
    "NoonMissingResourceError",
    "NoonUnsupportedError",
    "NoonPendingError",
    "NoonStalePublicationError",
    "NoonCallbackError",
    "NoonOwnershipError",

    "BLACK",
    "BLUE",
    "BLUE_A",
    "BLUE_B",
    "BLUE_C",
    "BLUE_D",
    "BLUE_E",
    "Color",
    "DEGREES",
    "DEFAULT_FRAME_HEIGHT",
    "DEFAULT_FRAME_WIDTH",
    "DEFAULT_MOBJECT_TO_EDGE_BUFFER",
    "DEFAULT_MOBJECT_TO_MOBJECT_BUFFER",
    "DL",
    "DOWN",
    "DR",
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
    "GREY",
    "GREY_A",
    "GREY_B",
    "GREY_C",
    "GREY_D",
    "GREY_E",
    "LEFT",
    "LIGHT_PINK",
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
    "RED",
    "RED_A",
    "RED_B",
    "RED_C",
    "RED_D",
    "RED_E",
    "RIGHT",
    "Scene",
    "TAU",
    "TEAL",
    "TEAL_A",
    "TEAL_B",
    "TEAL_C",
    "TEAL_D",
    "TEAL_E",
    "UL",
    "UP",
    "UR",
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
    "SMALL_BUFF",
    "MED_SMALL_BUFF",
    "MED_LARGE_BUFF",
    "LARGE_BUFF",
    *_PUBLIC_EXPORTS,
]
