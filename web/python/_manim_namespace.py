"""ManimCE top-level namespace parity with lazy browser dependencies.

Manim exposes a few names from star imports that are not ordinary scene objects.
NumPy's np alias is the first optional dependency. Loading NumPy unconditionally
in Pyodide would put it on every authoring startup path, so the worker detects
implicit alias use before user execution, loads only the required package, and
then replaces the pending alias with the real module.
"""

from __future__ import annotations

import ast
import importlib
import importlib.util
import json
from typing import Any

import noon as _base


_OPTIONAL_NAMESPACE_PACKAGES = {
    "np": "numpy",
}

_OPTIONAL_HELPER_PACKAGES = {
    "rate_functions": "numpy",
    "get_winding_number": "numpy",
    "bezier": "numpy",
    "partial_bezier_points": "numpy",
    "split_bezier": "numpy",
    "subdivide_bezier": "numpy",
    "bezier_remap": "numpy",
    "point_lies_on_bezier": "numpy",
    "proportions_along_bezier_curve_for_point": "numpy",
    "line_intersection": "numpy",
    "find_intersection": "numpy",
    "quaternion_from_angle_axis": "numpy",
    "angle_axis_from_quaternion": "numpy",
    "quaternion_conjugate": "numpy",
    "thick_diagonal": "numpy",
    "rotation_about_z": "numpy",
    "rotation_matrix": "scipy",
    "cartesian_to_spherical": "numpy",
    "spherical_to_cartesian": "numpy",
    "complex_to_R3": "numpy",
    "complex_func_to_R3_func": "numpy",
    "center_of_mass": "numpy",
    "midpoint": "numpy",
    "normalize": "numpy",
    "angle_of_vector": "numpy",
    "angle_between_vectors": "numpy",
    "shoelace": "numpy",
    "shoelace_direction": "numpy",
    "binary_search": "numpy",
    "sigmoid": "numpy",
    "slow_into": "numpy",
    "double_smooth": "numpy",
    "inverse_interpolate": "numpy",
    "match_interpolate": "numpy",
    "stretch_array_to_length": "numpy",
    "resize_array": "numpy",
    "resize_preserving_order": "numpy",
    "resize_with_interpolation": "numpy",
}

# Literal values pinned to ManimCommunity/manim@861cd4849b17db1db3515b531ffe80b297848f93.
_MANIM_COLORS = {
    "WHITE": 0xFFFFFF,
    "GRAY_A": 0xDDDDDD,
    "GREY_A": 0xDDDDDD,
    "GRAY_B": 0xBBBBBB,
    "GREY_B": 0xBBBBBB,
    "GRAY_C": 0x888888,
    "GREY_C": 0x888888,
    "GRAY_D": 0x444444,
    "GREY_D": 0x444444,
    "GRAY_E": 0x222222,
    "GREY_E": 0x222222,
    "BLACK": 0x000000,
    "LIGHTER_GRAY": 0xDDDDDD,
    "LIGHTER_GREY": 0xDDDDDD,
    "LIGHT_GRAY": 0xBBBBBB,
    "LIGHT_GREY": 0xBBBBBB,
    "GRAY": 0x888888,
    "GREY": 0x888888,
    "DARK_GRAY": 0x444444,
    "DARK_GREY": 0x444444,
    "DARKER_GRAY": 0x222222,
    "DARKER_GREY": 0x222222,
    "PURE_RED": 0xFF0000,
    "PURE_GREEN": 0x00FF00,
    "PURE_BLUE": 0x0000FF,
    "PURE_CYAN": 0x00FFFF,
    "PURE_MAGENTA": 0xFF00FF,
    "PURE_YELLOW": 0xFFFF00,
    "BLUE_A": 0xC7E9F1,
    "BLUE_B": 0x9CDCEB,
    "BLUE_C": 0x58C4DD,
    "BLUE_D": 0x29ABCA,
    "BLUE_E": 0x236B8E,
    "BLUE": 0x58C4DD,
    "DARK_BLUE": 0x236B8E,
    "TEAL_A": 0xACEAD7,
    "TEAL_B": 0x76DDC0,
    "TEAL_C": 0x5CD0B3,
    "TEAL_D": 0x55C1A7,
    "TEAL_E": 0x49A88F,
    "TEAL": 0x5CD0B3,
    "GREEN_A": 0xC9E2AE,
    "GREEN_B": 0xA6CF8C,
    "GREEN_C": 0x83C167,
    "GREEN_D": 0x77B05D,
    "GREEN_E": 0x699C52,
    "GREEN": 0x83C167,
    "YELLOW_A": 0xFFF1B6,
    "YELLOW_B": 0xFFEA94,
    "YELLOW_C": 0xF7D96F,
    "YELLOW_D": 0xF4D345,
    "YELLOW_E": 0xE8C11C,
    "YELLOW": 0xF7D96F,
    "GOLD_A": 0xF7C797,
    "GOLD_B": 0xF9B775,
    "GOLD_C": 0xF0AC5F,
    "GOLD_D": 0xE1A158,
    "GOLD_E": 0xC78D46,
    "GOLD": 0xF0AC5F,
    "RED_A": 0xF7A1A3,
    "RED_B": 0xFF8080,
    "RED_C": 0xFC6255,
    "RED_D": 0xE65A4C,
    "RED_E": 0xCF5044,
    "RED": 0xFC6255,
    "MAROON_A": 0xECABC1,
    "MAROON_B": 0xEC92AB,
    "MAROON_C": 0xC55F73,
    "MAROON_D": 0xA24D61,
    "MAROON_E": 0x94424F,
    "MAROON": 0xC55F73,
    "PURPLE_A": 0xCAA3E8,
    "PURPLE_B": 0xB189C6,
    "PURPLE_C": 0x9A72AC,
    "PURPLE_D": 0x715582,
    "PURPLE_E": 0x644172,
    "PURPLE": 0x9A72AC,
    "PINK": 0xD147BD,
    "LIGHT_PINK": 0xDC75CD,
    "ORANGE": 0xFF862F,
    "LIGHT_BROWN": 0xCD853F,
    "DARK_BROWN": 0x8B4513,
    "GRAY_BROWN": 0x736357,
    "GREY_BROWN": 0x736357,
    "LOGO_WHITE": 0xECE7E2,
    "LOGO_GREEN": 0x87C2A5,
    "LOGO_BLUE": 0x525893,
    "LOGO_RED": 0xE07A5F,
    "LOGO_BLACK": 0x343434,
}

_MANIM_CONSTANTS = {
    "NORMAL": "NORMAL",
    "ITALIC": "ITALIC",
    "OBLIQUE": "OBLIQUE",
    "BOLD": "BOLD",
    "THIN": "THIN",
    "ULTRALIGHT": "ULTRALIGHT",
    "LIGHT": "LIGHT",
    "SEMILIGHT": "SEMILIGHT",
    "BOOK": "BOOK",
    "MEDIUM": "MEDIUM",
    "SEMIBOLD": "SEMIBOLD",
    "ULTRABOLD": "ULTRABOLD",
    "HEAVY": "HEAVY",
    "ULTRAHEAVY": "ULTRAHEAVY",
    "DEFAULT_DOT_RADIUS": 0.08,
    "DEFAULT_SMALL_DOT_RADIUS": 0.04,
    "DEFAULT_DASH_LENGTH": 0.05,
    "DEFAULT_ARROW_TIP_LENGTH": 0.35,
    "DEFAULT_POINTWISE_FUNCTION_RUN_TIME": 3.0,
    "DEFAULT_WAIT_TIME": 1.0,
    "DEFAULT_POINT_DENSITY_2D": 25,
    "DEFAULT_POINT_DENSITY_1D": 10,
    "DEFAULT_STROKE_WIDTH": 4,
    "DEFAULT_FONT_SIZE": 48,
    "SCALE_FACTOR_PER_FONT_POINT": 0.0010416666666666667,
}

_AXIS_ALIASES = {"X_AXIS": "RIGHT", "Y_AXIS": "UP", "Z_AXIS": "OUT"}


class _PendingOptionalNamespace:
    """Placeholder replaced by the authoring worker before user code executes."""

    __slots__ = ("alias", "package")

    def __init__(self, alias: str, package: str) -> None:
        self.alias = alias
        self.package = package

    def __getattr__(self, name: str) -> Any:
        raise RuntimeError(
            f"Manim namespace alias {self.alias!r} requires optional package "
            f"{self.package!r}; the authoring host did not load it before access"
        )

    def __repr__(self) -> str:
        return f"<pending Manim namespace {self.alias!r} -> {self.package!r}>"


def _loaded_names(source: str) -> set[str]:
    """Collect loaded names, attribute access and explicit namespace imports."""

    tree = ast.parse(source, mode="exec")
    names = {
        node.id
        for node in ast.walk(tree)
        if isinstance(node, ast.Name) and isinstance(node.ctx, ast.Load)
    }
    names.update(node.attr for node in ast.walk(tree) if isinstance(node, ast.Attribute))
    names.update(alias.name for node in ast.walk(tree)
                 if isinstance(node, ast.ImportFrom) and node.module in {"manim", "noon", "_manim_iterables", "_manim_simple_functions"}
                 for alias in node.names)
    return names


def required_packages(source: str) -> tuple[str, ...]:
    """Return packages needed by optional namespace aliases and data helpers."""

    names = _loaded_names(source)
    return tuple(dict.fromkeys(
        package
        for alias, package in {**_OPTIONAL_NAMESPACE_PACKAGES, **_OPTIONAL_HELPER_PACKAGES}.items()
        if alias in names
    ))


def required_packages_json(source: str) -> str:
    return json.dumps(required_packages(source), separators=(",", ":"))


def missing_packages(packages: list[str] | tuple[str, ...]) -> tuple[str, ...]:
    """Return requested packages that are not importable in this interpreter."""

    return tuple(
        package
        for package in packages
        if importlib.util.find_spec(package) is None
    )


def missing_packages_json(packages_json: str) -> str:
    value = json.loads(packages_json)
    if not isinstance(value, list) or not all(isinstance(item, str) for item in value):
        raise TypeError("optional package list must be a JSON string array")
    return json.dumps(missing_packages(value), separators=(",", ":"))


def bind_loaded_packages(packages: list[str] | tuple[str, ...]) -> None:
    """Replace pending aliases with the corresponding real imported modules."""

    requested = set(packages)
    for alias, package in _OPTIONAL_NAMESPACE_PACKAGES.items():
        if package not in requested:
            continue
        setattr(_base, alias, importlib.import_module(package))


def bind_loaded_packages_json(packages_json: str) -> None:
    value = json.loads(packages_json)
    if not isinstance(value, list) or not all(isinstance(item, str) for item in value):
        raise TypeError("loaded optional package list must be a JSON string array")
    bind_loaded_packages(value)


def install() -> None:
    """Install lightweight namespace names without importing optional packages."""

    exports = list(dict.fromkeys(_base.__all__))
    for alias, package in _OPTIONAL_NAMESPACE_PACKAGES.items():
        if alias not in _base.__dict__:
            setattr(_base, alias, _PendingOptionalNamespace(alias, package))
        if alias not in exports:
            exports.append(alias)

    for name, value in _MANIM_COLORS.items():
        if name not in _base.__dict__:
            setattr(_base, name, _base.color_from_hex(value))
        if name not in exports:
            exports.append(name)

    for name, value in _MANIM_CONSTANTS.items():
        setattr(_base, name, value)
        if name not in exports:
            exports.append(name)

    for name, source in _AXIS_ALIASES.items():
        setattr(_base, name, getattr(_base, source))
        if name not in exports:
            exports.append(name)

    _base.__all__ = exports
