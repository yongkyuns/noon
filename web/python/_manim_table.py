"""Thin Table facades over Noon’s retained Rust table families."""
from operator import index as _index

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
    from js import noonCreateAuthoringTableWithLabelsHandle as _create_table_with_labels
    from js import noonHighlightTableCell as _highlight_table
    from js import noonGetHighlightedTableCell as _get_highlighted_table
    from js import noonTableFromFamily as _table_from_family
    from js import noonTableCell as _table_cell
except ImportError:
    _table_from_family = None
    _create_table = _create_math_table = _create_integer_table = None
    _create_decimal_table = _create_mobject_table = _create_table_with_labels = None
    _highlight_table = _get_highlighted_table = _table_cell = None


_OPTION_NAMES = {"v_buff", "h_buff", "include_outer_lines", "include_inner_lines", "line_config"}
_LINE_CONFIG_NAMES = {"color", "stroke_color", "stroke_width", "stroke_opacity"}
_DECIMAL_CONFIG_NAMES = {
    "num_decimal_places", "include_sign", "group_with_commas", "show_ellipsis", "unit"
}
_UNSET = object()


def _options(kwargs):
    unknown = set(kwargs) - _OPTION_NAMES
    if unknown:
        raise NotImplementedError("unsupported Table option(s): " + ", ".join(sorted(unknown)))
    line_config = kwargs.get("line_config", {})
    if not isinstance(line_config, dict):
        raise TypeError("line_config must be a dict")
    unknown_line_config = set(line_config) - _LINE_CONFIG_NAMES
    if unknown_line_config:
        raise NotImplementedError(
            "unsupported Table line_config option(s): "
            + ", ".join(sorted(unknown_line_config))
        )
    line_color = line_config.get("stroke_color", line_config.get("color", _base.WHITE))
    line_color = _compat._as_color("line_config color", line_color)
    # Match the renderer's shared Manim Cairo width conversion (default 4 -> .04).
    line_width = _compat._manim_stroke_width(line_config.get("stroke_width", 4.0))
    line_opacity = _compat._opacity(
        "line_config.stroke_opacity", line_config.get("stroke_opacity", 1.0)
    )
    return (
        kwargs.get("v_buff", .8),
        kwargs.get("h_buff", 1.3),
        kwargs.get("include_outer_lines", False),
        kwargs.get("include_inner_lines", True),
        line_color.red,
        line_color.green,
        line_color.blue,
        line_color.alpha,
        line_width,
        line_opacity,
    )


def _decimal_format(config):
    if config is _UNSET:
        config = {"num_decimal_places": 1}
    elif not isinstance(config, dict):
        raise TypeError("element_to_mobject_config must be a dict")
    unknown = set(config) - _DECIMAL_CONFIG_NAMES
    if unknown:
        raise NotImplementedError(
            "unsupported DecimalTable element option(s): " + ", ".join(sorted(unknown))
        )
    places = config.get("num_decimal_places", 2)
    if isinstance(places, bool):
        raise TypeError("num_decimal_places must be an integer")
    try:
        places = _index(places)
    except TypeError as error:
        raise TypeError("num_decimal_places must be an integer") from error
    if not 0 <= places <= 12:
        raise ValueError("num_decimal_places must be between 0 and 12")
    unit = config.get("unit")
    if unit is not None and not isinstance(unit, str):
        raise TypeError("DecimalTable unit must be a string or None")
    return (
        places,
        bool(config.get("include_sign", False)),
        bool(config.get("group_with_commas", True)),
        bool(config.get("show_ellipsis", False)),
        unit,
    )


def _use_custom_element_converter(owner, rows, kwargs, config=_UNSET, *, require_converter=False):
    converter = kwargs.pop("element_to_mobject", _UNSET)
    if config is _UNSET:
        config = kwargs.pop("element_to_mobject_config", _UNSET)
    if converter is _UNSET:
        if require_converter and config is not _UNSET:
            if not isinstance(config, dict):
                raise TypeError("element_to_mobject_config must be a dict")
            if config:
                raise NotImplementedError(
                    f"{type(owner).__name__} element_to_mobject_config requires an explicit shared-Mobject converter"
                )
        return False
    if not callable(converter):
        raise TypeError(f"{type(owner).__name__} element_to_mobject must be callable")
    if config is _UNSET:
        config = {}
    if not isinstance(config, dict):
        raise TypeError("element_to_mobject_config must be a dict")
    MobjectTable.__init__(
        owner,
        rows,
        element_to_mobject=converter,
        element_to_mobject_config=config,
        **kwargs,
    )
    return True


def _rows(values, convert):
    return [[convert(value) for value in row] for row in values]


def _handle_key(handle):
    return f"{int(handle.semanticSlot)}:{int(handle.semanticGeneration)}"


def _label_handle(value, kind="Table"):
    if not isinstance(value, _base.Mobject):
        raise TypeError(f"{kind} labels must be shared Mobjects")
    family = getattr(value, "_semantic_family_handle", None)
    if family is not None:
        return family
    handle = _handle_for(value)
    if handle is None:
        raise TypeError(f"{kind} labels require current shared semantic identities")
    return handle


def _cell_position(pos):
    try:
        row, column = pos
        row, column = _index(row), _index(column)
    except (IndexError, TypeError, ValueError) as error:
        raise ValueError("Table positions must be one-based (row, column) pairs") from error
    if row < 1 or column < 1:
        raise ValueError("Table positions must be one-based (row, column) pairs")
    return row - 1, column - 1


class Table(_compat.VGroup):
    """A normal retained family: entries, lines and highlights remain Rust handles."""
    def __init__(self, table, **kwargs):
        if _use_custom_element_converter(self, table, kwargs, require_converter=True):
            return
        row_label_mobjects = kwargs.pop("row_labels", None)
        column_label_mobjects = kwargs.pop("col_labels", None)
        top_left_entry = kwargs.pop("top_left_entry", None)
        if row_label_mobjects is not None:
            row_label_mobjects = list(row_label_mobjects)
        if column_label_mobjects is not None:
            column_label_mobjects = list(column_label_mobjects)
        if top_left_entry is not None and (
            row_label_mobjects is None or column_label_mobjects is None
        ):
            raise ValueError("top_left_entry requires both row_labels and col_labels")
        if _create_table is None:
            raise RuntimeError("Table requires Noon’s shared Rust authoring runtime")
        context = _live_constructor_context("Table", allow_unstarted=True)
        if row_label_mobjects is not None or column_label_mobjects is not None or top_left_entry is not None:
            if _create_table_with_labels is None:
                raise RuntimeError("Table labels require Noon’s shared Rust authoring runtime")
            handle = engine_call(
                _create_table_with_labels,
                _rows(table, str),
                [_label_handle(value) for value in row_label_mobjects or []],
                [_label_handle(value) for value in column_label_mobjects or []],
                [_label_handle(top_left_entry)] if top_left_entry is not None else [],
                *_options(kwargs),
                context,
            )
        else:
            handle = engine_call(_create_table, _rows(table, str), *_options(kwargs), context)
        self._initialize_table(handle, context)
        self.row_labels = row_label_mobjects
        self.col_labels = column_label_mobjects
        self.top_left_entry = top_left_entry
        family_index = 3
        for labels in (row_label_mobjects, column_label_mobjects):
            if labels is not None:
                label_group = self.submobjects[family_index]
                label_group._semantic_member_wrappers = {
                    _family_wrapper_key(value): value for value in labels
                }
                family_index += 1
        if top_left_entry is not None:
            top_left_family = self.submobjects[family_index]
            top_left_family._semantic_member_wrappers = {
                _family_wrapper_key(top_left_entry): top_left_entry
            }

    @staticmethod
    def _label_handle(value):
        return _label_handle(value, "Table")

    def _initialize_table(self, handle, context):
        self._table_handle = handle
        self._horizontal_lines_cache = None
        self._vertical_lines_cache = None
        if not hasattr(self, "row_labels"):
            self.row_labels = None
        if not hasattr(self, "col_labels"):
            self.col_labels = None
        if not hasattr(self, "top_left_entry"):
            self.top_left_entry = None
        _attach_shared_family(self, engine_call(handle.family), context)

    def _rehydrate_semantic_family_handle(self):
        if _table_from_family is None:
            raise RuntimeError("Table requires Noon’s shared Rust authoring runtime")
        self._initialize_table(
            engine_call(_table_from_family, self._semantic_family_handle),
            getattr(self, "_canonical_live_target_context", None),
        )

    def _get_entries_without_labels(self, pos=None):
        if pos is not None:
            row, column = _cell_position(pos)
            handle = engine_call(self._table_handle.entryAt, row, column)
            # Observe only the addressed row's identity registry. Asking for one
            # entry must not enumerate every entry in the table.
            known = {
                _family_wrapper_key(item): item
                for item in self._entry_family().submobjects[row].submobjects
            }
            return known[_handle_key(handle)]
        entries = list(engine_call(self._table_handle.entries))
        known = {
            _family_wrapper_key(item): item
            for row in self._entry_family().submobjects
            for item in row.submobjects
        }
        return _compat.VGroup(*[known[_handle_key(item)] for item in entries])

    def _rows_with_labels(self):
        rows = [list(row.submobjects) for row in self._entry_family().submobjects]
        if self.row_labels:
            rows = [[label, *row] for label, row in zip(self.row_labels, rows)]
        if self.col_labels:
            if self.row_labels:
                corner = self.top_left_entry
                if corner is None:
                    corner = _compat.VGroup()
                rows.insert(0, [corner, *self.col_labels])
            else:
                rows.insert(0, list(self.col_labels))
        return rows

    def get_entries_without_labels(self, pos=None):
        """Return body entries without any row, column, or corner labels."""
        return self._get_entries_without_labels(pos)

    def get_entries(self, pos=None):
        """Return entries in ManimCE's displayed grid, including supplied labels."""
        if not (self.row_labels or self.col_labels):
            return self._get_entries_without_labels(pos)

        rows = self._rows_with_labels()
        entries = [item for row in rows for item in row]
        missing_corner = bool(self.row_labels and self.col_labels) and self.top_left_entry is None
        if missing_corner:
            # ManimCE drops its empty grid placeholder from ``elements``.
            entries.pop(0)
        if pos is not None:
            row, column = _cell_position(pos)
            index = len(rows[0]) * row + column - int(missing_corner)
            return entries[index]
        return _compat.VGroup(*entries)

    def _entry_family(self):
        return self.submobjects[1]

    def _line_family(self):
        return self.submobjects[2]

    def _line_group(self, method_name, cache_name):
        cached = getattr(self, cache_name)
        if cached is not None:
            return cached
        known = {
            _handle_key(_handle_for(item)): item
            for item in self._line_family().submobjects
        }
        handles = engine_call(getattr(self._table_handle, method_name))
        group = _compat.VGroup(*[known[_handle_key(handle)] for handle in handles])
        setattr(self, cache_name, group)
        return group

    def get_horizontal_lines(self):
        return self._line_group("getHorizontalLines", "_horizontal_lines_cache")

    def get_vertical_lines(self):
        return self._line_group("getVerticalLines", "_vertical_lines_cache")

    def get_rows(self):
        return _compat.VGroup(*[
            _compat.VGroup(*row)
            for row in self._rows_with_labels()
        ])

    def get_columns(self):
        known = {
            _family_wrapper_key(item): item
            for row in self._entry_family().submobjects
            for item in row.submobjects
        }
        columns = [
            [known[_handle_key(item)] for item in column]
            for column in engine_call(self._table_handle.columns)
        ]
        if self.col_labels:
            columns = [
                [label, *column]
                for label, column in zip(self.col_labels, columns)
            ]
        if self.row_labels:
            label_column = list(self.row_labels)
            if self.col_labels:
                corner = self.top_left_entry
                if corner is None:
                    corner = _compat.VGroup()
                label_column.insert(0, corner)
            columns.insert(0, label_column)
        return _compat.VGroup(*[_compat.VGroup(*column) for column in columns])

    def get_row_labels(self):
        return _compat.VGroup(*(self.row_labels or []))

    def get_col_labels(self):
        return _compat.VGroup(*(self.col_labels or []))

    def get_labels(self):
        labels = []
        if self.top_left_entry is not None:
            labels.append(self.top_left_entry)
        labels.extend(self.col_labels or [])
        labels.extend(self.row_labels or [])
        return _compat.VGroup(*labels)

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
        row, column = _cell_position(pos)
        return self._rectangle_wrapper(engine_call(_table_cell, context, self._table_handle, row, column))

    def get_highlighted_cell(self, pos, color=_base.PURE_YELLOW, **kwargs):
        opacity = float(kwargs.pop("fill_opacity", 0.75))
        if kwargs:
            raise NotImplementedError("unsupported highlighted-cell option(s): " + ", ".join(sorted(kwargs)))
        color = _compat._as_color("color", color)
        if _get_highlighted_table is None:
            raise RuntimeError("Table requires Noon’s shared Rust authoring runtime")
        context = _live_constructor_context("Table.get_highlighted_cell", allow_unstarted=True)
        row, column = _cell_position(pos)
        return self._rectangle_wrapper(engine_call(
            _get_highlighted_table,
            context,
            self._table_handle,
            row, column, color.red, color.green, color.blue, color.alpha, opacity,
        ))

    def add_highlighted_cell(self, pos, color=_base.PURE_YELLOW, **kwargs):
        opacity = float(kwargs.pop("fill_opacity", 0.75))
        if kwargs:
            raise NotImplementedError("unsupported highlighted-cell option(s): " + ", ".join(sorted(kwargs)))
        if _highlight_table is None:
            raise RuntimeError("Table requires Noon’s shared Rust authoring runtime")
        color = _compat._as_color("color", color)
        context = _live_constructor_context("Table.add_highlighted_cell", allow_unstarted=True)
        row, column = _cell_position(pos)
        handle = engine_call(
            _highlight_table,
            context,
            self._table_handle,
            row, column, color.red, color.green, color.blue, color.alpha, opacity,
        )
        highlight = self._rectangle_wrapper(handle)
        self.submobjects[0]._semantic_member_wrappers[_family_wrapper_key(highlight)] = highlight
        return self


class MathTable(Table):
    def __init__(self, table, **kwargs):
        if _use_custom_element_converter(self, table, kwargs, require_converter=True):
            return
        if _create_math_table is None:
            raise RuntimeError("MathTable requires Noon’s shared Rust authoring runtime")
        context = _live_constructor_context("MathTable", allow_unstarted=True)
        self._initialize_table(engine_call(_create_math_table, _rows(table, str), *_options(kwargs), context), context)


class IntegerTable(Table):
    def __init__(self, table, **kwargs):
        if _use_custom_element_converter(self, table, kwargs, require_converter=True):
            return
        if _create_integer_table is None:
            raise RuntimeError("IntegerTable requires Noon’s shared Rust authoring runtime")
        context = _live_constructor_context("IntegerTable", allow_unstarted=True)
        self._initialize_table(engine_call(_create_integer_table, _rows(table, float), *_options(kwargs), context), context)


class DecimalTable(Table):
    def __init__(self, table, element_to_mobject=_UNSET,
                 element_to_mobject_config=_UNSET, **kwargs):
        if _use_custom_element_converter(
            self,
            table,
            {**kwargs, "element_to_mobject": element_to_mobject}
            if element_to_mobject is not _UNSET else kwargs,
            element_to_mobject_config,
        ):
            return
        if _create_decimal_table is None:
            raise RuntimeError("DecimalTable requires Noon’s shared Rust authoring runtime")
        context = _live_constructor_context("DecimalTable", allow_unstarted=True)
        self._initialize_table(engine_call(
            _create_decimal_table,
            _rows(table, float),
            *_decimal_format(element_to_mobject_config),
            *_options(kwargs),
            context,
        ), context)


class MobjectTable(Table):
    def __init__(self, table, **kwargs):
        element_to_mobject = kwargs.pop("element_to_mobject", lambda value: value)
        element_to_mobject_config = kwargs.pop("element_to_mobject_config", {})
        if not callable(element_to_mobject):
            raise TypeError("MobjectTable element_to_mobject must be callable")
        if not isinstance(element_to_mobject_config, dict):
            raise TypeError("element_to_mobject_config must be a dict")
        row_label_mobjects = list(kwargs.pop("row_labels", None) or [])
        column_label_mobjects = list(kwargs.pop("col_labels", None) or [])
        top_left_entry = kwargs.pop("top_left_entry", None)
        if top_left_entry is not None and (not row_label_mobjects or not column_label_mobjects):
            raise ValueError("top_left_entry requires both row_labels and col_labels")
        if _create_mobject_table is None:
            raise RuntimeError("MobjectTable requires Noon’s shared Rust authoring runtime")
        source_rows = [
            [element_to_mobject(value, **element_to_mobject_config) for value in row]
            for row in table
        ]
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
        row_labels = [self._label_handle(value) for value in row_label_mobjects]
        column_labels = [self._label_handle(value) for value in column_label_mobjects]
        top_left_entry_handle = (
            self._label_handle(top_left_entry) if top_left_entry is not None else None
        )
        context = _live_constructor_context("MobjectTable", allow_unstarted=True)
        self._initialize_table(
            engine_call(
                _create_mobject_table,
                _rows(source_rows, handle),
                row_labels,
                column_labels,
                [top_left_entry_handle] if top_left_entry_handle is not None else [],
                *_options(kwargs),
                context,
            ),
            context,
        )
        self.row_labels = row_label_mobjects or None
        self.col_labels = column_label_mobjects or None
        self.top_left_entry = top_left_entry
        for row_group, row in zip(self._entry_family().submobjects, source_rows):
            row_group._semantic_member_wrappers = {
                _family_wrapper_key(value): value for value in row
            }
        family_index = 3
        for labels in (row_label_mobjects, column_label_mobjects):
            if labels:
                label_group = self.submobjects[family_index]
                label_group._semantic_member_wrappers = {
                    _family_wrapper_key(value): value for value in labels
                }
                family_index += 1
        if top_left_entry is not None:
            top_left_family = self.submobjects[family_index]
            top_left_family._semantic_member_wrappers = {
                _family_wrapper_key(top_left_entry): top_left_entry
            }

    @staticmethod
    def _label_handle(value):
        return _label_handle(value, "MobjectTable")
