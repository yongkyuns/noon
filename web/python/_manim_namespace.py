"""ManimCE top-level namespace parity with lazy browser dependencies.

Manim exposes a few names from ``from manim import *`` that are not ordinary Manim
objects. In particular, its package ``__init__`` imports NumPy as ``np``. Loading NumPy
unconditionally in Pyodide would put a large optional dependency on every authoring
startup, so this module separates namespace identity from asynchronous package loading.

The JavaScript worker asks :func:`required_packages_json` before executing user source,
loads only the returned Pyodide packages, then calls :func:`bind_loaded_packages_json`.
Python never substitutes a partial implementation for an optional dependency.
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
    """Explicit placeholder replaced by the worker before user code executes."""

    __slots__ = ("alias", "package")

    def __init__(self, alias: str, package: str) -> None:
        self.alias = alias
        self.package = package

    def __getattr__(self, name: str) -> Any:
        raise RuntimeError(
            f"Manim namespace alias {self.alias!r} requires optional package "
            f"{self.package!r}; the browser authoring worker did not load it before access"
        )

    def __repr__(self) -> str:
        return f"<pending Manim namespace {self.alias!r} -> {self.package!r}>"


def _loaded_names(source: str) -> set[str]:
    """Return identifier names read by the source using only the stdlib AST parser."""

    tree = ast.parse(source, mode="exec")
    return {
        node.id
        for node in ast.walk(tree)
        if isinstance(node, ast.Name) and isinstance(node.ctx, ast.Load)
    }


def required_packages(source: str) -> tuple[str, ...]:
    """Return optional packages required by implicit Manim namespace aliases."""

    names = _loaded_names(source)
    return tuple(
        package
        for alias, package in _OPTIONAL_NAMESPACE_PACKAGES.items()
        if alias in names
    )


def required_packages_json(source: str) -> str:
    return json.dumps(required_packages(source), separators=(",", ":"))


def missing_packages(packages: list[str] | tuple[str, ...]) -> tuple[str, ...]:
    """Return requested packages that are not already importable in this interpreter."""

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
    """Bind real imported modules for aliases whose Pyodide packages are loaded."""

    requested = set(packages)
    for alias, package in _OPTIONAL_NAMESPACE_PACKAGES.items():
        if package not in requested:
            continue
        module = importlib.import_module(package)
        setattr(_base, alias, module)


def bind_loaded_packages_json(packages_json: str) -> None:
    value = json.loads(packages_json)
    if not isinstance(value, list) or not all(isinstance(item, str) for item in value):
        raise TypeError("loaded optional package list must be a JSON string array")
    bind_loaded_packages(value)


def install() -> None:
    """Install lightweight namespace names without importing optional packages."""

    exports = list(_base.__all__)
    for alias, package in _OPTIONAL_NAMESPACE_PACKAGES.items():
        if not hasattr(_base, alias):
            setattr(_base, alias, _PendingOptionalNamespace(alias, package))
        if alias not in exports:
            exports.append(alias)

    for name, value in _PURE_COLORS.items():
        setattr(_base, name, _base.color_from_hex(value))
        if name not in exports:
            exports.append(name)

    _base.__all__ = exports
