"""Manim-shaped Code authoring over Noon’s shared Rust code and text pipeline."""
from collections.abc import Mapping
from operator import index as _index
from pathlib import Path

from _noon_errors import engine_call
import noon as _base
import _manim_compat as _compat
from _manim_semantic_handles import _attach_shared_family, _live_constructor_context
from _manim_typst import NativeFontFace, Text

try:
    from js import noonCodeOptions as _new_code_options
    from js import noonCreateAuthoringCodeHandle as _create_code
    from js import noonCodeFromFamily as _code_from_family
    from js import noonCodeStylesList as _code_styles_list
    from js import noonCodeLanguagesList as _code_languages_list
except ImportError:
    _new_code_options = _create_code = _code_from_family = None
    _code_styles_list = _code_languages_list = None


def _integer(name, value):
    if isinstance(value, bool):
        raise TypeError(f"{name} must be an integer")
    try:
        return _index(value)
    except TypeError as error:
        raise TypeError(f"{name} must be an integer") from error


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


class Code(_compat.VGroup):
    """Retained, syntax-colored source lines with native tab and gutter layout.

    Indentation and empty lines are preserved. This display object does not
    format source code. Use ``get_languages_list`` and ``get_styles_list`` for
    the shared engine’s supported language and theme profiles.
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
        paragraph_config=None,
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
