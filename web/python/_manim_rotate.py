"""ManimCE-compatible deterministic procedural animations for the exact 2D subset.

``mobject.animate.rotate`` is target-state interpolation and intentionally remains a
regular Transform. Manim's explicit ``Rotate`` and ``Rotating`` instead follow
rotational paths. For geometry centered on its authored transform origin, Noon can
represent those paths exactly with the existing scalar rotation track, without adding
a new IR primitive. External pivots and non-z axes require curved translation/3D
support and are rejected until the runtime can represent them exactly.

``FocusOn`` is likewise deterministic: Manim transforms a transparent frame-sized Dot
into a zero-radius grey spotlight at the requested point and removes it at completion.
Shared Rust constructs and stages that spotlight with ordinary transform and membership
operations. Python keeps only the inert request and argument coercion.
"""

from __future__ import annotations

import math
from typing import Any

import noon as _base
import _manim_animate as _animate
import _manim_compat as _compat
import _manim_rate_functions as _rate_functions


_INSTALLED = False
_UNSUPPORTED_PATH_OPTIONS = {
    "path_arc",
    "path_arc_axis",
    "path_arc_centers",
    "path_func",
}


class Rotate:
    """Rotate one centered 2D mobject along Manim's procedural angular path."""

    def __init__(
        self,
        mobject: object,
        angle: float = math.pi,
        axis: object = _compat.OUT,
        about_point: object | None = None,
        about_edge: object | None = None,
        **kwargs: Any,
    ) -> None:
        if isinstance(mobject, _compat.Group):
            raise NotImplementedError(
                "Rotate(Group/VGroup) requires retained family pivot motion and is not yet supported"
            )
        if not isinstance(mobject, _base.Mobject):
            raise TypeError("Rotate target must be a Mobject")

        path_options = sorted(_UNSUPPORTED_PATH_OPTIONS.intersection(kwargs))
        if path_options:
            raise NotImplementedError(
                "Rotate path override(s) are not yet supported: " + ", ".join(path_options)
            )

        value = float(angle)
        if not math.isfinite(value):
            raise ValueError("Rotate angle must be finite")

        self.mobject = mobject
        self.angle = value
        self.axis = axis
        # ManimCE eagerly captures the default pivot in Rotate.__init__; do the same
        # rather than recomputing the center later when Scene.play is called.
        self.about_point = mobject.get_center() if about_point is None else about_point
        self.about_edge = about_edge
        self.anim_args = dict(kwargs)


class Rotating(Rotate):
    """ManimCE ``Rotating`` for the exact centered 2D subset.

    Unlike ``Rotate``, Manim's ``Rotating`` keeps ``about_point=None`` until frame
    interpolation and defaults to a full turn over five seconds with linear timing.
    The centered 2D case maps exactly to the same scalar rotation channel used by
    ``Rotate``; unsupported pivot/family/3D cases fail instead of being approximated.
    """

    def __init__(
        self,
        mobject: object,
        angle: float = math.tau,
        axis: object = _compat.OUT,
        about_point: object | None = None,
        about_edge: object | None = None,
        run_time: float = 5.0,
        rate_func: object = _rate_functions.linear,
        **kwargs: Any,
    ) -> None:
        if isinstance(mobject, _compat.Group):
            raise NotImplementedError(
                "Rotating(Group/VGroup) requires retained family pivot motion and is not yet supported"
            )
        if not isinstance(mobject, _base.Mobject):
            raise TypeError("Rotating target must be a Mobject")

        path_options = sorted(_UNSUPPORTED_PATH_OPTIONS.intersection(kwargs))
        if path_options:
            raise NotImplementedError(
                "Rotating path override(s) are not supported: " + ", ".join(path_options)
            )

        value = float(angle)
        if not math.isfinite(value):
            raise ValueError("Rotating angle must be finite")

        self.mobject = mobject
        self.angle = value
        self.axis = axis
        # ManimCE Rotating passes None through to Mobject.rotate, which resolves it
        # against the frame's current mobject center. Preserve that distinction from
        # Rotate, whose constructor eagerly captures the center.
        self.about_point = about_point
        self.about_edge = about_edge
        self.anim_args = dict(kwargs)
        self.anim_args["run_time"] = run_time
        self.anim_args["rate_func"] = rate_func


class FocusOn:
    """Shrink Manim's temporary frame-sized spotlight to one fixed 2D point.

    The supported subset is exact for explicit fixed points and for leaf Mobjects whose
    center stays fixed for the duration of the FocusOn play. Moving focus targets require
    updater semantics and remain outside this fixed-point request.
    """

    def __init__(
        self,
        focus_point: object,
        opacity: float = 0.2,
        color: _base.Color = _base.GREY,
        run_time: float = 2.0,
        **kwargs: Any,
    ) -> None:
        if isinstance(focus_point, _compat.Group):
            raise NotImplementedError(
                "FocusOn(Group/VGroup) requires retained family center semantics and is not yet supported"
            )
        if isinstance(focus_point, _base.Mobject):
            point = focus_point.get_center()
            self.focus_mobject: _base.Mobject | None = focus_point
        else:
            point = _base._as_vec2(focus_point)
            self.focus_mobject = None

        opacity_value = float(opacity)
        if not math.isfinite(opacity_value) or not 0.0 <= opacity_value <= 1.0:
            raise ValueError("FocusOn opacity must be finite and in [0, 1]")
        run_time_value = float(run_time)
        if not math.isfinite(run_time_value) or run_time_value <= 0.0:
            raise ValueError("FocusOn run_time must be finite and positive")

        if any(kwargs.get(name) is False for name in ("introducer", "remover")):
            raise NotImplementedError("FocusOn has fixed transient membership")
        if kwargs.get("lag_ratio", 0.0) != 0.0 or kwargs.get("reverse_rate_function", False):
            raise NotImplementedError("FocusOn does not support lag or rate reversal")
        if _UNSUPPORTED_PATH_OPTIONS.intersection(kwargs):
            raise NotImplementedError("FocusOn does not support path overrides")
        if not all(math.isfinite(float(value)) for value in point):
            raise ValueError("FocusOn point must be finite")
        self.focus_point = point
        self.opacity = opacity_value
        self.color = color
        self.anim_args = dict(kwargs)
        self.anim_args["run_time"] = run_time_value


def _axis_sign(axis: object) -> float:
    try:
        if len(axis) != 3:  # type: ignore[arg-type]
            raise NotImplementedError("rotation currently supports only 3D z-axis vectors")
        x = float(axis[0])  # type: ignore[index]
        y = float(axis[1])  # type: ignore[index]
        z = float(axis[2])  # type: ignore[index]
    except NotImplementedError:
        raise
    except (TypeError, ValueError, IndexError) as error:
        raise TypeError("rotation axis must be a three-component numeric vector") from error

    if not all(math.isfinite(value) for value in (x, y, z)):
        raise ValueError("rotation axis must be finite")
    if not math.isclose(x, 0.0, abs_tol=1e-12) or not math.isclose(
        y, 0.0, abs_tol=1e-12
    ) or math.isclose(z, 0.0, abs_tol=1e-12):
        raise NotImplementedError("rotation currently supports only the 2D OUT/IN z axis")
    return 1.0 if z > 0.0 else -1.0


def _points_close(left: object, right: object) -> bool:
    return all(math.isclose(float(a), float(b), abs_tol=1e-9) for a, b in zip(left, right))


def install() -> None:
    global _INSTALLED
    if _INSTALLED:
        return
    _INSTALLED = True

    public = {"Rotate": Rotate, "Rotating": Rotating, "FocusOn": FocusOn}
    for name, value in public.items():
        setattr(_base, name, value)
        setattr(_compat, name, value)
        setattr(_animate, name, value)

    exports = list(_base.__all__)
    for name in public:
        if name not in exports:
            exports.append(name)
    _base.__all__ = exports
