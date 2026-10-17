import types
import unittest
from unittest.mock import PropertyMock, patch

import _manim_table as table
from _manim_enums import CapStyleType, LineJointType


class _Handle:
    def __init__(self, slot):
        self.semanticSlot = slot
        self.semanticGeneration = 1


class _Family:
    pass


class _TableHandle:
    def __init__(self):
        self.a, self.b, self.c, self.d = (_Handle(index) for index in range(4))
        self._family = _Family()

    def family(self):
        return self._family

    def entries(self):
        return [self.a, self.b, self.c, self.d]

    def entryAt(self, row, column):
        return self.entries()[row * 2 + column]

    def columns(self):
        return [[self.a, self.c], [self.b, self.d]]

    def getHorizontalLines(self):
        return [self.a, self.b]

    def getVerticalLines(self):
        return [self.c, self.d]


class _Context:
    def __init__(self, handle):
        self.handle = handle
        self.calls = []

    def liveCreateTable(self, *args):
        self.calls.append(("create", args))
        return self.handle

    def liveCreateTableWithLabels(self, *args):
        self.calls.append(("create_with_labels", args))
        return self.handle

    def liveCreateDecimalTable(self, *args):
        self.calls.append(("create_decimal", args))
        return self.handle


class _MobjectHandle(_Handle):
    pass


class _MobjectTableHandle:
    def __init__(self, row_labels, column_labels, entries, top_left_entry=None):
        self._family = _Family()
        self._family.submobjects = [
            types.SimpleNamespace(submobjects=[]),
            types.SimpleNamespace(submobjects=[types.SimpleNamespace(submobjects=entries)]),
            types.SimpleNamespace(submobjects=[]),
            *([types.SimpleNamespace(submobjects=row_labels)] if row_labels else []),
            *([types.SimpleNamespace(submobjects=column_labels)] if column_labels else []),
            *([types.SimpleNamespace(submobjects=[top_left_entry])] if top_left_entry else []),
        ]

    def family(self):
        return self._family


class _FakeMobject:
    def __init__(self, slot):
        self._semantic_handle = _MobjectHandle(slot)


class _FakeFamily(_FakeMobject):
    def __init__(self, slot):
        super().__init__(slot)
        self._semantic_family_handle = object()


class TableFacadeTests(unittest.TestCase):
    def setUp(self):
        self.handle = _TableHandle()
        self.context = _Context(self.handle)

        def attach_family(wrapper, _handle, context):
            wrapper._semantic_family_handle = _handle
            wrapper._canonical_live_target_context = context
            return wrapper

        self.patches = [
            patch.object(table, "_create_table", side_effect=self.context.liveCreateTable),
            patch.object(table, "_create_table_with_labels", side_effect=self.context.liveCreateTableWithLabels),
            patch.object(table, "_create_decimal_table", side_effect=self.context.liveCreateDecimalTable),
            patch.object(table, "_live_constructor_context", return_value=self.context),
            patch.object(table, "engine_call", side_effect=lambda method, *args, **_: method(*args)),
            patch.object(table, "_attach_shared_family", side_effect=attach_family),
            patch.object(table, "_attach_shared_handle", side_effect=lambda wrapper, handle: setattr(wrapper, "_semantic_handle", handle)),
            patch.object(table, "_family_wrapper_key", side_effect=lambda value: f"{value._semantic_handle.semanticSlot}:{value._semantic_handle.semanticGeneration}"),
            patch.object(table._compat, "VGroup", side_effect=lambda *items: types.SimpleNamespace(submobjects=list(items))),
            patch.object(table, "_table_cell", side_effect=lambda *_: _Handle(9)),
            patch.object(table, "_highlight_table", side_effect=lambda *_: _Handle(10)),
            patch.object(table, "_get_highlighted_table", side_effect=lambda *_: _Handle(11)),
        ]
        for patcher in self.patches:
            patcher.start()

    def tearDown(self):
        for patcher in reversed(self.patches):
            patcher.stop()

    def test_entries_columns_and_cells_keep_the_shared_handle_contract(self):
        value = table.Table([["a", "b"], ["c", "d"]])
        self.assertEqual(self.context.calls[0][1][1:5], (0.8, 1.3, False, True))
        leaves = [types.SimpleNamespace(_semantic_handle=raw) for raw in self.handle.entries()]
        rows = [types.SimpleNamespace(submobjects=leaves[:2]),
                types.SimpleNamespace(submobjects=leaves[2:])]
        value._entry_family = lambda: types.SimpleNamespace(submobjects=rows)
        self.assertEqual(self.context.calls[0][0], "create")
        self.assertEqual(value.get_entries().submobjects[0]._semantic_handle, self.handle.a)
        self.assertEqual(value.get_entries((2, 1))._semantic_handle, self.handle.c)
        columns = value.get_columns().submobjects
        self.assertEqual(columns[0].submobjects[1]._semantic_handle, self.handle.c)
        cell = value.get_cell((1, 2))
        self.assertIsInstance(cell, table._compat.Rectangle)
        self.assertEqual(cell._semantic_handle.semanticSlot, 9)
        detached = value.get_highlighted_cell((1, 1))
        self.assertEqual(detached._semantic_handle.semanticSlot, 11)
        self.assertEqual(table._get_highlighted_table.call_args.args[4:], (1.0, 1.0, 0.0, 1.0, 0.75))
        table._highlight_table.assert_not_called()
        highlights = types.SimpleNamespace(_semantic_member_wrappers={})
        with patch.object(table.Table, "submobjects", new_callable=PropertyMock,
                          return_value=[highlights, None, None]):
            self.assertIs(value.add_highlighted_cell((2, 2)), value)
        self.assertEqual(highlights._semantic_member_wrappers["10:1"]._semantic_handle.semanticSlot, 10)
        for invalid in ((0, 1), (1.5, 1), (1, "2"), (1, 2, 3)):
            with self.subTest(invalid=invalid), self.assertRaisesRegex(ValueError, "one-based"):
                value.get_cell(invalid)

    def test_single_entry_does_not_enumerate_other_rows_or_all_entries(self):
        value = table.Table([["a", "b"], ["c", "d"]])
        leaf = types.SimpleNamespace(_semantic_handle=self.handle.c)
        class UnrelatedRow:
            @property
            def submobjects(self):
                raise AssertionError("a local entry lookup traversed an unrelated row")
        value._entry_family = lambda: types.SimpleNamespace(
            submobjects=[UnrelatedRow(), types.SimpleNamespace(submobjects=[leaf])])
        with patch.object(self.handle, "entryAt", return_value=self.handle.c), \
                patch.object(self.handle, "entries", side_effect=AssertionError("enumerated all entries")):
            self.assertIs(value.get_entries((2, 1)), leaf)

    def test_horizontal_and_vertical_line_groups_reuse_retained_wrappers(self):
        value = table.Table([["a", "b"], ["c", "d"]])
        lines = [types.SimpleNamespace(_semantic_handle=raw) for raw in self.handle.entries()]
        line_family = types.SimpleNamespace(submobjects=lines)
        with patch.object(table, "_handle_for", side_effect=lambda item: item._semantic_handle), \
                patch.object(table.Table, "submobjects", new_callable=PropertyMock,
                             return_value=[None, None, line_family]):
            horizontal = value.get_horizontal_lines()
            vertical = value.get_vertical_lines()
            self.assertIs(value.get_horizontal_lines(), horizontal)
            self.assertIs(value.get_vertical_lines(), vertical)
        self.assertEqual(horizontal.submobjects, lines[:2])
        self.assertEqual(vertical.submobjects, lines[2:])

    def test_label_queries_keep_manims_group_order_and_original_wrappers(self):
        value = table.Table([["a"]])
        row = object()
        column = object()
        corner = object()
        value.row_labels = [row]
        value.col_labels = [column]
        value.top_left_entry = corner

        self.assertEqual(value.get_row_labels().submobjects, [row])
        self.assertEqual(value.get_col_labels().submobjects, [column])
        self.assertEqual(value.get_labels().submobjects, [corner, column, row])

    def test_absent_label_queries_return_empty_groups(self):
        value = table.Table([["a"]])
        self.assertEqual(value.get_row_labels().submobjects, [])
        self.assertEqual(value.get_col_labels().submobjects, [])
        self.assertEqual(value.get_labels().submobjects, [])

    def test_decimal_table_forwards_supported_decimalnumber_format(self):
        table.DecimalTable(
            [[1234.5, -2.25]],
            element_to_mobject_config={
                "num_decimal_places": 3,
                "include_sign": True,
                "group_with_commas": False,
                "show_ellipsis": True,
                "unit": "m",
            },
        )
        _, args = self.context.calls[-1]
        self.assertEqual(args[0], [[1234.5, -2.25]])
        self.assertEqual(args[1:6], (3, True, False, True, "m"))
        self.assertEqual(args[6:10], (0.8, 1.3, False, True))

    def test_decimal_table_matches_default_and_empty_config_precision(self):
        table.DecimalTable([[1.25]])
        default_args = self.context.calls[-1][1]
        self.assertEqual(default_args[1:6], (1, False, True, False, None))

        table.DecimalTable([[1.25]], element_to_mobject_config={})
        empty_args = self.context.calls[-1][1]
        self.assertEqual(empty_args[1:6], (2, False, True, False, None))

    def test_decimal_table_rejects_unqualified_format_options(self):
        with self.assertRaisesRegex(NotImplementedError, "decimal_places"):
            table._decimal_format({"decimal_places": 2})
        with self.assertRaisesRegex(ValueError, "between 0 and 12"):
            table._decimal_format({"num_decimal_places": 13})

    def test_entry_queries_match_manims_display_grid_and_body_only_access(self):
        value = table.Table([["a", "b"], ["c", "d"]])
        wrappers = [
            types.SimpleNamespace(_semantic_handle=handle)
            for handle in self.handle.entries()
        ]
        value._entry_family = lambda: types.SimpleNamespace(submobjects=[
            types.SimpleNamespace(submobjects=wrappers[:2]),
            types.SimpleNamespace(submobjects=wrappers[2:]),
        ])
        row_labels = [object(), object()]
        column_labels = [object(), object()]
        corner = object()
        value.row_labels = row_labels
        value.col_labels = column_labels
        value.top_left_entry = corner

        entries = value.get_entries().submobjects
        self.assertEqual(
            entries,
            [corner, *column_labels, row_labels[0], *wrappers[:2], row_labels[1], *wrappers[2:]],
        )
        self.assertIs(value.get_entries((1, 1)), corner)
        self.assertIs(value.get_entries((2, 2)), wrappers[0])
        self.assertEqual(value.get_entries_without_labels().submobjects, wrappers)
        self.assertIs(value.get_entries_without_labels((1, 1)), wrappers[0])
        self.assertEqual(
            [row.submobjects for row in value.get_rows().submobjects],
            [[corner, *column_labels], [row_labels[0], *wrappers[:2]], [row_labels[1], *wrappers[2:]]],
        )
        self.assertEqual(
            [column.submobjects for column in value.get_columns().submobjects],
            [[corner, *row_labels], [column_labels[0], wrappers[0], wrappers[2]],
             [column_labels[1], wrappers[1], wrappers[3]]],
        )

    def test_missing_top_left_placeholder_matches_manims_flat_entry_indexing(self):
        value = table.Table([["a", "b"], ["c", "d"]])
        wrappers = [types.SimpleNamespace(_semantic_handle=handle) for handle in self.handle.entries()]
        value._entry_family = lambda: types.SimpleNamespace(submobjects=[
            types.SimpleNamespace(submobjects=wrappers[:2]),
            types.SimpleNamespace(submobjects=wrappers[2:]),
        ])
        value.row_labels = [object(), object()]
        value.col_labels = [object(), object()]
        value.top_left_entry = None

        self.assertEqual(len(value.get_entries().submobjects), 8)
        self.assertIsInstance(value.get_rows().submobjects[0].submobjects[0], types.SimpleNamespace)
        self.assertIs(value.get_entries((1, 1)), wrappers[-1])

    def test_single_axis_labels_expand_only_the_matching_display_dimension(self):
        wrappers = [types.SimpleNamespace(_semantic_handle=handle) for handle in self.handle.entries()]
        body_rows = [
            types.SimpleNamespace(submobjects=wrappers[:2]),
            types.SimpleNamespace(submobjects=wrappers[2:]),
        ]
        row_labels = [object(), object()]
        row_labeled = table.Table([["a", "b"], ["c", "d"]])
        row_labeled._entry_family = lambda: types.SimpleNamespace(submobjects=body_rows)
        row_labeled.row_labels = row_labels
        rows = row_labeled.get_rows().submobjects
        columns = row_labeled.get_columns().submobjects
        self.assertEqual([row.submobjects for row in rows], [
            [row_labels[0], *wrappers[:2]], [row_labels[1], *wrappers[2:]],
        ])
        self.assertEqual([column.submobjects for column in columns], [
            row_labels, [wrappers[0], wrappers[2]], [wrappers[1], wrappers[3]],
        ])

        column_labels = [object(), object()]
        column_labeled = table.Table([["a", "b"], ["c", "d"]])
        column_labeled._entry_family = lambda: types.SimpleNamespace(submobjects=body_rows)
        column_labeled.col_labels = column_labels
        rows = column_labeled.get_rows().submobjects
        columns = column_labeled.get_columns().submobjects
        self.assertEqual([row.submobjects for row in rows], [
            column_labels, wrappers[:2], wrappers[2:],
        ])
        self.assertEqual([column.submobjects for column in columns], [
            [column_labels[0], wrappers[0], wrappers[2]],
            [column_labels[1], wrappers[1], wrappers[3]],
        ])

    def test_mobject_table_row_and_column_labels_preserve_wrappers(self):
        entry = _FakeMobject(1)
        row_label = _FakeFamily(2)
        column_label = _FakeMobject(3)
        top_left_entry = _FakeFamily(4)
        context = types.SimpleNamespace()
        handle = _MobjectTableHandle([row_label], [column_label], [entry], top_left_entry)

        def create(rows, row_labels, column_labels, top_left_entries, v_buff, h_buff, outer_lines, inner_lines,
                   red, green, blue, alpha, width, opacity, join, cap, _context):
            self.assertEqual(rows, [[entry._semantic_handle]])
            self.assertEqual(row_labels, [row_label._semantic_family_handle])
            self.assertEqual(column_labels, [column_label._semantic_handle])
            self.assertEqual(top_left_entries, [top_left_entry._semantic_family_handle])
            self.assertEqual((v_buff, h_buff, outer_lines), (0.4, 0.7, True))
            self.assertTrue(inner_lines)
            self.assertEqual((red, green, blue, alpha, width, opacity), (1.0, 1.0, 1.0, 1.0, .04, 1.0))
            self.assertEqual((join, cap), ("miter", "butt"))
            return handle

        with patch.object(table._base, "Mobject", _FakeMobject), \
                patch.object(table, "_handle_for", side_effect=lambda value: value._semantic_handle), \
                patch.object(table, "_create_mobject_table", side_effect=create), \
                patch.object(table, "_live_constructor_context", return_value=context), \
                patch.object(table, "_attach_shared_family", side_effect=lambda wrapper, family, _context: (
                    setattr(wrapper, "_test_submobjects", family.submobjects),
                    setattr(wrapper, "_canonical_live_target_context", context),
                )), \
                patch.object(table._compat.Group, "submobjects", new=property(
                    lambda wrapper: wrapper._test_submobjects
                )), \
                patch.object(table, "_family_wrapper_key", side_effect=lambda value: (
                    f"{value._semantic_handle.semanticSlot}:{value._semantic_handle.semanticGeneration}"
                )):
            value = table.MobjectTable(
                [[entry]], row_labels=[row_label], col_labels=[column_label],
                top_left_entry=top_left_entry,
                v_buff=0.4, h_buff=0.7, include_outer_lines=True,
            )

        children = value._test_submobjects
        self.assertIs(value.row_labels[0], row_label)
        self.assertIs(value.col_labels[0], column_label)
        self.assertIs(value.top_left_entry, top_left_entry)
        self.assertIs(children[1].submobjects[0]._semantic_member_wrappers["1:1"], entry)
        self.assertIs(children[3]._semantic_member_wrappers["2:1"], row_label)
        self.assertIs(children[4]._semantic_member_wrappers["3:1"], column_label)
        self.assertIs(children[5]._semantic_member_wrappers["4:1"], top_left_entry)

    def test_mobject_table_element_converter_receives_config_and_retains_results(self):
        inputs = [_FakeMobject(1), _FakeMobject(2)]
        converted = [_FakeMobject(3), _FakeMobject(4)]
        calls = []
        context = types.SimpleNamespace()
        handle = _MobjectTableHandle([], [], converted)

        def convert(value, *, scale):
            calls.append((value, scale))
            return converted[len(calls) - 1]

        def create(rows, *args):
            self.assertEqual(rows, [[item._semantic_handle for item in converted]])
            self.assertIs(args[-1], context)
            return handle

        with patch.object(table._base, "Mobject", _FakeMobject), \
                patch.object(table, "_handle_for", side_effect=lambda value: value._semantic_handle), \
                patch.object(table, "_create_mobject_table", side_effect=create), \
                patch.object(table, "_live_constructor_context", return_value=context), \
                patch.object(table, "_attach_shared_family", side_effect=lambda wrapper, family, _context: (
                    setattr(wrapper, "_test_submobjects", family.submobjects),
                    setattr(wrapper, "_canonical_live_target_context", context),
                )), \
                patch.object(table._compat.Group, "submobjects", new=property(
                    lambda wrapper: wrapper._test_submobjects
                )), \
                patch.object(table, "_family_wrapper_key", side_effect=lambda value: (
                    f"{value._semantic_handle.semanticSlot}:{value._semantic_handle.semanticGeneration}"
                )):
            value = table.MobjectTable(
                [[inputs[0], inputs[1]]],
                element_to_mobject=convert,
                element_to_mobject_config={"scale": 0.5},
            )

        self.assertEqual(calls, [(inputs[0], 0.5), (inputs[1], 0.5)])
        row = value._test_submobjects[1].submobjects[0]
        self.assertIs(row._semantic_member_wrappers["3:1"], converted[0])
        self.assertIs(row._semantic_member_wrappers["4:1"], converted[1])

    def test_mobject_table_element_converter_validates_callable_and_config(self):
        with patch.object(table._base, "Mobject", _FakeMobject), \
                patch.object(table, "_handle_for", side_effect=lambda value: value._semantic_handle):
            with self.assertRaisesRegex(TypeError, "element_to_mobject must be callable"):
                table.MobjectTable([[_FakeMobject(1)]], element_to_mobject=object())
            with self.assertRaisesRegex(TypeError, "element_to_mobject_config must be a dict"):
                table.MobjectTable([[_FakeMobject(1)]], element_to_mobject_config=[])

    def test_table_custom_converter_uses_the_retained_mobject_path(self):
        calls = []
        converter = lambda value, *, color: (value, color)

        def initialize(wrapper, rows, **kwargs):
            calls.append((wrapper, rows, kwargs))

        with patch.object(table.MobjectTable, "__init__", autospec=True, side_effect=initialize):
            value = table.Table(
                [["alpha"]],
                element_to_mobject=converter,
                element_to_mobject_config={"color": "red"},
                h_buff=0.7,
            )

        self.assertIs(calls[0][0], value)
        self.assertEqual(calls[0][1], [["alpha"]])
        self.assertIs(calls[0][2]["element_to_mobject"], converter)
        self.assertEqual(calls[0][2]["element_to_mobject_config"], {"color": "red"})
        self.assertEqual(calls[0][2]["h_buff"], 0.7)

    def test_table_rejects_unbound_paragraph_config_instead_of_ignoring_it(self):
        with self.assertRaisesRegex(NotImplementedError, "requires an explicit shared-Mobject converter"):
            table.Table([["alpha"]], element_to_mobject_config={"font": "DejaVu Sans"})

    def test_numeric_and_math_table_custom_converters_use_the_shared_mobject_path(self):
        calls = []
        converter = lambda value, *, marker: (value, marker)

        def initialize(wrapper, rows, **kwargs):
            calls.append((type(wrapper).__name__, rows, kwargs))

        cases = [
            (table.MathTable, [["x"]]),
            (table.IntegerTable, [[1]]),
            (table.DecimalTable, [[1.5]]),
        ]
        with patch.object(table.MobjectTable, "__init__", autospec=True, side_effect=initialize):
            for constructor, rows in cases:
                constructor(
                    rows,
                    element_to_mobject=converter,
                    element_to_mobject_config={"marker": constructor.__name__},
                )

        self.assertEqual([name for name, _, _ in calls], ["MathTable", "IntegerTable", "DecimalTable"])
        for (_, rows, kwargs), (constructor, source_rows) in zip(calls, cases):
            self.assertEqual(rows, source_rows)
            self.assertIs(kwargs["element_to_mobject"], converter)
            self.assertEqual(kwargs["element_to_mobject_config"]["marker"], constructor.__name__)

    def test_mobject_table_top_left_entry_requires_both_label_sets(self):
        entry = _FakeMobject(1)
        top_left_entry = _FakeMobject(2)
        with patch.object(table._base, "Mobject", _FakeMobject), \
                patch.object(table, "_handle_for", side_effect=lambda value: value._semantic_handle):
            with self.assertRaisesRegex(ValueError, "requires both row_labels and col_labels"):
                table.MobjectTable([[entry]], top_left_entry=top_left_entry)

    def test_table_row_column_and_corner_labels_retain_shared_wrappers(self):
        row_label = _FakeMobject(1)
        column_label = _FakeFamily(2)
        top_left_entry = _FakeMobject(3)
        context = types.SimpleNamespace()
        self.handle._family.submobjects = [
            types.SimpleNamespace(submobjects=[]),
            types.SimpleNamespace(submobjects=[]),
            types.SimpleNamespace(submobjects=[]),
            types.SimpleNamespace(submobjects=[row_label]),
            types.SimpleNamespace(submobjects=[column_label]),
            types.SimpleNamespace(submobjects=[top_left_entry]),
        ]
        calls = []

        def create(rows, row_labels, column_labels, top_left_entries, v_buff, h_buff,
                   outer_lines, inner_lines, red, green, blue, alpha, width, opacity, join, cap, _context):
            calls.append((rows, row_labels, column_labels, top_left_entries))
            self.assertEqual(rows, [["a"]])
            self.assertEqual(row_labels, [row_label._semantic_handle])
            self.assertEqual(column_labels, [column_label._semantic_family_handle])
            self.assertEqual(top_left_entries, [top_left_entry._semantic_handle])
            self.assertEqual((v_buff, h_buff, outer_lines, inner_lines), (0.8, 1.3, False, True))
            return self.handle

        with patch.object(table._base, "Mobject", _FakeMobject), \
                patch.object(table, "_handle_for", side_effect=lambda value: value._semantic_handle), \
                patch.object(table, "_create_table_with_labels", side_effect=create), \
                patch.object(table, "_live_constructor_context", return_value=context), \
                patch.object(table, "_attach_shared_family", side_effect=lambda wrapper, family, _context: (
                    setattr(wrapper, "_test_submobjects", family.submobjects),
                    setattr(wrapper, "_canonical_live_target_context", context),
                )), \
                patch.object(table._compat.Group, "submobjects", new=property(
                    lambda wrapper: wrapper._test_submobjects
                )), \
                patch.object(table, "_family_wrapper_key", side_effect=lambda value: (
                    f"{value._semantic_handle.semanticSlot}:{value._semantic_handle.semanticGeneration}"
                )):
            value = table.Table(
                [["a"]], row_labels=[row_label], col_labels=[column_label],
                top_left_entry=top_left_entry,
            )

        self.assertEqual(calls, [([['a']], [row_label._semantic_handle],
                                  [column_label._semantic_family_handle],
                                  [top_left_entry._semantic_handle])])
        self.assertIs(value.row_labels[0], row_label)
        self.assertIs(value.col_labels[0], column_label)
        self.assertIs(value.top_left_entry, top_left_entry)
        self.assertIs(value._test_submobjects[3]._semantic_member_wrappers["1:1"], row_label)
        self.assertIs(value._test_submobjects[4]._semantic_member_wrappers["2:1"], column_label)
        self.assertIs(value._test_submobjects[5]._semantic_member_wrappers["3:1"], top_left_entry)

    def test_table_top_left_entry_requires_both_label_sets(self):
        with self.assertRaisesRegex(ValueError, "requires both row_labels and col_labels"):
            table.Table([["a"]], top_left_entry=_FakeMobject(1))

    def test_mobject_table_accepts_family_labels_and_rejects_unshared_labels(self):
        with patch.object(table._base, "Mobject", _FakeMobject), \
                patch.object(table, "_handle_for", side_effect=lambda value: value._semantic_handle):
            family = _FakeFamily(1)
            self.assertIs(
                table.MobjectTable._label_handle(family),
                family._semantic_family_handle,
            )
            with self.assertRaisesRegex(TypeError, "current shared semantic identities"):
                with patch.object(table, "_handle_for", return_value=None):
                    table.MobjectTable._label_handle(_FakeMobject(2))
            with self.assertRaisesRegex(NotImplementedError, "unsupported Table option"):
                table._options({"top_left_entry": _FakeMobject(2)})

    def test_include_inner_lines_is_forwarded_and_defaults_to_manimce_true(self):
        self.assertEqual(
            table._options({}),
            (0.8, 1.3, False, True, 1.0, 1.0, 1.0, 1.0, .04, 1.0, "miter", "butt"),
        )
        self.assertEqual(
            table._options({"include_outer_lines": True, "include_inner_lines": False}),
            (0.8, 1.3, True, False, 1.0, 1.0, 1.0, 1.0, .04, 1.0, "miter", "butt"),
        )

    def test_line_config_stroke_subset_is_forwarded_with_manim_width_units(self):
        table.Table(
            [["a"]],
            line_config={"color": "#FF00AA", "stroke_width": 2, "stroke_opacity": .25},
        )
        args = self.context.calls[0][1]
        self.assertEqual(args[5:11], (1.0, 0.0, 170 / 255, 1.0, .02, .25))
        self.assertEqual(args[11:13], ("miter", "butt"))

    def test_line_config_joint_and_cap_styles_accept_manim_enums(self):
        self.assertEqual(
            table._options({"line_config": {
                "joint_type": LineJointType.BEVEL,
                "cap_style": CapStyleType.SQUARE,
            }})[-2:],
            ("bevel", "square"),
        )
        self.assertEqual(
            table._options({"line_config": {"joint_type": "auto", "cap_style": "auto"}})[-2:],
            ("miter", "butt"),
        )
        with self.assertRaisesRegex(ValueError, "joint_type"):
            table._options({"line_config": {"joint_type": "not-a-join"}})
        with self.assertRaisesRegex(ValueError, "cap_style"):
            table._options({"line_config": {"cap_style": "not-a-cap"}})

    def test_unqualified_line_config_options_remain_explicit(self):
        with self.assertRaisesRegex(NotImplementedError, "background_stroke_width"):
            table.Table([["a"]], line_config={"background_stroke_width": 1})


if __name__ == "__main__":
    unittest.main()
