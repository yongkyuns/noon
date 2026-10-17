"""Planar Angle signatures over Rust intersections, Arc paths and nominal values."""
import noon as _base
import _manim_compat as _compat
import _manim_semantic_handles as _shared
from _manim_arc import _finish_candidate, _require_geometry_host
from _manim_shared_geometry import _sector_component_count
from _noon_errors import engine_call, NoonError, raise_engine_error


class Angle(_compat.VMobject):
    def __init__(self, line1, line2, radius=None, quadrant=(1, 1), other_angle=False,
                 dot=False, dot_radius=None, dot_distance=0.55, dot_color=_base.WHITE,
                 elbow=False, **kwargs):
        _require_geometry_host("Angle")
        if not isinstance(line1, _compat.Line) or not isinstance(line2, _compat.Line):
            raise TypeError("Angle requires two Line objects")
        if dot and not elbow:
            raise NotImplementedError("Angle dot submobjects require shared VMobject child geometry")
        quadrant = tuple(quadrant)
        if len(quadrant) != 2:
            raise ValueError("Angle quadrant requires two values")
        from pyodide.ffi import to_js
        points = [line1.get_start(), line1.get_end(), line2.get_start(), line2.get_end()]
        values = dict(kwargs)
        components = _sector_component_count(values.pop("num_components", 9))
        coordinates = to_js([coordinate for point in points for coordinate in point])
        radius_value = (
            None if radius is None else _base._ir._finite_number("radius", radius)
        )
        candidate = engine_call(_shared._geometry_options.angle,
            coordinates,
            radius_value,
            _base._ir._finite_number("quadrant[0]", quadrant[0]),
            _base._ir._finite_number("quadrant[1]", quadrant[1]),
            bool(other_angle), bool(elbow), components)
        explicit_radius_attribute = getattr(candidate, "explicitAngleRadius", None)
        _finish_candidate(self, candidate, type(self).__name__, values)
        if explicit_radius_attribute is not None:
            # Rust omits this when parallel lines produce Manim's empty Angle.
            self.radius = float(explicit_radius_attribute)
        self.lines = (line1, line2)
        self.quadrant = quadrant
        self.elbow = bool(elbow)
        self.dot_distance = dot_distance

    @staticmethod
    def from_three_points(A, B, C, **kwargs):
        """Construct the angle ABC through shared Line and Angle semantics."""
        return Angle(_compat.Line(B, A), _compat.Line(B, C), **kwargs)

    def get_lines(self):
        return _compat.VGroup(*self.lines)

    def get_value(self, degrees=False):
        handle = _shared._handle_for(self)
        if handle is None:
            raise RuntimeError("Angle observation requires the shared Rust authoring host")
        try:
            return float(engine_call(handle.angleValue, bool(degrees)))
        except NoonError as error:
            if error.code == "angle.value_missing":
                raise AttributeError("Angle elbow has no angle_value") from error
            raise

    @property
    def angle_value(self):
        return self.get_value()

    @angle_value.setter
    def angle_value(self, value):
        radians = _shared._ir._finite_number("angle_value", value)
        context = _shared._live_mutation_context(self)
        handle = _shared._handle_for(self)
        if handle is None and context is not None:
            handle = getattr(self, "_semantic_handle", None)
        if handle is None:
            raise RuntimeError("Angle value mutation requires a current shared Rust handle")
        try:
            if context is not None:
                engine_call(context.liveSetAngleValue, handle, radians)
            else:
                engine_call(handle.setAngleValue, radians)
        except Exception as error:
            raise_engine_error(error)


class RightAngle(Angle):
    def __init__(self, line1, line2, length=None, **kwargs):
        super().__init__(line1, line2, radius=length, elbow=True, **kwargs)
