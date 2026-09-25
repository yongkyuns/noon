"""Thin Table facades over Noon’s retained Rust table families."""
from _noon_errors import engine_call
import noon as _base
import _manim_compat as _compat
from _manim_semantic_handles import (
    _attach_shared_family,
    _attach_shared_handle,
    _family_wrapper_key,
    _handle_for,
    _live_constructor_context,
)

try:
    from js import noonCreateAuthoringTableHandle as _create_table
    from js import noonCreateAuthoringMathTableHandle as _create_math_table
    from js import noonCreateAuthoringIntegerTableHandle as _create_integer_table
    from js import noonCreateAuthoringDecimalTableHandle as _create_decimal_table
    from js import noonCreateAuthoringMobjectTableHandle as _create_mobject_table
    from js import noonHighlightTableCell as _highlight_table
    from js import noonTableCell as _table_cell
except ImportError:
    _create_table = _create_math_table = _create_integer_table = None
    _create_decimal_table = _create_mobject_table = _highlight_table = _table_cell = None


_OPTION_NAMES = {"v_buff", "h_buff", "include_outer_lines"}


def _options(kwargs):
    unknown = set(kwargs) - _OPTION_NAMES
    if unknown:
        raise NotImplementedError("unsupported Table option(s): " + ", ".join(sorted(unknown)))
    return (kwargs.get("v_buff", .8), kwargs.get("h_buff", 1.3), kwargs.get("include_outer_lines", False))


def _rows(values, convert):
    return [[convert(value) for value in row] for row in values]


def _handle_key(handle):
    return f"{int(handle.semanticSlot)}:{int(handle.semanticGeneration)}"


class Table(_compat.VGroup):
    """A normal retained family: entries, lines and highlights remain Rust handles."""
    def __init__(self, table, **kwargs):
        if _create_table is None:
            raise RuntimeError("Table requires Noon’s shared Rust authoring runtime")
        context = _live_constructor_context("Table", allow_unstarted=True)
        self._initialize_table(engine_call(_create_table, _rows(table, str), *_options(kwargs), context), context)

    def _initialize_table(self, handle, context):
        self._table_handle = handle
        _attach_shared_family(self, engine_call(handle.family), context)

    def get_entries(self, pos=None):
        entries = list(engine_call(self._table_handle.entries))
        known = {
            _family_wrapper_key(item): item
            for row in self._entry_family().submobjects
            for item in row.submobjects
        }
        if pos is not None:
            row, column = (int(pos[0]), int(pos[1]))
            return known[_handle_key(engine_call(self._table_handle.entryAt, row, column))]
        return _compat.VGroup(*[known[_handle_key(item)] for item in entries])

    def _entry_family(self):
        return self.submobjects[0]

    def get_rows(self):
        return _compat.VGroup(*self._entry_family().submobjects)

    def get_columns(self):
        known = {
            _family_wrapper_key(item): item
            for row in self._entry_family().submobjects
            for item in row.submobjects
        }
        return _compat.VGroup(*[
            _compat.VGroup(*[known[_handle_key(item)] for item in column])
            for column in engine_call(self._table_handle.columns)
        ])

    def _rectangle_wrapper(self, handle):
        wrapper = object.__new__(_compat.Rectangle)
        _attach_shared_handle(wrapper, handle)
        context = getattr(self, "_canonical_live_target_context", None)
        if context is not None:
            wrapper._canonical_live_target_context = context
        return wrapper

    def get_cell(self, pos, **kwargs):
        if kwargs:
            raise NotImplementedError("cell styling is owned by the Rust table API")
        if _table_cell is None:
            raise RuntimeError("Table requires Noon’s shared Rust authoring runtime")
        context = _live_constructor_context("Table.get_cell", allow_unstarted=True)
        return self._rectangle_wrapper(engine_call(_table_cell, context, self._table_handle, int(pos[0]), int(pos[1])))

    def get_highlighted_cell(self, pos, color=_base.BLUE, **kwargs):
        opacity = float(kwargs.pop("fill_opacity", 1.0))
        if kwargs:
            raise NotImplementedError("unsupported highlighted-cell option(s): " + ", ".join(sorted(kwargs)))
        color = _compat._as_color("color", color)
        if _highlight_table is None:
            raise RuntimeError("Table requires Noon’s shared Rust authoring runtime")
        context = _live_constructor_context("Table.highlight_cell", allow_unstarted=True)
        return self._rectangle_wrapper(engine_call(
            _highlight_table,
            context,
            self._table_handle,
            int(pos[0]), int(pos[1]), color.red, color.green, color.blue, color.alpha, opacity,
        ))

    def add_highlighted_cell(self, pos, color=_base.BLUE, **kwargs):
        self.get_highlighted_cell(pos, color, **kwargs)
        return self


class MathTable(Table):
    def __init__(self, table, **kwargs):
        if _create_math_table is None:
            raise RuntimeError("MathTable requires Noon’s shared Rust authoring runtime")
        context = _live_constructor_context("MathTable", allow_unstarted=True)
        self._initialize_table(engine_call(_create_math_table, _rows(table, str), *_options(kwargs), context), context)


class IntegerTable(Table):
    def __init__(self, table, **kwargs):
        if _create_integer_table is None:
            raise RuntimeError("IntegerTable requires Noon’s shared Rust authoring runtime")
        context = _live_constructor_context("IntegerTable", allow_unstarted=True)
        self._initialize_table(engine_call(_create_integer_table, _rows(table, float), *_options(kwargs), context), context)


class DecimalTable(Table):
    def __init__(self, table, **kwargs):
        if _create_decimal_table is None:
            raise RuntimeError("DecimalTable requires Noon’s shared Rust authoring runtime")
        context = _live_constructor_context("DecimalTable", allow_unstarted=True)
        self._initialize_table(engine_call(_create_decimal_table, _rows(table, float), *_options(kwargs), context), context)


class MobjectTable(Table):
    def __init__(self, table, **kwargs):
        if _create_mobject_table is None:
            raise RuntimeError("MobjectTable requires Noon’s shared Rust authoring runtime")
        source_rows = [list(row) for row in table]
        def handle(value):
            if not isinstance(value, _base.Mobject):
                raise TypeError("MobjectTable entries must be shared Mobjects or Groups")
            family = getattr(value, "_semantic_family_handle", None)
            if family is not None:
                return family
            semantic = _handle_for(value)
            if semantic is None:
                raise TypeError("MobjectTable entries require current shared semantic identities")
            return semantic
        context = _live_constructor_context("MobjectTable", allow_unstarted=True)
        self._initialize_table(engine_call(_create_mobject_table, _rows(source_rows, handle), *_options(kwargs), context), context)
        for row_group, row in zip(self._entry_family().submobjects, source_rows):
            row_group._semantic_member_wrappers = {
                _family_wrapper_key(value): value for value in row
            }
