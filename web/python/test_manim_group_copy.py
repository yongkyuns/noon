import unittest

from noon import Arrow, ORIGIN, RIGHT, VGroup


class ManimGroupCopyTests(unittest.TestCase):
    def test_group_subclass_copy_preserves_named_child_references(self) -> None:
        arrow = Arrow(ORIGIN, 2 * RIGHT)
        clone = arrow.copy()

        self.assertIsInstance(clone, Arrow)
        self.assertIsNot(clone, arrow)
        self.assertEqual(len(clone), len(arrow))
        self.assertIs(clone._shaft, clone[0])
        self.assertIs(clone._tip, clone[1])
        self.assertIsNot(clone._shaft, arrow._shaft)
        self.assertIsNot(clone._tip, arrow._tip)

        original_center = arrow.get_center()
        clone.shift(RIGHT)
        self.assertEqual(arrow.get_center(), original_center)
        self.assertNotEqual(clone.get_center(), original_center)

    def test_nested_group_copy_keeps_custom_subclass_clone(self) -> None:
        arrow = Arrow(ORIGIN, 2 * RIGHT)
        family = VGroup(arrow)
        clone = family.copy()

        self.assertIsInstance(clone[0], Arrow)
        self.assertIs(clone[0]._shaft, clone[0][0])
        self.assertIs(clone[0]._tip, clone[0][1])
        self.assertIsNot(clone[0], arrow)


if __name__ == "__main__":
    unittest.main()
