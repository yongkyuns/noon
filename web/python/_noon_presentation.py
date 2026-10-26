"""Immutable Noon presentation values supplied by the shared Rust API.

Python only adapts names/indices and forwards resolved defaults to ordinary Text.
There is no Python palette, text-style registry, scene state, or ambient theme.
"""
from operator import index as _index

import noon as _base
from _noon_errors import engine_call


def _name(value, kind):
    if not isinstance(value, str):
        raise TypeError(f"{kind} must be a string")
    return value


def _color(value):
    try:
        return _base.Color(float(value.red), float(value.green), float(value.blue), float(value.alpha))
    finally:
        value.free()


class Theme:
    """An explicit immutable theme; existing objects never inherit later changes.

    Use ``Theme.dark()`` or ``Theme.light()``. Color, spacing, category bounds and
    text-role defaults are resolved in Rust. ``text`` returns an ordinary Text,
    and explicit Text keyword arguments override only the supplied fields.
    """
    __slots__ = ("_definition",)

    def __init__(self, preset="dark"):
        if hasattr(self, "_definition"):
            raise AttributeError("Theme is immutable")
        preset = _name(preset, "theme preset")
        try:
            from js import noonTheme
        except ImportError as error:
            raise RuntimeError("Theme requires the shared Rust authoring host") from error
        object.__setattr__(self, "_definition", engine_call(noonTheme, preset))

    def __setattr__(self, name, value):
        raise AttributeError("Theme is immutable")

    def __delattr__(self, name):
        raise AttributeError("Theme is immutable")

    @classmethod
    def dark(cls):
        return cls("dark")

    @classmethod
    def light(cls):
        return cls("light")

    def __copy__(self):
        return self

    def __deepcopy__(self, memo):
        memo[id(self)] = self
        return self

    def color(self, role):
        """Resolve a semantic color role without changing any object."""
        return _color(engine_call(self._definition.color, _name(role, "color role")))

    def spacing(self, role):
        """Resolve a named gap in scene units."""
        return float(engine_call(self._definition.spacing, _name(role, "spacing role")))

    @property
    def categorical_count(self):
        """The finite palette length; category lookup never silently cycles."""
        return int(self._definition.categoricalCount)

    def categorical(self, index):
        if isinstance(index, bool):
            raise TypeError("categorical color index must be an integer, not a boolean")
        # __index__ is Python argument coercion. The WASM boundary and shared
        # Theme perform numeric-range and finite-palette admission.
        try:
            value = float(_index(index))
        except OverflowError as error:
            raise ValueError("categorical color index is outside the host numeric range") from error
        return _color(engine_call(self._definition.categorical, value))

    def text(self, source, *, role="body", **kwargs):
        """Construct ordinary Text using Rust defaults and explicit overrides."""
        style = engine_call(self._definition.textStyle, _name(role, "text role"))
        try:
            options = {
                "font": str(style.fontFamily),
                "font_size": float(style.fontSize),
                "line_spacing": float(style.lineSpacing),
                "color": _color(style.color),
            }
        finally:
            style.free()
        options.update(kwargs)
        return _base.Text(source, **options)
