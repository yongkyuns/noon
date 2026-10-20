import unittest
from unittest.mock import Mock

from noon import Mobject, RIGHT


class LeafArrangementTests(unittest.TestCase):
    def test_default_arrangement_centers_the_leaf(self):
        value = object.__new__(Mobject)
        value.center = Mock(return_value=value)

        self.assertIs(Mobject.arrange(value), value)
        value.center.assert_called_once_with()

    def test_center_false_keeps_the_leaf_position(self):
        value = object.__new__(Mobject)
        value.center = Mock(return_value=value)

        self.assertIs(Mobject.arrange(value, RIGHT, 0.4, False), value)
        value.center.assert_not_called()

    def test_arrange_submobjects_dispatches_to_dynamic_arrange(self):
        value = object.__new__(Mobject)
        arrange = Mock(return_value=value)
        value.arrange = arrange

        self.assertIs(
            Mobject.arrange_submobjects(value, RIGHT, 0.4, False, aligned_edge=RIGHT),
            value,
        )
        arrange.assert_called_once_with(RIGHT, 0.4, False, aligned_edge=RIGHT)


if __name__ == "__main__":
    unittest.main()
