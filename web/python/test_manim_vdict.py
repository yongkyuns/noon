import unittest
from unittest.mock import patch

import noon
from _manim_compat import VDict, VGroup, VMobject


class VDictTests(unittest.TestCase):
    def test_constructor_uses_shared_vgroup_and_adds_mapping_values_in_order(self):
        first = object.__new__(VMobject)
        second = object.__new__(VMobject)
        added = []
        initialized = []

        def initialize(value, **kwargs):
            initialized.append((value, kwargs))

        with patch.object(VGroup, "__init__", initialize), patch.object(
            VGroup, "add", lambda value, member: added.append((value, member)) or value
        ):
            result = VDict([("first", first), ("second", second)], color="red")

        self.assertIs(result, added[0][0])
        self.assertEqual(initialized, [(result, {"color": "red"})])
        self.assertEqual(list(result.submob_dict), ["first", "second"])
        self.assertIs(result["first"], first)
        self.assertEqual([member for _, member in added], [first, second])
        self.assertFalse(result.show_keys)
        self.assertIs(noon.VDict, VDict)
        self.assertIsInstance(result, VMobject)

    def test_mapping_operations_preserve_manims_key_and_family_semantics(self):
        first = object.__new__(VMobject)
        second = object.__new__(VMobject)
        replacement = object.__new__(VMobject)
        value = VDict.__new__(VDict)
        value.show_keys = False
        value.submob_dict = {"first": first, "second": second}

        added = []
        removed = []
        with patch.object(
            VGroup, "add", lambda owner, member: added.append(member) or owner
        ), patch.object(
            VGroup, "remove", lambda owner, member: removed.append(member) or owner
        ):
            self.assertIs(value.add([("third", replacement)]), value)
            self.assertIs(value.remove("third"), value)
            self.assertIsNone(value.__setitem__("second", replacement))
            values = value.get_all_submobjects()
            del value["first"]

        self.assertIs(value["second"], replacement)
        self.assertEqual(list(values), [replacement])
        self.assertEqual(list(value.submob_dict), ["second"])
        self.assertEqual(added, [replacement, replacement])
        self.assertEqual(removed, [replacement, second])
        with self.assertRaisesRegex(KeyError, "given key 'missing'.*not present"):
            value.remove("missing")

    def test_constructor_collapses_duplicate_mapping_keys_and_rejects_key_labels(self):
        first = object.__new__(VMobject)
        last = object.__new__(VMobject)
        added = []
        with patch.object(VGroup, "__init__"), patch.object(
            VGroup, "add", lambda owner, member: added.append(member) or owner
        ):
            value = VDict([("same", first), ("same", last)])
        self.assertEqual(list(value.submob_dict), ["same"])
        self.assertIs(value["same"], last)
        self.assertEqual(added, [last])

        with patch.object(VGroup, "__init__") as initialize:
            with self.assertRaisesRegex(NotImplementedError, "show_keys"):
                VDict(show_keys=True)
        initialize.assert_not_called()

    def test_values_must_be_vector_mobjects(self):
        with patch.object(VGroup, "__init__"), patch.object(VGroup, "add"):
            value = VDict()
        with self.assertRaisesRegex(TypeError, "VDict values must be VMobjects"):
            value.add_key_value_pair("invalid", object())


if __name__ == "__main__":
    unittest.main()
