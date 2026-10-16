"""Polygon signatures and coercion over shared Rust retained geometry."""
import operator
import noon as _base
import _manim_compat as _compat
import _manim_semantic_handles as _shared
from _noon_errors import engine_call


def _count(name, value):
    if isinstance(value, bool):
        raise TypeError(f"{name} must be an integer")
    value = operator.index(value)
    if not 0 <= value <= 0xFFFFFFFF:
        raise ValueError(f"{name} must fit an unsigned 32-bit integer")
    return value


def _optional_number(name, value):
    return None if value is None else _base._ir._finite_number(name, value)


def _finish(self, method, args, kwargs):
    if _shared._create_geometry_handle is None:
        raise RuntimeError("Polygon construction requires the shared Rust authoring host")
    values = dict(kwargs)
    color = values.pop("color", None)
    candidate = engine_call(getattr(_shared._geometry_options, method), *args)
    resolved_start_angle = getattr(candidate, "resolvedStartAngle", None)
    _shared._apply_shared_constructor_options(candidate, values)
    if color is not None:
        _shared._apply_constructor_color(candidate, _compat._as_color("color", color))
    _shared._attach_geometry_options(self, candidate, type(self).__name__)
    return resolved_start_angle


class Polygram(_compat.VMobject):
    """Closed planar vertex groups; geometry is constructed by Rust."""
    def __init__(self, *vertex_groups, color=_base.BLUE, **kwargs):
        from pyodide.ffi import to_js
        groups = [[_base._as_vec2(point) for point in group] for group in vertex_groups]
        coordinates = to_js([component for group in groups for point in group for component in point])
        lengths = to_js([len(group) for group in groups])
        _finish(self, "polygram", (coordinates, lengths), dict(kwargs, color=color))

    def get_vertices(self):
        return self.get_start_anchors()

    def get_vertex_groups(self):
        from _manim_path_queries import vertex_groups
        return vertex_groups(self)


class Polygon(Polygram):
    def __init__(self, *vertices, **kwargs):
        super().__init__(vertices, **kwargs)


class RegularPolygram(Polygram):
    def __init__(self, num_vertices, *, density=2, radius=1, start_angle=None, **kwargs):
        resolved_start_angle = _finish(self, "regularPolygram", (
            _count("num_vertices", num_vertices), _count("density", density),
            _base._ir._finite_number("radius", radius), _optional_number("start_angle", start_angle),
        ), kwargs)
        self.start_angle = float(resolved_start_angle)


class RegularPolygon(RegularPolygram):
    def __init__(self, n=6, **kwargs):
        super().__init__(n, density=1, **kwargs)


class Star(Polygon):
    def __init__(self, n=5, *, outer_radius=1, inner_radius=None, density=2,
                 start_angle=_base.TAU / 4, **kwargs):
        resolved_start_angle = _finish(self, "star", (
            _count("n", n), _base._ir._finite_number("outer_radius", outer_radius),
            _optional_number("inner_radius", inner_radius), _count("density", density),
            _optional_number("start_angle", start_angle),
        ), kwargs)
        self.start_angle = float(resolved_start_angle)
