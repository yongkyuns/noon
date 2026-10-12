import copy
import pickle
import unittest

import noon
from noon import DictAsObject, merge_dicts_recursively, update_dict_recursively


class ConfigUtilities(unittest.TestCase):
    def test_recursive_precedence_replacement_and_input_aliases(self):
        untouched = {"list": [1, 2]}
        left = {"nested": {"left": 1, "shared": 2}, "untouched": untouched, "replace": {"a": 1}}
        right = {"nested": {"right": 3, "shared": 4}, "replace": 8}
        result = merge_dicts_recursively(left, right, {"nested": {"last": 5}})
        self.assertEqual(result, {"nested": {"left": 1, "shared": 4, "right": 3, "last": 5},
                                  "untouched": untouched, "replace": 8})
        self.assertEqual(left["nested"], {"left": 1, "shared": 2})
        self.assertIs(result["untouched"], untouched)
        self.assertIsNot(result["nested"], left["nested"])
        self.assertEqual(merge_dicts_recursively(), {})
        self.assertEqual(merge_dicts_recursively({"x": 3}, {"x": {"a": 1}}), {"x": {"a": 1}})

    def test_update_retains_outer_identity_and_replaces_only_merged_children(self):
        child = {"x": 1}
        values = {"child": child, "other": [2]}
        outer = values
        self.assertIsNone(update_dict_recursively(values, {"child": {"y": 3}}))
        self.assertIs(values, outer)
        self.assertEqual(values, {"child": {"x": 1, "y": 3}, "other": [2]})
        self.assertIsNot(values["child"], child)
        self.assertEqual(child, {"x": 1})

    def test_dict_as_object_retains_bidirectional_attribute_identity(self):
        values = {"x": 1, "nested": {"a": [2]}}
        obj = DictAsObject(dictin=values)
        self.assertIs(obj.__dict__, values)
        obj.x = 3
        self.assertEqual(values["x"], 3)
        values["new"] = 4
        self.assertEqual(obj.new, 4)
        del obj.x
        self.assertNotIn("x", values)
        with self.assertRaises(AttributeError):
            _ = obj.x
        with self.assertRaises(TypeError):
            DictAsObject([])
        for cloned in [copy.deepcopy(obj), pickle.loads(pickle.dumps(obj))]:
            self.assertEqual(cloned.__dict__, values)
            self.assertIsNot(cloned.__dict__, values)

    def test_items_protocol_order_matches_manim_and_exports_keep_identity(self):
        calls = []
        class Recorded(dict):
            def items(self):
                calls.append((self["key"], "items"))
                def entries():
                    calls.append((self["key"], "iterate"))
                    yield from super(Recorded, self).items()
                return entries()
        self.assertEqual(merge_dicts_recursively(Recorded(key=1), Recorded(key=2)), {"key": 2})
        self.assertEqual(calls, [(1, "items"), (2, "items"), (1, "iterate"), (2, "iterate")])
        namespace = {}
        exec("from noon import *", namespace)
        for name in ["DictAsObject", "merge_dicts_recursively", "update_dict_recursively"]:
            self.assertIs(namespace[name], getattr(noon, name))
            self.assertEqual(noon.__all__.count(name), 1)


if __name__ == "__main__":
    unittest.main()
