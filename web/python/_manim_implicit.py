"""Static implicit contours; Rust owns adaptive sampling, topology and geometry."""
from operator import index

import _manim_plotting as _plot
from _noon_errors import engine_call

try:
    from pyodide.ffi import create_proxy as _create_proxy
except ImportError:
    _create_proxy = None


def _integer(value, name):
    if isinstance(value, bool):
        raise TypeError(name + " must be an integer, not bool")
    return index(value)


def _prepare(factory, function, args, min_depth, max_quads, use_smoothing):
    _plot._callable(function)
    if _create_proxy is None:
        raise RuntimeError("implicit curves require the shared Rust authoring host")
    depth = _integer(min_depth, "min_depth")
    quads = _integer(max_quads, "max_quads")
    failure = []

    def evaluate(x, y):
        try:
            return float(function(float(x), float(y)))
        except BaseException as error:
            failure.append(error)
            raise

    # Adaptive traversal invokes this scalar bridge synchronously during Rust
    # preparation. The proxy and callable never enter the retained scene.
    proxy = _create_proxy(evaluate)
    try:
        try:
            return engine_call(factory, proxy, *args, depth, quads, bool(use_smoothing))
        except BaseException:
            if failure:
                raise failure[0]
            raise
    finally:
        proxy.destroy()


class ImplicitFunction(_plot._compat.VMobject):
    """Retained zero contour of f(x, y), sampled once during construction."""

    def __init__(self, func, x_range=None, y_range=None, min_depth=5,
                 max_quads=1500, use_smoothing=True, color=None, **kwargs):
        _plot._callable(func)
        if _plot._shared._geometry_options is None:
            raise RuntimeError("implicit curves require the shared Rust authoring host")
        options = _prepare(
            _plot._shared._geometry_options.implicitPlot, func,
            (_plot._array(() if x_range is None else x_range),
             _plot._array(() if y_range is None else y_range)),
            min_depth, max_quads, use_smoothing,
        )
        _plot._curve(self, options, color, kwargs)


def plot_implicit_curve(axes, function, min_depth=5, max_quads=1500,
                        *, use_smoothing=True, color=None, **kwargs):
    _plot._callable(function)
    # The frame is captured before calling arbitrary host code and remains
    # fixed throughout the whole contour, just like ordinary static plots.
    with _plot._owned(axes._coordinate_frame()) as frame:
        options = _prepare(frame.implicitPlot, function, (), min_depth,
                           max_quads, use_smoothing)
    return _plot._curve(object.__new__(ImplicitFunction), options, color, kwargs)


__all__ = ["ImplicitFunction"]
