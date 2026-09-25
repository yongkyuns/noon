"""Tex syntax over the shared Rust compiler and an explicitly prepared host."""
from __future__ import annotations

import noon as _base
import _manim_compat as _compat
from _noon_errors import engine_call, engine_await
from _manim_typst import (
    _RetainedTextMobject, _validated_font_size, _validated_opacity,
    _as_color, _copy_text_parts, _live_text_context, TextSourcePart,
)

try:
    from js import (
        noonPrepareLatex as _prepare,
        noonCreateAuthoringLatexHandle as _create,
        noonCreateAuthoringLatexStringsHandle as _create_strings,
    )
except ImportError:
    _prepare = _create = _create_strings = None


async def prepare_latex():
    """Prepare the optional pinned real TeX compiler once in this authoring worker."""
    if _prepare is None:
        raise RuntimeError("LaTeX requires Noon's shared Rust authoring runtime")
    await engine_await(_prepare(), operation="prepare_latex")


def _source_slice(source: str, part: TextSourcePart) -> str:
    encoded = source.encode("utf-8")
    return encoded[part.source_start:part.source_end].decode("utf-8")


class MathTexPart(_RetainedTextMobject):
    """Ordinary retained text leaf for one compiler-authored source part."""

    def __init__(self, owner: _TexBase, part: TextSourcePart, handle: object):
        self._owner = owner
        self._part = part
        self._initialize_text(
            owner.source, getattr(owner, "_font_size", 48.0), handle,
            getattr(owner, "_initial_color", _base.WHITE),
            getattr(owner, "_initial_opacity", 1.0), presentation_applied=True,
        )

    @property
    def semantic_key(self) -> str | None:
        return self._part.semantic_key

    @property
    def tex_string(self) -> str:
        return _source_slice(self._owner.source, self._part)

    def get_tex_string(self) -> str:
        return self.tex_string

    def set_color(self, color: _base.Color, family: bool = True) -> MathTexPart:
        del family
        self._owner._set_part_view_colors(((self, _as_color(color)),))
        return self

    def __repr__(self) -> str:
        return f"MathTexPart({self.tex_string!r})"


class _TexBase(_compat.VGroup):
    _math_mode = False

    @property
    def source(self) -> str:
        if hasattr(self, "_source"):
            return self._source
        return str(engine_call(self._semantic_handle.textSource))

    @property
    def font_size(self) -> float:
        return float(engine_call(lambda: self._semantic_latex_handle.fontSize))

    @property
    def tex_string(self) -> str:
        return self.source

    def get_tex_string(self) -> str:
        return self.source

    def __init__(self, *sources: str, font_size=48.0, color=_base.WHITE, opacity=1.0, **kwargs):
        arg_separator = kwargs.pop("arg_separator", " " if self._math_mode else "")
        expected_separator = " " if self._math_mode else ""
        if arg_separator != expected_separator:
            raise NotImplementedError("custom Tex arg_separator is not yet supported")
        substrings = kwargs.pop("substrings_to_isolate", ())
        if substrings is None:
            substrings = ()
        if isinstance(substrings, str) or not all(isinstance(value, str) for value in substrings):
            raise TypeError("substrings_to_isolate must be an iterable of strings")
        if substrings:
            raise NotImplementedError(
                "substrings_to_isolate requires compiler-authored retained part publication"
            )
        color_map = kwargs.pop("tex_to_color_map", None)
        if color_map is not None and not hasattr(color_map, "items"):
            raise TypeError("tex_to_color_map must be a mapping")
        if kwargs:
            raise NotImplementedError(f"unsupported Tex option(s): {', '.join(sorted(kwargs))}")
        if not sources:
            raise TypeError("Tex requires at least one source string")
        if not all(isinstance(source, str) for source in sources):
            raise TypeError("Tex source must be a string")
        if _create is None:
            raise RuntimeError("LaTeX requires Noon's shared Rust authoring runtime")
        font_size = _validated_font_size(font_size)
        opacity = _validated_opacity(opacity)
        color = _as_color(color)
        context = _live_text_context()
        create = _create if len(sources) == 1 else _create_strings
        if create is None:
            raise RuntimeError("LaTeX requires Noon's shared Rust authoring runtime")
        arguments = (sources[0],) if len(sources) == 1 else (list(sources),)
        handle = engine_call(
            create, *arguments, self._math_mode, font_size,
            float(color.red), float(color.green), float(color.blue), float(color.alpha),
            opacity, context,
        )
        if context is not None:
            self._canonical_live_target_context = context
        source = (" " if self._math_mode else "").join(sources)
        self._source = str(getattr(handle, "source", source))
        self._font_size = float(font_size)
        self._initial_color = color
        self._initial_opacity = opacity
        self._semantic_latex_handle = handle
        self._semantic_family_handle = engine_call(handle.family)
        self._semantic_member_wrappers = {}
        self._part_views()
        if color_map:
            self.set_color_by_tex_to_color_map(color_map)

    def text_parts(self) -> tuple[TextSourcePart, ...]:
        return tuple(member._part for member in self._part_views())

    def source_parts_for(self, needle: str) -> tuple[TextSourcePart, ...]:
        if not isinstance(needle, str):
            raise TypeError("text source-part needle must be a string")
        indices = tuple(engine_call(self._semantic_latex_handle.sourceMemberIndicesFor, needle))
        views = self._part_views()
        return tuple(views[int(index)]._part for index in indices)

    def _part_views(self) -> tuple[MathTexPart, ...]:
        views = []
        live = {}
        for member_handle in tuple(engine_call(self._semantic_latex_handle.members)):
            raw_parts = _copy_text_parts(engine_call(member_handle.textParts))
            if len(raw_parts) != 1:
                raise RuntimeError("retained TeX part leaf must expose one source part")
            key = f"{int(member_handle.semanticSlot)}:{int(member_handle.semanticGeneration)}"
            member = self._semantic_member_wrappers.get(key)
            if member is None:
                member = MathTexPart(self, raw_parts[0], member_handle)
            else:
                member._part = raw_parts[0]
                member._semantic_handle = member_handle
            live[key] = member
            views.append(member)
        self._semantic_member_wrappers = live
        return tuple(views)

    def _refresh_semantic_members(self) -> None:
        self._part_views()

    def copy(self):
        import _manim_semantic_handles as semantic

        clone = semantic._group_copy(self)
        clone._semantic_latex_handle = engine_call(
            self._semantic_latex_handle.rebindFamily,
            clone._semantic_family_handle,
        )
        clone._semantic_member_wrappers = {}
        clone._part_views()
        return clone

    def _set_part_view_colors(self, selections):
        views = self._part_views()
        by_identity = {id(member): index for index, member in enumerate(views)}
        colors = [None] * len(views)
        for member, color in selections:
            try:
                colors[by_identity[id(member)]] = _as_color(color)
            except KeyError as error:
                raise ValueError("MathTex part is no longer a current family member") from error
        values = []
        for color in colors:
            if color is None:
                values.extend((float("nan"), 0.0, 0.0, 0.0))
            else:
                values.extend((color.red, color.green, color.blue, color.alpha))
        context = getattr(self, "_canonical_live_target_context", None)
        if context is None:
            engine_call(self._semantic_latex_handle.setMemberColors, values)
        else:
            engine_call(context.liveSetLatexMemberColors, self._semantic_latex_handle, values)
        return self

    def __len__(self) -> int:
        return len(self.text_parts())

    def __iter__(self):
        return iter(self._part_views())

    def __getitem__(self, index):
        return self._part_views()[index]

    def get_part_by_tex(self, tex: str, **kwargs):
        if kwargs:
            raise NotImplementedError(
                "unsupported get_part_by_tex option(s): " + ", ".join(sorted(kwargs))
            )
        parts = self.get_parts_by_tex(tex)
        return None if not parts else parts[0]

    def get_parts_by_tex(self, tex: str, **kwargs) -> tuple[MathTexPart, ...]:
        if kwargs:
            raise NotImplementedError(
                "unsupported get_parts_by_tex option(s): " + ", ".join(sorted(kwargs))
            )
        indices = tuple(engine_call(self._semantic_latex_handle.sourceMemberIndicesFor, tex))
        parts = self._part_views()
        return tuple(parts[int(index)] for index in indices)

    def set_color_by_tex(self, tex: str, color: _base.Color, **kwargs):
        if kwargs:
            raise NotImplementedError(
                "unsupported set_color_by_tex option(s): " + ", ".join(sorted(kwargs))
            )
        parts = self.get_parts_by_tex(tex)
        self._set_part_view_colors(tuple((part, color) for part in parts))
        return self

    def set_color_by_tex_to_color_map(self, texs_to_color_map, **kwargs):
        if kwargs:
            raise NotImplementedError(
                "unsupported color-map option(s): " + ", ".join(sorted(kwargs))
            )
        if not hasattr(texs_to_color_map, "items"):
            raise TypeError("tex_to_color_map must be a mapping")
        selections = []
        for tex, color in texs_to_color_map.items():
            if not isinstance(tex, str):
                raise TypeError("tex_to_color_map keys must be strings")
            value = _as_color(color)
            selections.extend((part, value) for part in self.get_parts_by_tex(tex))
        self._set_part_view_colors(tuple(selections))
        return self

    def index_of_part(self, part: MathTexPart) -> int:
        if not isinstance(part, MathTexPart) or part._owner is not self:
            raise ValueError("Trying to get index of part not in MathTex")
        for index, candidate in enumerate(self._part_views()):
            if candidate is part:
                return index
        raise ValueError("Trying to get index of part not in MathTex")


class SingleStringMathTex(_TexBase):
    _math_mode = True

    def __init__(self, tex_string: str, **kwargs):
        super().__init__(tex_string, **kwargs)


class MathTex(SingleStringMathTex):
    def __init__(self, *sources: str, **kwargs):
        _TexBase.__init__(self, *sources, **kwargs)


class Tex(MathTex):
    _math_mode = False
