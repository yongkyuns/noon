import types
import unittest
from unittest.mock import PropertyMock, patch

import _manim_table as table


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


class _Context:
    def __init__(self, handle):
        self.handle = handle
        self.calls = []

    def liveCreateTable(self, *args):
        self.calls.append(("create", args))
        return self.handle


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
                          return_value=[None, None, highlights]):
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


if __name__ == "__main__":
    unittest.main()
