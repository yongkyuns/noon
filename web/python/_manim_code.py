"""Manim-shaped Code authoring over Noon’s shared Rust code and text pipeline."""
from collections.abc import Mapping
from numbers import Real
from operator import index as _index
from pathlib import Path

from _noon_errors import engine_call
import noon as _base
import _manim_compat as _compat
from _manim_semantic_handles import _attach_shared_family, _attach_shared_handle, _live_constructor_context
from _manim_typst import NativeFontFace, Text

try:
    from js import noonCodeOptions as _new_code_options
    from js import noonCreateAuthoringCodeHandle as _create_code
    from js import noonCodeFromFamily as _code_from_family
    from js import noonCodeStylesList as _code_styles_list
    from js import noonCodeLanguagesList as _code_languages_list
    from js import noonCodeHighlightOptions as _new_highlight_options
    from js import noonGetHighlightedCodeLines as _get_highlighted_lines
    from js import noonHighlightCodeLines as _highlight_lines
    from js import noonReplaceCodeLine as _replace_line
except ImportError:
    _new_code_options = _create_code = _code_from_family = None
    _code_styles_list = _code_languages_list = None
    _new_highlight_options = _get_highlighted_lines = _highlight_lines = None
    _replace_line = None


def _integer(name, value):
    if isinstance(value, bool):
        raise TypeError(f"{name} must be an integer")
    try:
        return _index(value)
    except TypeError as error:
        raise TypeError(f"{name} must be an integer") from error


def _number(name, value):
    if isinstance(value, bool) or not isinstance(value, Real):
        raise TypeError(f"{name} must be a number")
    try:
        return float(value)
    except OverflowError as error:
        raise ValueError(f"{name} is outside the host numeric range") from error


def _config(name, defaults, supplied, allowed):
    if supplied is not None and not isinstance(supplied, Mapping):
        raise TypeError(f"{name} must be a mapping")
    result = {**defaults, **({} if supplied is None else supplied)}
    unknown = set(result) - allowed
    if unknown:
        raise NotImplementedError(f"unsupported {name} option(s): " + ", ".join(sorted(unknown)))
    return result


def _handle_key(handle):
    return f"{int(handle.semanticSlot)}:{int(handle.semanticGeneration)}"


def _colors(name, supplied):
    if supplied is None:
        return ()
    if not isinstance(supplied, Mapping):
        raise TypeError(f"{name} must be a mapping")
    result = []
    for selector, value in supplied.items():
        if not isinstance(selector, str):
            raise TypeError(f"{name} keys must be strings")
        result.append((selector, _compat._as_color(name, value)))
    return result


class Code(_compat.VGroup):
    """Retained, syntax-colored source lines with native tab and gutter layout.

    Indentation and empty lines are preserved. This display object does not
    format source code. Use ``get_languages_list`` and ``get_styles_list`` for
    the shared engine’s supported language and theme profiles.

    ``reserved_source`` measures an alternate source with the same number of
    rows before fitting. Later ``replace_line`` calls use the allocated layout.
    ``syntax_colors`` overrides foreground/comment/keyword/number/string/operator
    colors. ``token_colors`` selects whole identifiers or dotted identifier paths
    outside comments and strings; omitted colors use the named theme.
    """

    default_paragraph_config = {
        "font": "Monospace", "font_size": 24, "line_spacing": 0.5,
        "disable_ligatures": True,
    }
    default_background_config = {
        "buff": 0.3, "fill_color": None, "stroke_color": _base.WHITE,
        "corner_radius": 0.2, "stroke_width": 1, "fill_opacity": 1,
        "stroke_opacity": 1,
    }

    def __init__(
        self, code_file=None, code_string=None, language=None,
        formatter_style="vim", tab_width=4, add_line_numbers=True,
        line_numbers_from=1, background="rectangle", background_config=None,
        paragraph_config=None, *, reserved_source=None, syntax_colors=None,
        token_colors=None,
    ):
        if _create_code is None or _new_code_options is None:
            raise RuntimeError("Code requires Noon’s shared Rust authoring runtime")
        if code_file is not None:
            # Filesystem access is a host-language input boundary. All source
            # normalization, lexing, theme selection and layout stay in Rust.
            path = Path(code_file)
            source = path.read_text(encoding="utf-8")
            source_path = str(path)
        elif code_string is not None:
            if not isinstance(code_string, str):
                raise TypeError("code_string must be a string")
            source, source_path = code_string, None
        else:
            raise ValueError("Code requires code_string or code_file")
        if language is not None and not isinstance(language, str):
            raise TypeError("language must be a string or None")
        if not isinstance(formatter_style, str):
            raise NotImplementedError("Code uses native named themes; Python Pygments Style classes are unsupported")
        if not isinstance(add_line_numbers, bool):
            raise TypeError("add_line_numbers must be a bool")
        if not isinstance(background, str):
            raise TypeError("background must be a string")
        if reserved_source is not None and not isinstance(reserved_source, str):
            raise TypeError("reserved_source must be a string or None")
        syntax_colors = _colors("syntax_colors", syntax_colors)
        token_colors = _colors("token_colors", token_colors)
        tab_width = _integer("tab_width", tab_width)
        line_numbers_from = _integer("line_numbers_from", line_numbers_from)
        paragraph = _config("paragraph_config", self.default_paragraph_config, paragraph_config,
                            {"font", "font_size", "line_spacing", "disable_ligatures", "color"})
        panel = _config("background_config", self.default_background_config, background_config,
                        {"buff", "corner_radius", "fill_color", "fill_opacity",
                         "stroke_color", "stroke_width", "stroke_opacity"})
        font = paragraph["font"]
        font_face = font if isinstance(font, NativeFontFace) else None
        font_family = font_face.family if font_face is not None else font
        if not isinstance(font_family, str):
            raise TypeError("paragraph_config font must be a string or NativeFontFace")
        if not isinstance(paragraph["disable_ligatures"], bool):
            raise TypeError("disable_ligatures must be a bool")
        font_size, line_spacing = float(paragraph["font_size"]), float(paragraph["line_spacing"])
        stroke = _compat._as_color("stroke_color", panel["stroke_color"])
        fill = panel["fill_color"]
        if fill is not None:
            fill = _compat._as_color("fill_color", fill)
        context = _live_constructor_context("Code", allow_unstarted=True)
        options = engine_call(_new_code_options, language, source_path, formatter_style,
                              tab_width, add_line_numbers, line_numbers_from, background)
        try:
            engine_call(options.setParagraph, font_family, font_size, line_spacing,
                        paragraph["disable_ligatures"],
                        None if font_face is None else font_face._clone_for_text())
            engine_call(options.setBackgroundGeometry, float(panel["buff"]), float(panel["corner_radius"]))
            engine_call(options.setBackgroundOpacity, float(panel["fill_opacity"]))
            if fill is not None:
                engine_call(options.setBackgroundFill, fill.red, fill.green, fill.blue, fill.alpha)
            engine_call(options.setBackgroundStroke, stroke.red, stroke.green, stroke.blue, stroke.alpha,
                        float(panel["stroke_width"]) * 0.01, float(panel["stroke_opacity"]))
            if reserved_source is not None:
                engine_call(options.setReservedSource, reserved_source)
            for selector, value in syntax_colors:
                engine_call(options.setSyntaxColor, selector, value.red, value.green,
                            value.blue, value.alpha)
            for selector, value in token_colors:
                engine_call(options.setTokenColor, selector, value.red, value.green,
                            value.blue, value.alpha)
        except BaseException:
            options.free()
            raise
        # The WASM constructor consumes options even on a rejected admission.
        # Do not free its moved wrapper a second time.
        handle = engine_call(_create_code, source, options, context)
        self._code_font = font_family
        self._code_font_face = font_face
        self._code_font_size = font_size
        self._code_line_spacing = line_spacing
        self._initialize_code(handle, context)

    def _initialize_code(self, handle, context):
        self._code_handle = handle
        _attach_shared_family(self, engine_call(handle.family), context)
        known = self._semantic_member_wrappers
        self.background = known[_handle_key(engine_call(handle.background))]
        self.highlights = known[_handle_key(engine_call(handle.highlights))]
        self.code_lines = known[_handle_key(engine_call(handle.codeLines))]
        numbers = engine_call(handle.lineNumbers)
        if numbers is not None:
            self.line_numbers = known[_handle_key(numbers)]
            self._text_wrappers(self.line_numbers, engine_call(handle.lineNumberSources), context)
        else:
            self.__dict__.pop("line_numbers", None)
        self._text_wrappers(self.code_lines, engine_call(handle.lineSources), context)

    def _text_wrappers(self, family, sources, context):
        # Observe Rust source rows once. Python only gives the returned handles
        # their public Text wrapper class; it neither calculates rows nor places
        # glyphs. Existing wrappers survive copy rehydration and preserve aliases.
        members, sources = family.submobjects, list(sources)
        if len(members) != len(sources):
            raise RuntimeError("Code line handles and source observations disagree")
        for member, source in zip(members, sources):
            wrapper = member if isinstance(member, Text) else object.__new__(Text)
            font_size = getattr(wrapper, "_font_size", self._code_font_size)
            wrapper._initialize_text(str(source), font_size, member._semantic_handle,
                                     _base.WHITE, 1.0, presentation_applied=True)
            wrapper._font = getattr(wrapper, "_font", self._code_font)
            wrapper._font_face = getattr(wrapper, "_font_face", self._code_font_face)
            wrapper._line_spacing = getattr(wrapper, "_line_spacing", self._code_line_spacing)
            if context is not None:
                wrapper._canonical_live_target_context = context
            family._semantic_member_wrappers[_handle_key(member._semantic_handle)] = wrapper

    def _rehydrate_semantic_family_handle(self):
        if _code_from_family is None:
            raise RuntimeError("Code requires Noon’s shared Rust authoring runtime")
        self._initialize_code(engine_call(_code_from_family, self._semantic_family_handle),
                              getattr(self, "_canonical_live_target_context", None))

    @property
    def code_string(self):
        """The current normalized source as observed from retained Rust rows."""
        return "\n".join(str(source) for source in engine_call(self._code_handle.lineSources))

    def replace_line(self, index, source):
        """Replace one source row without reflow, returning ``(previous, current)``.

        ``reserved_source`` can reserve a larger same-row-count variant when
        constructing the listing. Rust owns fitting, contextual syntax and the
        logical baseline. An edit outside the allocated layout fails atomically.
        Unchanged rows retain their handles. The previous Text leaves this
        listing and the new Text occupies its slot, including for identical
        source strings. Other ordinary memberships of the previous Text remain.

        For an ordinary animated transition, add ``previous`` to the scene and
        play ``FadeOut(previous)`` with ``Write(current)``.
        """
        if _replace_line is None:
            raise RuntimeError("Code source replacement requires Noon’s shared Rust authoring runtime")
        index = _number("index", _integer("index", index))
        if not isinstance(source, str):
            raise TypeError("source must be a string")
        context = _live_constructor_context("Code.replace_line", allow_unstarted=True)
        if context is None:
            context = getattr(self, "_canonical_live_target_context", None)
        replacement = engine_call(_replace_line, context, self._code_handle, index, source)
        try:
            previous_handle = engine_call(replacement.previous)
            current_handle = engine_call(replacement.current)
        finally:
            replacement.free()
        context = context if context is not None else getattr(self, "_canonical_live_target_context", None)
        # This registry preserves Python aliases only. Rust has already made
        # the atomic source, resource and membership change.
        known = self.code_lines._semantic_member_wrappers
        previous = known.pop(_handle_key(previous_handle), None)
        if isinstance(previous, Text):
            # Retain its existing JS handle and derived ObjectId so ordinary
            # re-entry can reuse the same binding. Source is observed from Rust;
            # normal membership reconciliation decides whether it detached.
            previous._source = str(engine_call(previous_handle.textSource))
            scene = getattr(previous, "_scene", None)
            if scene is not None:
                from _manim_scene import _sync_membership_wrapper_attachments
                _sync_membership_wrapper_attachments(scene, "remove", (previous,))
        else:
            previous = self._replacement_text(previous_handle, context)
        current = self._replacement_text(current_handle, context)
        known[_handle_key(current_handle)] = current
        return previous, current

    def _replacement_text(self, handle, context):
        wrapper = object.__new__(Text)
        source = str(engine_call(handle.textSource))
        wrapper._initialize_text(source, self._code_font_size,
                                 handle, _base.WHITE, 1.0, presentation_applied=True)
        wrapper._font = self._code_font
        wrapper._font_face = self._code_font_face
        wrapper._line_spacing = self._code_line_spacing
        if context is not None:
            wrapper._canonical_live_target_context = context
        return wrapper

    def get_highlighted_lines(
        self, start, stop=None, *, include_line_numbers=False, padding=None,
        color=None, fill_opacity=None,
    ):
        """Return a detached Rectangle for allocated source rows ``[start, stop)``.

        Indices are zero-based, independent of displayed line numbers; omitted
        ``stop`` selects one row. Blank rows retain their native layout slots.
        ``padding`` is in the Code's local units. Omitted styling uses Rust's
        defaults. Use this ordinary rectangle as a ``Transform`` target.
        """
        return self._line_region(start, stop, include_line_numbers, padding,
                                 color, fill_opacity, attached=False)

    def highlight_lines(
        self, start, stop=None, *, include_line_numbers=False, padding=None,
        color=None, fill_opacity=None,
    ):
        """Attach and return a native row-region Rectangle beneath the glyphs.

        The region lives in ``highlights``, above the panel and below the source
        and numbers. It follows ordinary Code transforms, copying and opacity.
        Family opacity writes update the region's fill opacity.
        Rows and options have the same meaning as ``get_highlighted_lines``;
        moving individual glyphs does not change their allocated row slots.
        """
        return self._line_region(start, stop, include_line_numbers, padding,
                                 color, fill_opacity, attached=True)

    def _line_region(self, start, stop, include_line_numbers, padding,
                     color, fill_opacity, *, attached):
        operation = "Code.highlight_lines" if attached else "Code.get_highlighted_lines"
        factory = _highlight_lines if attached else _get_highlighted_lines
        if factory is None or _new_highlight_options is None:
            raise RuntimeError("Code line regions require Noon’s shared Rust authoring runtime")
        start = _number("start", _integer("start", start))
        stop = None if stop is None else _number("stop", _integer("stop", stop))
        if not isinstance(include_line_numbers, bool):
            raise TypeError("include_line_numbers must be a bool")
        padding = None if padding is None else _number("padding", padding)
        fill_opacity = None if fill_opacity is None else _number("fill_opacity", fill_opacity)
        color = None if color is None else _compat._as_color("color", color)
        context = _live_constructor_context(operation, allow_unstarted=True)
        options = engine_call(_new_highlight_options, include_line_numbers)
        try:
            if padding is not None:
                engine_call(options.setPadding, padding)
            if fill_opacity is not None:
                engine_call(options.setFillOpacity, fill_opacity)
            if color is not None:
                engine_call(options.setColor, color.red, color.green, color.blue, color.alpha)
        except BaseException:
            options.free()
            raise
        # The shared operation consumes options, including rejected admissions.
        handle = engine_call(factory, context, self._code_handle, start, stop, options)
        region = object.__new__(_compat.Rectangle)
        _attach_shared_handle(region, handle)
        context = context if context is not None else getattr(self, "_canonical_live_target_context", None)
        if context is not None:
            region._canonical_live_target_context = context
        if attached:
            # This registry preserves Python aliases; Rust owns membership/order.
            self.highlights._semantic_member_wrappers[_handle_key(handle)] = region
        return region

    @staticmethod
    def get_styles_list():
        if _code_styles_list is None:
            raise RuntimeError("Code requires Noon’s shared Rust authoring runtime")
        return [str(name) for name in engine_call(_code_styles_list)]

    @staticmethod
    def get_languages_list():
        if _code_languages_list is None:
            raise RuntimeError("Code requires Noon’s shared Rust authoring runtime")
        return [str(name) for name in engine_call(_code_languages_list)]

    @staticmethod
    def get_pygments_style(style):
        raise NotImplementedError("Code uses native named themes and does not construct Python Pygments Style classes")
