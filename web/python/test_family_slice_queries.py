import unittest
from types import SimpleNamespace
from unittest.mock import Mock

import _manim_semantic_handles as handles
import test_family_indexed_queries as indexed


class FamilySliceQueryTests(unittest.TestCase):
    def group(self, *, own=False, keys=("0:1", "1:1")):
        root, _, values, bridge = indexed.FamilyIndexedQueryTests().group()
        plan = SimpleNamespace(includesSelf=own, normalizationCount=2,
                               selectNormalized=Mock(return_value=list(keys)), free=Mock())
        bridge.prepareSlice = Mock(return_value=plan)
        root.get_group_class = Mock(return_value=lambda *members: tuple(members))
        return root, values, bridge, plan

    def test_point_free_coercion_precedes_captured_list_length(self):
        root, values, bridge, base = self.group(keys=("1:1",))
        events = []
        class Plan:
            includesSelf = False
            free = base.free
            @property
            def normalizationCount(self):
                events.append("count")
                return 1
            def selectNormalized(self, *args):
                events.append("select")
                return base.selectNormalized(*args)
        bridge.prepareSlice.side_effect = lambda include: (events.append("capture"), Plan())[1]
        root.get_group_class.side_effect = lambda: (events.append("class"), lambda *members: tuple(members))[1]
        class Operand:
            def __init__(self, phase, value): self.phase, self.value = phase, value
            def __index__(self):
                events.append(self.phase)
                return self.value
        self.assertEqual(root[slice(Operand("start", 0), Operand("stop", 3), Operand("step", 1))], (values[1],))
        self.assertEqual(events, ["class", "capture", "step", "start", "stop", "count", "select"])
        base.selectNormalized.assert_called_once_with(0, 1, 1)
        base.free.assert_called_once_with()
        self.assertEqual(root._semantic_member_reference_scope.readers, 0)

    def test_own_root_normalizes_against_length_before_index_callbacks(self):
        root, values, bridge, plan = self.group(own=True, keys=("99:1", "1:1"))
        bridge.splitCount = Mock(return_value=3)
        class Start:
            def __index__(self):
                bridge.splitCount.return_value = 1
                plan.includesSelf = False
                return 0
        self.assertEqual(root[slice(Start(), 3, 1)], (root, values[1]))
        bridge.splitCount.assert_called_once_with(None)
        plan.selectNormalized.assert_called_once_with(0, 3, 1)
        plan.free.assert_called_once_with()

    def test_missing_current_rank_is_list_index_error_and_releases_plan(self):
        root, _, _, plan = self.group()
        plan.selectNormalized.return_value = None
        with self.assertRaisesRegex(IndexError, "^list index out of range$"):
            root[:]
        plan.free.assert_called_once_with()
        self.assertEqual(root._semantic_member_reference_scope.readers, 0)

    def test_slice_diagnostics_preserve_user_protocol_errors(self):
        class Broken:
            def __index__(self): raise TypeError("user index error")
        for value, error, message in [(0, ValueError, "slice step cannot be zero"),
                                      (1.5, TypeError, "slice indices must be integers or None or have an __index__ method"),
                                      (Broken(), TypeError, "user index error")]:
            with self.subTest(value=value):
                root, _, _, plan = self.group()
                with self.assertRaisesRegex(error, "^" + message + "$"):
                    root[::value]
                plan.free.assert_called_once_with()
                plan.selectNormalized.assert_not_called()

    def test_arbitrary_size_step_is_coerced_before_exact_js_integer_conversion(self):
        root, values, _, plan = self.group(keys=("0:1",))
        self.assertEqual(root[::10**1000], (values[0],))
        plan.selectNormalized.assert_called_once_with(0, 2, (1 << 53) - 1)
        self.assertTrue(all(type(value) is float for value in plan.selectNormalized.call_args.args))

    def test_class_hook_errors_precede_native_capture(self):
        root, _, bridge, _ = self.group()
        root.get_group_class.side_effect = RuntimeError("class hook")
        with self.assertRaisesRegex(RuntimeError, "class hook"):
            root[:]
        bridge.prepareSlice.assert_not_called()

    def test_captured_members_keep_original_wrapper_references_after_replacement(self):
        root, values, _, plan = self.group(keys=("0:1",))
        retained = []
        class Start:
            def __index__(self):
                retained.append(root._semantic_member_reference_scope)
                handles._fork_group_wrapper_references(root)
                root._semantic_member_wrappers.pop("0:1")
                return 0
        self.assertEqual(root[slice(Start(), 1)], (values[0],))
        self.assertEqual(retained[0].readers, 0)
        plan.free.assert_called_once_with()

    def test_constructor_runs_after_plan_release_and_keeps_its_exception(self):
        root, _, _, plan = self.group()
        def constructor(*members):
            plan.free.assert_called_once_with()
            raise LookupError("constructor")
        root.get_group_class.return_value = constructor
        with self.assertRaisesRegex(LookupError, "constructor"):
            root[:]
