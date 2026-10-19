import unittest
from unittest.mock import Mock

import noon


class ManimMobjectBasePathMethodTests(unittest.TestCase):
    def test_repeat_submobject_returns_the_member_copy(self):
        parent = object.__new__(noon.Mobject)
        member = Mock()
        copied = object()
        member.copy.return_value = copied

        self.assertIs(parent.repeat_submobject(member), copied)
        member.copy.assert_called_once_with()

    def test_base_path_queries_raise_the_pinned_override_error(self):
        mobject = object.__new__(noon.Mobject)

        with self.assertRaisesRegex(
            NotImplementedError, "^Please override in a child class\\.$"
        ):
            mobject.point_from_proportion(0.5)
        with self.assertRaisesRegex(
            NotImplementedError, "^Please override in a child class\\.$"
        ):
            mobject.proportion_from_point((0.0, 0.0, 0.0))


if __name__ == "__main__":
    unittest.main()
