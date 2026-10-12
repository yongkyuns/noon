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

_PURE_COLORS = {
    "PURE_RED": 0xFF0000,
    "PURE_GREEN": 0x00FF00,
    "PURE_BLUE": 0x0000FF,
    "PURE_CYAN": 0x00FFFF,
    "PURE_MAGENTA": 0xFF00FF,
    "PURE_YELLOW": 0xFFFF00,
}


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
    """Return identifier names actually read by syntactically valid Python source."""

    tree = ast.parse(source, mode="exec")
    return {
        node.id
        for node in ast.walk(tree)
        if isinstance(node, ast.Name) and isinstance(node.ctx, ast.Load)
    }


def required_packages(source: str) -> tuple[str, ...]:
    """Return optional packages needed by implicit Manim namespace aliases."""

    names = _loaded_names(source)
    return tuple(
        package
        for alias, package in _OPTIONAL_NAMESPACE_PACKAGES.items()
        if alias in names
    )


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

    exports = list(_base.__all__)
    for alias, package in _OPTIONAL_NAMESPACE_PACKAGES.items():
        if alias not in _base.__dict__:
            setattr(_base, alias, _PendingOptionalNamespace(alias, package))
        if alias not in exports:
            exports.append(alias)

    for name, value in _PURE_COLORS.items():
        setattr(_base, name, _base.color_from_hex(value))
        if name not in exports:
            exports.append(name)

    _base.__all__ = exports
