"""Partial ManimCE-compatible static SVG authoring over shared Rust resources.

Python owns file/string loading and wrapper identity. Shared Rust owns SVG parsing,
compatibility normalization, retained path resources, styles, family identity and
all later semantic mutations.
"""

from __future__ import annotations

from pathlib import Path
from typing import Any

import noon as _base
import _manim_compat as _compat
from _noon_errors import engine_call
from _manim_semantic_handles import (
    _attach_shared_handle,
    _family_wrapper_key,
    _live_constructor_context,
)

try:
    from js import noonCreateAuthoringSvgHandle as _create_svg_handle
except ImportError:  # Native CPython tests do not have the browser bridge.
    _create_svg_handle = None


def _read_svg_file(file_name: object) -> tuple[Path, str]:
    if file_name is None:
        raise ValueError("Must specify file for SVGMobject")
    try:
        path = Path(file_name)
    except TypeError as error:
        raise TypeError("SVGMobject file_name must be path-like") from error
    candidates = [path]
    if path.suffix == "":
        candidates.append(path.with_suffix(".svg"))
    for candidate in candidates:
        if candidate.is_file():
            return candidate, candidate.read_text(encoding="utf-8")
    raise FileNotFoundError(f"SVG file not found: {path}")


def _validate_parser_options(svg_default: object, path_string_config: object) -> None:
    if svg_default is not None:
        raise NotImplementedError(
            "custom SVGMobject svg_default requires shared Rust default-style configuration"
        )
    if path_string_config not in (None, {}):
        raise NotImplementedError(
            "SVGMobject path_string_config is not yet supported by retained SVG import"
        )


def _wrap_imported_family(owner: "SVGMobject", family: object) -> None:
    owner._semantic_family_handle = family
    wrappers: dict[str, _compat.VMobject] = {}
    count = int(engine_call(getattr, family, "memberCount"))
    for index in range(count):
        leaf = object.__new__(_compat.VMobject)
        _attach_shared_handle(leaf, engine_call(family.memberMobject, index))
        wrappers[_family_wrapper_key(leaf)] = leaf
    owner._semantic_member_wrappers = wrappers


class SVGMobject(_compat.VGroup):
    """Static SVG imported as one retained semantic family.

    Direct filesystem paths and :meth:`from_string` are supported in this partial
    slice. Browser URL/blob loading, non-default parser configuration, cache-policy
    compatibility and construction after live execution starts remain explicit gaps.
    """

    def __init__(
        self,
        file_name: object | None = None,
        *,
        should_center: bool = True,
        height: float | None = 2,
        width: float | None = None,
        color: object | None = None,
        opacity: float | None = None,
        fill_color: object | None = None,
        fill_opacity: float | None = None,
        stroke_color: object | None = None,
        stroke_opacity: float | None = None,
        stroke_width: float | None = None,
        svg_default: dict | None = None,
        path_string_config: dict | None = None,
        use_svg_cache: bool = True,
        _svg_string: str | None = None,
        **kwargs: Any,
    ) -> None:
        if _create_svg_handle is None:
            raise RuntimeError("SVGMobject construction requires the shared Rust authoring host")
        if kwargs:
            raise NotImplementedError(
                "unsupported SVGMobject constructor option(s): " + ", ".join(sorted(kwargs))
            )
        if _live_constructor_context("SVG") is not None:
            raise NotImplementedError(
                "live SVGMobject construction requires atomic retained-resource publication"
            )
        _validate_parser_options(svg_default, path_string_config)
        if not use_svg_cache:
            # Cache choice is not a visual semantic. Noon currently relies on the
            # shared immutable resource arena rather than Manim's wrapper cache.
            pass

        if _svg_string is None:
            self.file_name, source = _read_svg_file(file_name)
        else:
            if file_name is not None:
                raise TypeError("SVGMobject accepts either file_name or SVG source, not both")
            self.file_name = None
            source = str(_svg_string)
            if not source.strip():
                raise ValueError("SVG source must be non-empty")

        self.should_center = bool(should_center)
        self.svg_height = None if height is None else float(height)
        self.svg_width = None if width is None else float(width)
        self.color = color
        self.opacity = opacity
        self.fill_color = fill_color
        self.fill_opacity = fill_opacity
        self.stroke_color = stroke_color
        self.stroke_opacity = stroke_opacity
        self.stroke_width = 0 if stroke_width is None else float(stroke_width)
        self.svg_default = svg_default
        self.path_string_config = {} if path_string_config is None else path_string_config
        self.id_to_vgroup_dict = {}

        family = engine_call(
            _create_svg_handle,
            source,
            self.should_center,
            self.svg_height,
            self.svg_width,
        )
        _wrap_imported_family(self, family)

        # Manim v0.21 applies these explicit paint overrides after parsing. Its
        # top-level `color`/`opacity` fields are retained metadata here as there,
        # while fill/stroke-specific arguments mutate the imported family.
        if fill_color is not None or fill_opacity is not None:
            self.set_fill(fill_color, fill_opacity)
        if stroke_color is not None or stroke_opacity is not None or stroke_width is not None:
            self.set_stroke(stroke_color, stroke_width, stroke_opacity)

    @classmethod
    def from_string(cls, source: str, **kwargs: Any) -> "SVGMobject":
        """Import SVG text without introducing a frontend geometry model."""
        return cls(_svg_string=source, **kwargs)
