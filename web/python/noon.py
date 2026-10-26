"""Public Noon authoring API.

Public classes and methods delegate to shared Rust semantic operations. Python
owns authoring syntax, argument conversion, and wrapper identity.
"""

from __future__ import annotations

import math
import operator
from functools import partialmethod
from typing import TYPE_CHECKING, Any, Callable, Self
from enum import Enum
from importlib.metadata import PackageNotFoundError, version

if TYPE_CHECKING:
    from manim.typing import Point3DLike

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
    MultiAnimationOverrideException,
)
import _noon_ir as _ir

VectorPath = _ir.VectorPath
Color = _ir.Color


def _default_mobject_path_func():
    from _manim_user_vectors import straight_path
    return straight_path()


class Vec2(tuple):
    """Explicit planar vector accepted by Noon's 2D conveniences."""

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

    This explicit planar projection accepts three-component inputs with z=0.
    Full-coordinate operations use their XYZ argument adapter instead.
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


class _Direction3(tuple):
    """Immutable three-component arithmetic for Manim coordinates and directions."""

    __slots__ = ()

    def __new__(cls, x, y, z):
        return tuple.__new__(cls, (
            _ir._finite_number("direction component", float(value)) for value in (x, y, z)
        ))

    def __getnewargs__(self):
        return tuple(self)

    @property
    def x(self):
        return self[0]

    @property
    def y(self):
        return self[1]

    @property
    def z(self):
        return self[2]

    @staticmethod
    def _array_operand(value):
        # Let array libraries own their arithmetic/result types, without
        # importing NumPy on Noon's cold authoring path.
        return hasattr(type(value), "__array_ufunc__")

    @staticmethod
    def _components(value):
        if isinstance(value, Vec2):
            return (*value, 0.0)
        components = tuple(value)
        if len(components) != 3:
            raise TypeError("direction arithmetic requires three components")
        return tuple(float(component) for component in components)

    def __add__(self, other):
        if self._array_operand(other):
            return NotImplemented
        return _Direction3(*(left + right for left, right in zip(self, self._components(other))))

    def __radd__(self, other):
        return self + other

    def __rsub__(self, other):
        if self._array_operand(other):
            return NotImplemented
        return _Direction3(*(right - left for left, right in zip(self, self._components(other))))

    def __sub__(self, other):
        if self._array_operand(other):
            return NotImplemented
        return _Direction3(*(left - right for left, right in zip(self, self._components(other))))

    def __neg__(self):
        return self * -1

    def __mul__(self, scalar):
        if self._array_operand(scalar):
            return NotImplemented
        factor = float(scalar)
        return _Direction3(*(component * factor for component in self))

    def __rmul__(self, scalar):
        return self * scalar

    def __truediv__(self, scalar):
        if self._array_operand(scalar):
            return NotImplemented
        return self * (1 / float(scalar))

    def length(self):
        return math.hypot(*self)

    def normalized(self):
        length = self.length()
        if length == 0.0:
            raise ValueError("direction must be non-zero")
        return self / length



ORIGIN = _Direction3(0.0, 0.0, 0.0)
UP = _Direction3(0.0, 1.0, 0.0)
DOWN = _Direction3(0.0, -1.0, 0.0)
LEFT = _Direction3(-1.0, 0.0, 0.0)
RIGHT = _Direction3(1.0, 0.0, 0.0)
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


class SectionType(str, Enum):
    NORMAL = "normal"
    SKIP = "skip"



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


def override_animation(
    animation_class: type[Any],
) -> Callable[[Callable[..., Any]], Callable[..., Any]]:
    """Mark a Mobject method as an override for the supplied animation class."""
    def decorator(func: Callable[..., Any]) -> Callable[..., Any]:
        setattr(func, "_override_animation", animation_class)
        return func

    return decorator


class Mobject:
    """Python identity wrapper; shared Rust handles own all semantic state."""

    animation_overrides: dict[type[Any], Callable[..., Any]] = {}

    def __init__(
        self, color: Color = WHITE, name: str | None = None, dim: int = 3,
        target: Mobject | None = None, z_index: float = 0,
    ) -> None:
        from operator import index
        if isinstance(dim, bool):
            raise TypeError("Mobject dimension must be an integer")
        dimension = index(dim)
        if dimension < 0:
            raise ValueError("Mobject dimension must be non-negative")
        if dimension not in (2, 3):
            raise NotImplementedError("Mobject currently supports planar dimensions 2 and 3")
        _semantic_operations()._empty_path_init(self, color=color, kind="Mobject", z_index=z_index)
        self.name = self.__class__.__name__ if name is None else name
        self.dim = dim
        self.target = target
        self.generate_points()
        self.init_colors()

    @property
    def submobjects(self):
        """Read direct children from Rust; use membership methods to edit them."""
        return _semantic_operations()._group_members(self)

    @submobjects.setter
    def submobjects(self, members):
        _semantic_operations()._replace_submobjects(self, members)

    def _assert_valid_submobjects(self, submobjects):
        return self._assert_valid_submobjects_internal(submobjects, Mobject)

    def _assert_valid_submobjects_internal(self, submobjects, mob_class):
        for index, member in enumerate(submobjects):
            if not isinstance(member, mob_class):
                message = (f"Only values of type {mob_class.__name__} can be added "
                           f"as submobjects of {type(self).__name__}, but the value "
                           f"{member} (at index {index}) is of type {type(member).__name__}.")
                if isinstance(member, Mobject):
                    message += " You can try adding this value into a Group instead."
                raise TypeError(message)
            if member is self:
                raise ValueError(f"Cannot add {type(self).__name__} as a submobject of itself (at index {index}).")
        return self

    def add(self, *mobjects):
        return _semantic_operations()._group_add(self, *mobjects)

    def add_to_back(self, *mobjects):
        return _semantic_operations()._group_add_to_back(self, *mobjects)

    def insert(self, index, mobject):
        return _semantic_operations()._group_insert(self, index, mobject)

    def remove(self, *mobjects):
        return _semantic_operations()._group_remove(self, *mobjects)

    def get_group_class(self):
        from _manim_compat import Group
        return Group

    def __iter__(self):
        from _manim_semantic_handles import _group_iter
        return _group_iter(self)

    def __len__(self):
        from _manim_semantic_handles import _group_length
        return _group_length(self)

    def __getitem__(self, value):
        from _manim_semantic_handles import _group_indexed_member, _group_slice
        return _group_slice(self, value) if isinstance(value, slice) else _group_indexed_member(self, value)

    def set(self, **kwargs: Any) -> Mobject:
        for attr, value in kwargs.items():
            setattr(self, attr, value)
        return self

    def interpolate_color(
        self, mobject1: Mobject, mobject2: Mobject, alpha: float
    ) -> Mobject:
        """Raise the pinned base-class override error for unsupported objects."""
        raise NotImplementedError("Please override in a child class.")

    def interpolate(
        self,
        mobject1: Mobject,
        mobject2: Mobject,
        alpha: float,
        path_func: Callable[..., Any] = _default_mobject_path_func(),
    ) -> Self:
        """Interpolate supported VMobject geometry and style through shared Rust."""
        from _manim_semantic_handles import _interpolate_manim
        return _interpolate_manim(self, mobject1, mobject2, alpha, path_func)

    def point_from_proportion(self, alpha: float) -> Any:
        raise NotImplementedError("Please override in a child class.")

    def proportion_from_point(self, point: object) -> float:
        raise NotImplementedError("Please override in a child class.")

    def get_point_mobject(self, center: object | None = None) -> Mobject:
        message = f"get_point_mobject not implemented for {self.__class__.__name__}"
        raise NotImplementedError(message)

    def repeat_submobject(self, submob: Mobject) -> Mobject:
        """Return an independent shared-semantic copy of ``submob``."""
        return submob.copy()

    def align_points(self, mobject: Mobject) -> Mobject:
        count = self.get_num_points()
        other_count = mobject.get_num_points()
        if count < other_count:
            self.align_points_with_larger(mobject)
        elif other_count < count:
            mobject.align_points_with_larger(self)
        return self

    def align_submobjects(self, mobject: Mobject) -> Self:
        own_members = getattr(self, "submobjects", ())
        other_members = getattr(mobject, "submobjects", ())
        own_growth = max(0, len(other_members) - len(own_members))
        other_growth = max(0, len(own_members) - len(other_members))
        own_add = getattr(self, "add_n_more_submobjects", None)
        other_add = getattr(mobject, "add_n_more_submobjects", None)
        if (own_growth and not callable(own_add)) or (
            other_growth and not callable(other_add)
        ):
            raise NotImplementedError(
                "align_submobjects requires family-backed growth for both objects"
            )
        if callable(own_add):
            own_add(own_growth)
        if callable(other_add):
            other_add(other_growth)
        return self

    def null_point_align(self, mobject: Mobject) -> Self:
        for left, right in ((self, mobject), (mobject, self)):
            if left.has_no_points() and right.has_points():
                right.push_self_into_submobjects()
        return self

    def push_self_into_submobjects(self) -> Self:
        """Move own points to an appended child through shared Rust semantics."""
        from _manim_point_promotion import push_self_into_submobjects
        return push_self_into_submobjects(self)

    def align_data(
        self, mobject: Mobject, skip_point_alignment: bool = False
    ) -> Self:
        self.null_point_align(mobject)
        self.align_submobjects(mobject)
        if not skip_point_alignment:
            self.align_points(mobject)
        own_members = getattr(self, "submobjects", ())
        other_members = getattr(mobject, "submobjects", ())
        for own_member, other_member in zip(own_members, other_members, strict=True):
            own_member.align_data(other_member)
        return self

    def align_points_with_larger(self, larger_mobject: Mobject) -> Mobject:
        raise NotImplementedError("Please override in a child class.")

    @staticmethod
    def get_mobject_type_class():
        return Mobject

    _original__init__ = __init__

    def __init_subclass__(cls, **kwargs: Any) -> None:
        super().__init_subclass__(**kwargs)
        cls.animation_overrides = {}
        cls._add_intrinsic_animation_overrides()
        cls._original__init__ = cls.__init__

    @classmethod
    def animation_override_for(cls, animation_class: type[Any]) -> Callable[..., Any] | None:
        return cls.animation_overrides.get(animation_class)

    @classmethod
    def add_animation_override(
        cls,
        animation_class: type[Any],
        override_func: Callable[..., Any],
    ) -> None:
        if animation_class not in cls.animation_overrides:
            cls.animation_overrides[animation_class] = override_func
        else:
            existing = cls.animation_overrides[animation_class]
            raise MultiAnimationOverrideException(
                f"The animation {animation_class.__name__} for "
                f"{cls.__name__} is overridden by more than one method: "
                f"{existing.__qualname__} and {override_func.__qualname__}."
            )

    @classmethod
    def _add_intrinsic_animation_overrides(cls) -> None:
        for method_name in dir(cls):
            if method_name.startswith("__"):
                continue
            method = getattr(cls, method_name)
            if hasattr(method, "_override_animation"):
                cls.add_animation_override(method._override_animation, method)

    @classmethod
    def set_default(cls, **kwargs: Any) -> None:
        """Change future Python constructor defaults, or restore this class's original constructor."""
        cls.__init__ = partialmethod(cls.__init__, **kwargs) if kwargs else cls._original__init__

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

    def get_edge_center(self: Mobject, direction: object) -> Any:
        return self.get_critical_point(direction)

    def get_corner(self: Mobject, direction: object) -> Any:
        return self.get_critical_point(direction)

    def get_left(self: Mobject) -> Any:
        return self.get_critical_point(LEFT)

    def get_right(self: Mobject) -> Any:
        return self.get_critical_point(RIGHT)

    def get_top(self: Mobject) -> Any:
        return self.get_critical_point(UP)

    def get_bottom(self: Mobject) -> Any:
        return self.get_critical_point(DOWN)

    def get_zenith(self: Mobject) -> Any:
        return self.get_critical_point((0., 0., 1.))

    def get_nadir(self: Mobject) -> Any:
        return self.get_critical_point((0., 0., -1.))

    def get_coord(
        self: Mobject, dim: int, direction: object = ORIGIN
    ) -> float:
        dim = operator.index(dim)
        normalized_dim = dim if dim >= 0 else 3 + dim
        if not 0 <= normalized_dim < 3:
            raise IndexError(f"index {dim} is out of bounds for axis 0 with size 3")
        if isinstance(direction, Vec2) and normalized_dim == 2:
            key = 0.0
        else:
            key = direction[normalized_dim]
        return self.get_extremum_along_dim(dim=normalized_dim, key=key)

    def get_x(self: Mobject, direction: object = ORIGIN) -> float:
        return self.get_coord(0, direction)

    def get_y(self: Mobject, direction: object = ORIGIN) -> float:
        return self.get_coord(1, direction)

    def get_z(self: Mobject, direction: object = ORIGIN) -> float:
        return self.get_coord(2, direction)

    def scale_to_fit_width(self: Mobject, width: float, **kwargs: Any) -> Mobject:
        return self.rescale_to_fit(width, 0, stretch=False, **kwargs)

    def scale_to_fit_height(self: Mobject, height: float, **kwargs: Any) -> Mobject:
        return self.rescale_to_fit(height, 1, stretch=False, **kwargs)

    def scale_to_fit_depth(self: Mobject, depth: float, **kwargs: Any) -> Mobject:
        return self.rescale_to_fit(depth, 2, stretch=False, **kwargs)

    def stretch(self, factor: float, dim: int, *, about_point=None, about_edge=None) -> Mobject:
        from _manim_semantic_handles import _stretch
        return _stretch(self, factor, dim, about_point=about_point, about_edge=about_edge)

    def stretch_about_point(self, factor: float, dim: int, point: object) -> Mobject:
        return self.stretch(factor, dim, about_point=point)

    def stretch_to_fit_width(self: Mobject, width: float, **kwargs: Any) -> Mobject:
        return self.rescale_to_fit(width, 0, stretch=True, **kwargs)

    def stretch_to_fit_height(self: Mobject, height: float, **kwargs: Any) -> Mobject:
        return self.rescale_to_fit(height, 1, stretch=True, **kwargs)

    def stretch_to_fit_depth(self: Mobject, depth: float, **kwargs: Any) -> Mobject:
        return self.rescale_to_fit(depth, 2, stretch=True, **kwargs)

    def match_width(
        self: Mobject, mobject: Mobject, **kwargs: Any
    ) -> Mobject:
        return self.match_dim_size(mobject, 0, **kwargs)

    def match_height(
        self: Mobject, mobject: Mobject, **kwargs: Any
    ) -> Mobject:
        return self.match_dim_size(mobject, 1, **kwargs)

    def match_depth(
        self: Mobject, mobject: Mobject, **kwargs: Any
    ) -> Mobject:
        return self.match_dim_size(mobject, 2, **kwargs)

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

    def get_family(self, recurse: bool = True) -> list[Mobject]:
        """Return existing wrappers in shared family order, including this root."""
        from _manim_semantic_handles import _get_family
        return _get_family(self)

    def get_family_updaters(self) -> list[Callable[..., Any]]:
        """Return each member's updaters in shared family order."""
        return [
            updater
            for mobject in self.get_family()
            for updater in mobject.get_updaters()
        ]

    def split(self) -> list[Mobject]:
        """Return this point-owning object, if any, followed by direct children."""
        from _manim_semantic_handles import _group_split
        return _group_split(self)

    def family_members_with_points(self) -> list[Mobject]:
        """Return point-owning family members in the shared family order."""
        return [mobject for mobject in self.get_family() if mobject.get_num_points() > 0]

    def reverse_points(self) -> Self:
        """Reverse point order through the shared path and family operations."""
        from _manim_path_editing import reverse_points
        return reverse_points(self)

    def repeat(self, count: int) -> Self:
        """Repeat supported point arrays through the shared path/family operation."""
        from _manim_path_editing import repeat_points
        return repeat_points(self, count)

    def space_out_submobjects(self, factor: float = 1.5, **kwargs: Any) -> Self:
        """Expand the family, then shrink direct children through shared transforms."""
        self.scale(factor, **kwargs)
        for submobject in getattr(self, "submobjects", ()):
            submobject.scale(1.0 / factor, **kwargs)
        return self

    def apply_to_family(self, func: Callable[[Mobject], None]) -> Mobject:
        """Apply ``func`` to every point-owning member in shared family order."""
        for mobject in self.family_members_with_points():
            func(mobject)
        return self

    def match_color(self, mobject: Mobject) -> Mobject:
        """Match the donor's observed color through the existing shared setter."""
        return self.set_color(mobject.get_color())

    def to_original_color(self) -> Mobject:
        """Apply this object's current color to its shared family."""
        color = self.color if hasattr(self, "color") else self.get_color()
        return self.set_color(color)

    def set_color_by_gradient(self, *colors: object) -> Mobject:
        return self.set_submobject_colors_by_gradient(*colors)

    def set_submobject_colors_by_gradient(self, *colors: object) -> Mobject:
        from _manim_compat import _as_color
        if not colors:
            raise ValueError("Need at least one color")
        parsed = [_as_color("gradient color", color) for color in colors]
        if len(parsed) == 1:
            return self.set_color(parsed[0])
        if not self.family_members_with_points():
            return self
        from _manim_semantic_handles import _set_color_by_gradient
        return _set_color_by_gradient(self, *parsed)

    def set_colors_by_radial_gradient(
        self,
        center: object | None = None,
        radius: float = 1,
        inner_color: object = WHITE,
        outer_color: object = BLACK,
    ) -> Mobject:
        from _manim_semantic_handles import _set_color_by_radial_gradient
        return _set_color_by_radial_gradient(
            self, center, radius, inner_color, outer_color
        )

    def set_submobject_colors_by_radial_gradient(
        self,
        center: object | None = None,
        radius: float = 1,
        inner_color: object = WHITE,
        outer_color: object = BLACK,
    ) -> Mobject:
        return self.set_colors_by_radial_gradient(
            center, radius, inner_color, outer_color
        )

    def match_points(self, mobject: object) -> Mobject:
        from _manim_geometry import match_points
        return match_points(self, mobject)

    def get_center(self) -> Any:
        return _callback_operations()._canonical_get_center(self)

    def get_z_index_reference_point(self) -> Any:
        """Return the center of this object's z-index grouping target."""
        z_index_group = getattr(self, "z_index_group", self)
        return z_index_group.get_center()

    def get_start(self) -> Any:
        """Return the first point of this object's path."""
        self.throw_error_if_no_points()
        from _manim_path_queries import endpoint
        return endpoint(self, False)

    def get_end(self) -> Any:
        """Return the last point of this object's path."""
        self.throw_error_if_no_points()
        from _manim_path_queries import endpoint
        return endpoint(self, True)

    def get_start_and_end(self) -> tuple[Any, Any]:
        """Return the starting and ending point of this object's path."""
        return self.get_start(), self.get_end()

    def get_num_points(self) -> int:
        """Return the number of points in this object's path."""
        from _manim_path_queries import point_count
        return point_count(self)

    def invert(self, recursive: bool = False) -> Self:
        """Reverse child order when this object has a family-aware wrapper."""
        return self

    def sort(
        self,
        point_to_num_func: Callable[[Point3DLike], float] = lambda point: point[0],
        submob_func: Callable[[Mobject], Any] | None = None,
    ) -> Self:
        """Sort family-aware children; a bare point object has no children."""
        return self

    def sort_submobjects(self, *args: Any, **kwargs: Any) -> Self:
        return self.sort(*args, **kwargs)

    def shuffle(self, recursive: bool = False) -> Self:
        """Shuffle children when this object has a family-aware wrapper."""
        return self

    def shuffle_submobjects(self, *args: Any, **kwargs: Any) -> Self:
        return self.shuffle(*args, **kwargs)

    def arrange(
        self,
        direction: object = RIGHT,
        buff: float = DEFAULT_MOBJECT_TO_MOBJECT_BUFFER,
        center: bool = True,
        **kwargs: Any,
    ) -> Self:
        """Apply Manim's leaf-object arrangement behavior.

        Family wrappers override this with shared Rust layout. A point-owning
        leaf has no direct children, so its only arrangement effect is the
        optional shared move to the origin.
        """
        if center:
            self.center()
        return self

    def arrange_submobjects(self, *args: Any, **kwargs: Any) -> Self:
        return self.arrange(*args, **kwargs)

    def has_points(self) -> bool:
        from _manim_path_queries import point_count
        return point_count(self) > 0

    def has_no_points(self) -> bool:
        return not self.has_points()

    def reset_points(self) -> Mobject:
        """Clear this object's authored points through the shared path editor."""
        from _manim_path_editing import clear_points
        return clear_points(self)

    def init_colors(self, propagate_colors: bool = True) -> Mobject:
        """Base color hook for concrete compatibility classes to override."""
        del propagate_colors
        return self

    def generate_points(self) -> Mobject:
        """Base point hook for concrete compatibility classes to override."""
        return self

    def throw_error_if_no_points(self) -> None:
        if self.has_no_points():
            import sys
            caller_name = sys._getframe(1).f_code.co_name
            raise Exception(
                f"Cannot call Mobject.{caller_name} for a Mobject with no points"
            )

    def nonempty_submobjects(self) -> list[Mobject]:
        return [
            submobject
            for submobject in getattr(self, "submobjects", ())
            if submobject.get_num_points() > 0
            or bool(getattr(submobject, "submobjects", ()))
        ]

    def get_all_points(self):
        """Return this object's and its family's authored point arrays."""
        from _manim_path_queries import all_points
        return all_points(self)

    def get_pieces(self, n_pieces: float):
        """Split a vector leaf into equal curve-parameter pieces.

        Path subdivision is delegated to shared VMobject operations and the
        result uses Manim's base Group constructor.
        Shared family roots are not supported by this path-only profile.
        """
        from operator import index
        from _manim_compat import Group, _is_shared_family

        if _is_shared_family(self):
            raise NotImplementedError(
                "get_pieces for shared-family roots requires shared path ownership"
            )

        template = self.copy()
        template.submobjects = []

        sample_count = index(n_pieces + 1)
        if sample_count < 0:
            raise ValueError(
                f"Number of samples, {sample_count}, must be non-negative."
            )
        if sample_count == 0:
            alphas = []
        elif sample_count == 1:
            alphas = [0.0]
        else:
            alphas = [i / (sample_count - 1) for i in range(sample_count)]

        return Group(
            *(
                template.copy().pointwise_become_partial(self, a1, a2)
                for a1, a2 in zip(alphas[:-1], alphas[1:], strict=True)
            )
        )

    def set_points(self, points):
        """Assign this root's canonical point data through the shared Rust edit."""
        from _manim_path_editing import set_points
        return set_points(self, points)

    def get_array_attrs(self) -> list[str]:
        return ["points"]

    def apply_over_attr_arrays(self, func: Callable[[Any], Any]) -> Self:
        """Map this object's supported point array and publish it through Rust."""
        from _manim_path_editing import apply_over_attr_arrays
        return apply_over_attr_arrays(self, func)

    def apply_points_function_about_point(
        self,
        func: Callable[[Any], Any],
        about_point: object | None = None,
        about_edge: object | None = None,
    ) -> Self:
        from _manim_path_editing import apply_points_function_about_point
        return apply_points_function_about_point(
            self, func, about_point=about_point, about_edge=about_edge
        )

    def apply_function(
        self,
        function: Callable[[Any], Any],
        *,
        about_point: object | None = None,
        about_edge: object | None = None,
    ) -> Self:
        from _manim_path_editing import apply_function
        return apply_function(
            self, function, about_point=about_point, about_edge=about_edge
        )

    def apply_complex_function(
        self,
        function: Callable[[complex], complex],
        *,
        about_point: object | None = None,
        about_edge: object | None = None,
    ) -> Self:
        from _manim_path_editing import apply_complex_function
        return apply_complex_function(
            self, function, about_point=about_point, about_edge=about_edge
        )

    def apply_matrix(
        self,
        matrix: object,
        *,
        about_point: object | None = None,
        about_edge: object | None = None,
    ) -> Self:
        from _manim_path_editing import apply_matrix
        return apply_matrix(
            self, matrix, about_point=about_point, about_edge=about_edge
        )

    def get_merged_array(self, array_attr: str):
        from _manim_path_queries import merged_array
        return merged_array(self, array_attr)

    def get_points_defining_boundary(self):
        return self.get_all_points()

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

    @property
    def depth(self) -> float:
        return _semantic_operations()._layout_dimension(self, 2)

    @depth.setter
    def depth(self, value: float) -> None:
        self.scale_to_fit_depth(value)

    def shift(self, direction: object) -> Mobject:
        return _callback_operations()._canonical_shift(self, direction)

    def move_to(self, point_or_mobject: object, *args: object, **kwargs: object) -> Mobject:
        return _callback_operations()._canonical_move_to(self, point_or_mobject, *args, **kwargs)

    def put_start_and_end_on(self, start: object, end: object) -> Self:
        from _manim_shared_geometry import _put_start_and_end_on
        return _put_start_and_end_on(self, start, end)

    def center(self) -> Mobject:
        return self.move_to(ORIGIN)

    def align_on_border(
        self,
        direction: object,
        buff: float = DEFAULT_MOBJECT_TO_EDGE_BUFFER,
    ) -> Mobject:
        from _manim_updaters import canonical_callback_phase_active
        if canonical_callback_phase_active():
            raise NotImplementedError("frame alignment queries are unavailable during staged callbacks")
        return self._align_on_frame(_as_vec2(direction), float(buff))

    def apply_function_to_position(
        self,
        function: Callable[[Any], Any],
    ) -> Self:
        """Map the Rust-backed center and move this object to the result."""
        from _manim_compat import _manim_point3
        point = _manim_point3(self.get_center())
        if isinstance(point, list):
            point = tuple(point)
        self.move_to(function(point))
        return self

    def apply_function_to_submobject_positions(
        self,
        function: Callable[[Any], Any],
    ) -> Self:
        """Map each direct child's center using its Rust-backed placement."""
        for submobject in getattr(self, "submobjects", ()):
            submobject.apply_function_to_position(function)
        return self

    def set_x(self, x: float, direction: object = ORIGIN) -> Mobject:
        return _callback_operations()._canonical_set_x(self, x, direction)

    def set_y(self, y: float, direction: object = ORIGIN) -> Mobject:
        return _callback_operations()._canonical_set_y(self, y, direction)

    def set_z(self, z: float, direction: object = ORIGIN) -> Mobject:
        """Keep planar objects on z=0; spatial z placement is unsupported."""
        if float(z) != 0.0:
            raise NotImplementedError("Noon currently supports only planar z=0 placement")
        return self

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

    def match_z(self, mobject: Mobject, direction: object = ORIGIN) -> Mobject:
        return self.match_coord(mobject, 2, direction)

    @property
    def z_index(self) -> float:
        return _semantic_operations()._get_z_index(self)

    @z_index.setter
    def z_index(self, value: float) -> None:
        self.set_z_index(value, family=False)

    def set_z_index(self, z_index_value: float, family: bool = True) -> Mobject:
        return _semantic_operations()._set_z_index(self, z_index_value, family)

    def set_z_index_by_z_Point3D(self) -> Mobject:
        # Noon geometry is planar, so its pinned 3D center z coordinate is zero.
        return self.set_z_index(0.0)

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

    def set_color(
        self,
        color: object = Color(1.0, 1.0, 0.0),
        alpha: Any = None,
        family: bool = True,
    ) -> Mobject:
        # The pinned base method does not consume alpha. This base wrapper has no
        # child family; Group and VMobject provide their own family paint behavior.
        del alpha, family
        from _manim_compat import _as_color
        parsed = _as_color("color", color)
        return _callback_operations()._canonical_set_color(self, parsed)

    def set_glow(self, *, color=None, radius=None, intensity=None, source=None, scope=None) -> Mobject:
        from _noon_effects import set_glow
        return set_glow(self, color=color, radius=radius, intensity=intensity, source=source, scope=scope)

    def add_effect(self, definition, *, name, scope=None) -> Mobject:
        from _noon_effects import add_effect
        return add_effect(self, definition, name=name, scope=scope)

    def get_effect(self, name):
        from _noon_effects import get_effect
        return get_effect(self, name)

    def set_effect(self, selector, *, color=None, radius=None, intensity=None, source=None) -> Mobject:
        from _noon_effects import set_effect
        return set_effect(self, selector, color=color, radius=radius, intensity=intensity, source=source)

    def remove_effect(self, selector) -> Mobject:
        from _noon_effects import remove_effect
        return remove_effect(self, selector)

    def remove_glow(self) -> Mobject:
        from _noon_effects import remove_glow
        return remove_glow(self)

    def set_fill(self, color: Color | None = None, opacity: float | None = None) -> Mobject:
        return _callback_operations()._canonical_set_fill(self, color, opacity)

    def set_stroke(self, color: Color | None = None, width: float | None = None) -> Mobject:
        return _callback_operations()._canonical_set_stroke(self, color, width)

    def set_opacity(self, opacity: float) -> Mobject:
        return _callback_operations()._canonical_set_opacity(self, opacity)

    def fade(self, darkness: float = 0.5, family: bool = True) -> Mobject:
        return _callback_operations()._canonical_fade(self, darkness, family)

    def fade_to(self, color: Color, alpha: float, family: bool = True) -> Mobject:
        """Interpolate this object's color toward ``color`` by ``alpha``."""
        return _callback_operations()._canonical_fade_to(self, color, alpha, family)

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

    def shift_onto_screen(self, **kwargs: Any) -> Mobject:
        from _manim_updaters import canonical_callback_phase_active
        if canonical_callback_phase_active():
            raise NotImplementedError("frame-bound queries are unavailable during staged callbacks")
        for direction in (UP, DOWN, LEFT, RIGHT):
            horizontal = direction.x != 0.0
            radius = DEFAULT_FRAME_WIDTH / 2.0 if horizontal else DEFAULT_FRAME_HEIGHT / 2.0
            max_value = radius - kwargs.get("buff", DEFAULT_MOBJECT_TO_EDGE_BUFFER)
            edge_center = self.get_edge_center(direction)
            projection = edge_center[0] * direction.x + edge_center[1] * direction.y
            if projection > max_value:
                self.to_edge(direction, **kwargs)
        return self

    def is_off_screen(self) -> bool:
        x_radius = DEFAULT_FRAME_WIDTH / 2.0
        y_radius = DEFAULT_FRAME_HEIGHT / 2.0
        if self.get_left()[0] > x_radius:
            return True
        if self.get_right()[0] < -x_radius:
            return True
        if self.get_bottom()[1] > y_radius:
            return True
        return self.get_top()[1] < -y_radius

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

    @property
    def always(self):
        """Build immediate, repeatedly invoked updater calls like ManimCE."""
        from _manim_updaters import _UpdaterBuilder
        return _UpdaterBuilder(self)


    def add_updater(
        self,
        update_function: Callable[..., Any],
        index: int | None = None,
        call_updater: bool = False,
    ) -> Mobject:
        return _callback_operations().add_updater(self, update_function, index, call_updater)

    def set_effective_circle(self, radius: float) -> Mobject:
        """Replace this callback target's effective circle without editing authored content.

        This first producer is supported only by the last updater in an ordered
        callback phase. A later region cannot yet read the new geometry.
        """
        return _callback_operations().set_effective_circle(self, radius)

    def set_effective_path(
        self, points: list[tuple[float, float]], closed: bool = False
    ) -> Mobject:
        """Replace this callback target's effective content with a polygonal path.

        Path output is limited to a finite list of at least two points and must
        be produced by the final updater in its callback phase.
        """
        return _callback_operations().set_effective_path(self, points, closed)

    def set_effective_text(self, source: Mobject) -> Mobject:
        """Replace callback-effective content with a prebuilt Text resource."""
        return _callback_operations().set_effective_text(self, source)

    def remove_updater(self, update_function: Callable[..., Any]) -> Mobject:
        return _callback_operations().remove_updater(self, update_function)

    def clear_updaters(self, recursive: bool = True) -> Mobject:
        return _callback_operations().clear_updaters(self, recursive)

    def get_updaters(self) -> list[Callable[..., Any]]:
        return _callback_operations().get_updaters(self)

    def get_time_based_updaters(self) -> list[Callable[..., Any]]:
        return _callback_operations().get_time_based_updaters(self)

    def has_time_based_updater(self) -> bool:
        return _callback_operations().has_time_based_updater(self)

    def match_updaters(self, mobject: Mobject) -> Mobject:
        """Clear this family, then copy only the donor root's callbacks."""
        return _callback_operations().match_updaters(self, mobject)

    @property
    def updating_suspended(self) -> bool:
        return _callback_operations().updating_suspended(self)

    @updating_suspended.setter
    def updating_suspended(self, value: object) -> None:
        # Manim stores this attribute directly; boolean conversion preserves its
        # truth-value contract while keeping the shared Rust node strongly typed.
        _callback_operations().set_updating_suspended(self, bool(value), recursive=False)

    def suspend_updating(self, recursive: bool = True) -> Mobject:
        return _callback_operations().suspend_updating(self, recursive)

    def resume_updating(self, recursive: bool = True) -> Mobject:
        return _callback_operations().resume_updating(self, recursive)

    def update(self, dt: float = 0, recursive: bool = True) -> Mobject:
        """Invoke detached-object updaters; live updates belong to Rust phases."""
        return _callback_operations().update(self, dt, recursive)

    def has_updaters(self) -> bool:
        return _callback_operations().has_updaters(self)

    def _copy_for_animate_target(self) -> Mobject:
        return _semantic_operations()._target_mobject(self)

    def get_critical_point(self, direction: object) -> Any:
        return _semantic_operations()._get_critical_point(self, direction)

    def get_extremum_along_dim(self, points=None, dim: int = 0, key: int = 0):
        from _manim_path_queries import extremum_along_dim
        return extremum_along_dim(self, points, dim, key)

    def reduce_across_dimension(self, reduce_func: Callable[[Any], Any], dim: int):
        from _manim_path_queries import reduce_across_dimension
        return reduce_across_dimension(self, reduce_func, dim)

    def get_center_of_mass(self) -> Vec2:
        from _manim_path_queries import center_of_mass
        return center_of_mass(self)

    def length_over_dim(self, dim: int) -> float:
        from _manim_path_queries import length_over_dim
        return length_over_dim(self, dim)

    def get_boundary_point(self, direction: object) -> Vec2:
        from _manim_path_queries import boundary_point
        return boundary_point(self, direction)

    def get_midpoint(self) -> Vec2:
        return self.point_from_proportion(0.5)

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

    def surround(
        self,
        mobject: Mobject,
        dim_to_match: int = 0,
        stretch: bool = False,
        buff: float = MED_SMALL_BUFF,
    ) -> Mobject:
        self.replace(mobject, dim_to_match, stretch)
        length = mobject.length_over_dim(dim_to_match)
        self.scale((length + buff) / length)
        return self

    def add_background_rectangle(
        self,
        color: object | None = None,
        opacity: float = 0.75,
        **kwargs: Any,
    ) -> Mobject:
        """Prepend a shared BackgroundRectangle to a family-backed object.

        The public base API assumes every Mobject owns a mutable submobject
        list. Noon currently represents that capability with shared Group and
        VGroup family handles, so bare geometry objects are rejected before
        constructing or publishing a rectangle.
        """
        add_to_back = getattr(self, "add_to_back", None)
        if not callable(add_to_back):
            raise NotImplementedError(
                "add_background_rectangle requires a shared family object"
            )
        from _manim_shared_geometry import BackgroundRectangle

        background_rectangle = BackgroundRectangle(
            self, color=color, fill_opacity=opacity, **kwargs
        )
        self.background_rectangle = background_rectangle
        add_to_back(background_rectangle)
        return self

    def __deepcopy__(self, memo):
        from _manim_compat import deepcopy_semantic_wrapper
        return deepcopy_semantic_wrapper(self, memo)


# Compiler capture checks this original descriptor, including after a user
# replaces the public method. It never executes user code to discover a kernel.
_CANONICAL_MOBJECT_REPLACE_METHOD = Mobject.replace


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

    @property
    def foreground_mobjects(self) -> list[object]:
        return _scene_operations()._canonical_scene_foreground_mobjects(self)

    def _edit_membership(
        self, kind: str, values: tuple[object, ...] = (), *, key=None
    ) -> None:
        _scene_operations()._canonical_edit_membership(self, kind, values, key=key)

    def add(self, *mobjects: object, key: str | None = None) -> Mobject | Scene:
        if not mobjects:
            return self
        # Validate the caller's ordinary facade contract before Rust publication.
        from _manim_compat import _leaf_mobjects
        leaves = [member for value in mobjects for member in _leaf_mobjects(value)]
        if key is not None and (len(mobjects) != 1 or not isinstance(mobjects[0], Mobject)):
            raise ValueError("an explicit key requires one ordinary Mobject add")
        self._edit_membership("add", mobjects, key=key)
        return leaves[0] if len(leaves) == 1 else self

    def add_foreground_mobjects(self, *mobjects: object) -> Scene:
        if mobjects:
            self._edit_membership("add_foreground", mobjects)
        return self

    def add_foreground_mobject(self, mobject: object) -> Scene:
        return self.add_foreground_mobjects(mobject)

    def remove_foreground_mobjects(self, *mobjects: object) -> Scene:
        if mobjects:
            self._edit_membership("remove_foreground", mobjects)
        return self

    def remove_foreground_mobject(self, mobject: object) -> Scene:
        return self.remove_foreground_mobjects(mobject)

    def bring_to_front(self, *mobjects: object) -> Scene:
        self.add(*mobjects)
        return self

    def bring_to_back(self, *mobjects: object) -> Scene:
        self._edit_membership("bring_to_back", mobjects)
        return self

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

    def declare_value(
        self, tracker: object, target: float, *, run_time: float = 1.0,
        rate_func: object = "smooth",
    ) -> Scene:
        """Predeclare a scalar interval on the shared Rust authoring cursor.

        Use with Scene.value_tracker/bind_position before playback. This is an
        explicit finite declaration, not a Scene.play continuation barrier.
        """
        return _scene_operations()._declare_value(
            self, tracker, target, run_time=run_time, rate_func=rate_func,
        )

    def audio(
        self, source: object, *, start_time: float = 0.0,
        source_start_frame: int = 0, source_end_frame: int | None = None,
        duration: float | None = None, loop: bool = False, gain: float = 1.0,
        fade_in: float = 0.0, fade_out: float = 0.0,
    ) -> Any:
        """Declare a finite PCM clip before execution without advancing time.

        Times are authored seconds; start_time is absolute and defaults to zero.
        Source trims are half-open PCM frame indices. A loop requires duration;
        fades apply once across the clip, including repeated source regions.
        """
        from _noon_audio import audio
        return audio(
            self, source, start_time=start_time, source_start_frame=source_start_frame,
            source_end_frame=source_end_frame, duration=duration, loop=loop,
            gain=gain, fade_in=fade_in, fade_out=fade_out,
        )

    def on_click_playback(self, target: Mobject, action: object) -> Scene:
        """Declare a finite viewer control through the normal shared Rust picker."""
        from _noon_audio import on_click_playback
        return on_click_playback(self, target, action)

    @property
    def time(self) -> float:
        return _scene_operations()._canonical_scene_time(self)

    def next_section(
        self, name: str = "", type: SectionType = SectionType.NORMAL
    ) -> Scene:
        return _scene_operations()._canonical_next_section(self, name, type)

    @property
    def sections(self) -> list[dict[str, object]]:
        return _scene_operations()._canonical_sections(self)

    def value_tracker(self, value: float = 0.0) -> Any:
        return _scene_operations()._canonical_value_tracker(self, value)

    def bind_position(
        self, mobject: object, tracker: object,
        direction: object = None, offset: object = None,
    ) -> Scene:
        return _scene_operations()._canonical_bind_position(self, mobject, tracker, direction, offset)

    def on_click(self, target: Mobject, animation: object) -> Scene:
        """Run Indicate(target) in Rust on a primary click of a filled circle/rectangle.

        Clicks during that target's indication are ignored. Completion restores
        its appearance; script time stays unchanged. No Python callback is kept.
        """
        return _scene_operations()._on_click(self, target, animation)

    def set_drag_targets(self, *mobjects: Mobject) -> Scene:
        """Allow primary native translation dragging for these bound Mobjects.

        Rust owns picking, capture, effective writes, authored reconciliation,
        and cancellation. This declaration only selects semantic handles.
        """
        return _scene_operations()._set_translation_drag_targets(self, *mobjects)

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
def always_redraw(producer: Callable[[], Mobject]) -> Mobject:
    """Refresh one stable inline Mobject from a bounded visual producer.

    The current bounded producer accepts Circle, Rectangle/Square, Line,
    retained Path content, or prebuilt Text resources. Shape results replace
    effective geometry, transform, and style together; Text results select an
    existing text resource without shaping new text during the callback.
    Authored content and object identity stay fixed. The producer runs during
    construction and once per scheduled callback phase.
    """
    if not callable(producer):
        raise TypeError("always_redraw producer must be callable")
    from _manim_compat import Circle, Rectangle, Line, Path
    from _manim_typst import Text
    target = producer()
    shape_types = (Circle, Rectangle, Line, Path)
    text_producer = isinstance(target, Text)
    if not (isinstance(target, shape_types) or text_producer):
        raise NotImplementedError(
            "always_redraw requires Circle, Rectangle, Square, Line, Path, or prebuilt Text"
        )

    def refresh(mobject: Mobject, _dt: float) -> None:
        result = producer()
        if text_producer:
            if not isinstance(result, Text):
                raise NotImplementedError(
                    "Text always_redraw producer must return a prebuilt Text resource"
                )
            mobject.set_effective_text(result)
            return
        if not isinstance(result, shape_types):
            raise NotImplementedError(
                "always_redraw producer must return Circle, Rectangle, Square, Line, or Path"
            )
        from _manim_updaters import _canonical_provisional_context
        if _canonical_provisional_context(result) is None:
            raise RuntimeError(
                "always_redraw producer must construct a fresh shape in its callback phase"
            )
        mobject.become(result)

    target.add_updater(refresh)
    return target


_PUBLIC_EXPORTS = {
    "AudioSource": "_noon_audio",
    "AudioClip": "_noon_audio",
    "PlaybackControl": "_noon_audio",
    "typing": "_manim_typing",
    "get_unit_normal": "_manim_user_vector_bases",
    "z_to_vector": "_manim_user_vector_bases",
    "lingering": "_manim_rate_functions",
    "AS2700": "_manim_color_palettes",
    "BS381": "_manim_color_palettes",
    "DVIPSNAMES": "_manim_color_palettes",
    "SVGNAMES": "_manim_color_palettes",
    "X11": "_manim_color_palettes",
    "XKCD": "_manim_color_palettes",
    "rate_functions": "_manim_rate_functions",
    "unit": "_manim_unit",

    "CubicBezier": "_manim_geometry",

    "get_winding_number": "_manim_user_vectors",
    "bezier": "_manim_user_vectors",
    "partial_bezier_points": "_manim_user_vectors",
    "split_bezier": "_manim_user_vectors",
    "subdivide_bezier": "_manim_user_vectors",
    "bezier_remap": "_manim_user_vectors",
    "point_lies_on_bezier": "_manim_user_vectors",
    "proportions_along_bezier_curve_for_point": "_manim_user_vectors",
    "is_closed": "_manim_user_vectors",
    "straight_path": "_manim_user_vectors",
    "line_intersection": "_manim_user_vectors",
    "find_intersection": "_manim_user_vectors",
    "quaternion_mult": "_manim_user_vectors",
    "quaternion_from_angle_axis": "_manim_user_vectors",
    "angle_axis_from_quaternion": "_manim_user_vectors",
    "quaternion_conjugate": "_manim_user_vectors",
    "thick_diagonal": "_manim_user_vectors",
    "rotation_about_z": "_manim_user_vectors",
    "rotation_matrix": "_manim_user_vectors",
    "rotate_vector": "_manim_user_vectors",
    "compass_directions": "_manim_user_vectors",
    "cartesian_to_spherical": "_manim_user_vectors",
    "spherical_to_cartesian": "_manim_user_vectors",
    "complex_to_R3": "_manim_user_vectors",
    "R3_to_complex": "_manim_user_vectors",
    "complex_func_to_R3_func": "_manim_user_vectors",
    "center_of_mass": "_manim_user_vectors",
    "midpoint": "_manim_user_vectors",
    "normalize": "_manim_user_vectors",
    "angle_of_vector": "_manim_user_vectors",
    "angle_between_vectors": "_manim_user_vectors",
    "shoelace": "_manim_user_vectors",
    "shoelace_direction": "_manim_user_vectors",
    "cross2d": "_manim_user_vectors",
    "add_extension_if_not_present": "_manim_file_paths",
    "guarantee_existence": "_manim_file_paths",
    "guarantee_empty_existence": "_manim_file_paths",
    "seek_full_path_from_defaults": "_manim_file_paths",
    "ensure_executable": "_manim_file_paths",
    "modify_atime": "_manim_file_paths",
    "interpolate": "_manim_interpolation",
    "integer_interpolate": "_manim_interpolation",
    "mid": "_manim_interpolation",
    "inverse_interpolate": "_manim_interpolation",
    "match_interpolate": "_manim_interpolation",
    "binary_search": "_manim_simple_functions",
    "choose": "_manim_simple_functions",
    "clip": "_manim_simple_functions",
    "sigmoid": "_manim_simple_functions",
    "adjacent_n_tuples": "_manim_iterables",
    "adjacent_pairs": "_manim_iterables",
    "all_elements_are_instances": "_manim_iterables",
    "concatenate_lists": "_manim_iterables",
    "list_difference_update": "_manim_iterables",
    "list_update": "_manim_iterables",
    "listify": "_manim_iterables",
    "tuplify": "_manim_iterables",
    "make_even": "_manim_iterables",
    "make_even_by_cycling": "_manim_iterables",
    "remove_list_redundancies": "_manim_iterables",
    "remove_nones": "_manim_iterables",
    "batch_by_property": "_manim_iterables",
    "uniq_chain": "_manim_iterables",
    "hash_obj": "_manim_iterables",
    "stretch_array_to_length": "_manim_iterables",
    "DictAsObject": "_manim_config_utils",
    "merge_dicts_recursively": "_manim_config_utils",
    "update_dict_recursively": "_manim_config_utils",
    "RendererType": "_manim_enums",
    "LineJointType": "_manim_enums",
    "CapStyleType": "_manim_enums",
    "DefaultSectionType": "_manim_enums",
    "SampleSpace": "_manim_sample_space",
    "NumberLine": "_manim_plotting",
    "UnitInterval": "_manim_plotting",
    "LinearBase": "_manim_plotting",
    "LogBase": "_manim_plotting",
    "BarChart": "_manim_plotting",
    "NumberPlane": "_manim_number_plane",
    "VectorScene": "_manim_vector_space",
    "LinearTransformationScene": "_manim_vector_space",
    "PolarPlane": "_manim_polar_plane",
    "ComplexPlane": "_manim_complex_plane",
    "Axes": "_manim_plotting",
    "ThreeDAxes": "_manim_plotting",
    "Surface": "_manim_spatial_geometry",
    "Sphere": "_manim_spatial_geometry",
    "Dot3D": "_manim_spatial_geometry",
    "Cube": "_manim_spatial_geometry",
    "Cylinder": "_manim_spatial_geometry",
    "Prism": "_manim_spatial_geometry",
    "Line3D": "_manim_spatial_geometry",
    "Torus": "_manim_spatial_geometry",
    "Cone": "_manim_spatial_geometry",
    "FunctionGraph": "_manim_plotting",
    "Graph": "_manim_graph",
    "DiGraph": "_manim_graph",
    "ImplicitFunction": "_manim_implicit",
    "ParametricFunction": "_manim_plotting",
    "Transform": "_manim_animate",
    "MoveAlongPath": "_manim_animate",
    "ReplacementTransform": "_manim_animate",
    "TransformFromCopy": "_manim_animate",
    "TransformMatchingShapes": "_manim_animate",
    "TransformMatchingTex": "_manim_animate",
    "ApplyMatrix": "_manim_animate",
    "CyclicReplace": "_manim_animate",
    "Swap": "_manim_animate",
    "Create": "_manim_animate",
    "Uncreate": "_manim_animate",
    "FadeIn": "_manim_animate",
    "FadeOut": "_manim_animate",
    "Indicate": "_manim_animate",
    "ScaleInPlace": "_manim_animate",
    "ShrinkToCenter": "_manim_animate",

    "linear": "_manim_rate_functions",
    "smooth": "_manim_rate_functions",
    "smoothstep": "_manim_rate_functions",
    "smootherstep": "_manim_rate_functions",
    "smoothererstep": "_manim_rate_functions",
    "slow_into": "_manim_rate_functions",
    "double_smooth": "_manim_rate_functions",
    "rush_into": "_manim_rate_functions",
    "rush_from": "_manim_rate_functions",
    "there_and_back": "_manim_rate_functions",
    "there_and_back_with_pause": "_manim_rate_functions",
    "running_start": "_manim_rate_functions",
    "wiggle": "_manim_rate_functions",
    "exponential_decay": "_manim_rate_functions",
    "not_quite_there": "_manim_rate_functions",
    "squish_rate_func": "_manim_rate_functions",
    "Add": "_manim_composition",
    "AnimationGroup": "_manim_composition",
    "LaggedStart": "_manim_composition",
    "LaggedStartMap": "_manim_composition",
    "Succession": "_manim_composition",
    "Wait": "_manim_composition",
    "UpdateFromFunc": "_manim_composition",
    "Rotate": "_manim_rotate",
    "Rotating": "_manim_rotate",
    "FocusOn": "_manim_rotate",
    "ShowIncreasingSubsets": "_manim_lifecycle",
    "ShowSubmobjectsOneByOne": "_manim_lifecycle",
    "GrowFromPoint": "_manim_growing",
    "GrowFromCenter": "_manim_growing",
    "GrowFromEdge": "_manim_growing",
    "GrowArrow": "_manim_growing",
    "SpinInFromNothing": "_manim_growing",
    "DrawBorderThenFill": "_manim_draw_border_then_fill",
    "ShowPassingFlash": "_manim_indication",
    "ValueTracker": "_manim_reactive",
    "NativeVectorSignal": "_manim_reactive",
    "NativeBoolSignal": "_manim_reactive",
    "DashedLine": "_manim_dashed_line",
    "MovingCameraScene": "_manim_camera",
    "ZoomedScene": "_manim_zoomed_scene",
    "Write": "_manim_family_creation",
    "Unwrite": "_manim_family_creation",
    "VMobject": "_manim_compat",
    "VectorizedPoint": "_manim_compat",
    "CurvesAsSubmobjects": "_manim_compat",
    "SVGMobject": "_manim_svg",
    "ImageMobject": "_manim_image",
    "Glow": "_noon_effects",
    "LinearGradient": "_noon_paints",
    "GlowPulse": "_noon_effects",
    "Pixels": "_noon_effects",
    "EffectHandle": "_noon_effects",
    "Mesh3D": "_noon_spatial",
    "SpatialScene": "_noon_spatial",
    "ThreeDScene": "_noon_spatial",
    "WorldTransformTo": "_noon_spatial",
    "RESAMPLING_ALGORITHMS": "_manim_image",
    "Circle": "_manim_compat",
    "Rectangle": "_manim_compat",
    "ScreenRectangle": "_manim_compat",
    "Square": "_manim_compat",
    "Line": "_manim_compat",
    "Path": "_manim_compat",
    "Group": "_manim_compat",
    "VGroup": "_manim_compat",
    "VDict": "_manim_compat",
    "MoveToTarget": "_manim_compat",
    "Restore": "_manim_compat",
    "OUT": "_manim_compat",
    "IN": "_manim_compat",
    "Arc": "_manim_arc",
    "ArcBetweenPoints": "_manim_arc",
    "Brace": "_manim_brace",
    "BraceBetweenPoints": "_manim_brace",
    "BraceLabel": "_manim_brace",
    "BraceText": "_manim_brace",
    "Arrow": "_manim_arrow",
    "Vector": "_manim_arrow",
    "DoubleArrow": "_manim_arrow",
    "ArrowVectorField": "_manim_arrow",
    "Elbow": "_manim_shared_geometry",
    "RoundedRectangle": "_manim_shared_geometry",
    "SurroundingRectangle": "_manim_shared_geometry",
    "BackgroundRectangle": "_manim_shared_geometry",
    "Cross": "_manim_shared_geometry",
    "Underline": "_manim_shared_geometry",
    "Union": "_manim_shared_geometry",
    "Intersection": "_manim_shared_geometry",
    "Difference": "_manim_shared_geometry",
    "Exclusion": "_manim_shared_geometry",

    "AnnularSector": "_manim_shared_geometry",
    "Sector": "_manim_shared_geometry",
    "Annulus": "_manim_shared_geometry",
    "Dot": "_manim_geometry",
    "AnnotationDot": "_manim_geometry",
    "Ellipse": "_manim_geometry",
    "Triangle": "_manim_geometry",
    "Polygram": "_manim_polygon",
    "Polygon": "_manim_polygon",
    "RegularPolygram": "_manim_polygon",
    "RegularPolygon": "_manim_polygon",
    "Star": "_manim_polygon",
    "Angle": "_manim_angle",
    "RightAngle": "_manim_angle",
    "TangentLine": "_manim_geometry",
    "ApplyMethod": "_manim_geometry",
    "DEFAULT_DOT_RADIUS": "_manim_geometry",
    "PURE_YELLOW": "_manim_geometry",
    "Text": "_manim_typst",
    "MarkupText": "_manim_typst",
    "NativeFontFace": "_manim_typst",
    "Typst": "_manim_typst",
    "MathTypst": "_manim_typst",
    "Table": "_manim_table",
    "Code": "_manim_code",
    "MathTable": "_manim_table",
    "IntegerTable": "_manim_table",
    "DecimalTable": "_manim_table",
    "MobjectTable": "_manim_table",
    "Matrix": "_manim_matrix",
    "IntegerMatrix": "_manim_matrix",
    "DecimalMatrix": "_manim_matrix",
    "MobjectMatrix": "_manim_matrix",
    "DecimalNumber": "_manim_numbers",
    "Integer": "_manim_numbers",
    "Variable": "_manim_numbers",
    "Tex": "_manim_latex",
    "MathTex": "_manim_latex",
    "SingleStringMathTex": "_manim_latex",
    "prepare_latex": "_manim_latex",
}


def __getattr__(name: str):
    module = _PUBLIC_EXPORTS.get(name)
    if module is not None:
        from importlib import import_module
        value = getattr(import_module(module), name)
        globals()[name] = value
        return value
    raise AttributeError(f"module {__name__!r} has no attribute {name!r}")


def __dir__():
    return sorted(set(globals()) | set(__all__))


__all__ = [
    "annotations",
    "PackageNotFoundError",
    "version",
    "Glow",
    "GlowPulse",
    "Pixels",
    "EffectHandle",
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
    "MultiAnimationOverrideException",
    "override_animation",

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
    "always_redraw",
    "SMALL_BUFF",
    "MED_SMALL_BUFF",
    "MED_LARGE_BUFF",
    "LARGE_BUFF",
    *_PUBLIC_EXPORTS,
]
