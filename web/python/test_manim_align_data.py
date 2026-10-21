import unittest
from types import SimpleNamespace
from unittest.mock import Mock

from noon import Mobject


class AlignDataTests(unittest.TestCase):
    def test_align_data_orders_point_family_and_recursive_alignment(self):
        own = SimpleNamespace()
        other = SimpleNamespace()
        events = []
        child = SimpleNamespace(
            align_data=lambda value: events.append(("child", value))
        )
        other_child = object()
        own.submobjects = [child]
        other.submobjects = [other_child]
        own.null_point_align = lambda value: events.append(("null", value))
        own.align_submobjects = lambda value: events.append(("family", value))
        own.align_points = lambda value: events.append(("points", value))

        self.assertIs(Mobject.align_data(own, other), own)
        self.assertEqual(
            events,
            [
                ("null", other),
                ("family", other),
                ("points", other),
                ("child", other_child),
            ],
        )

    def test_skip_point_alignment_skips_only_the_current_level(self):
        own = SimpleNamespace()
        other = SimpleNamespace()
        child = SimpleNamespace(align_data=Mock())
        other_child = object()
        own.submobjects = [child]
        other.submobjects = [other_child]
        own.null_point_align = Mock(return_value=own)
        own.align_submobjects = Mock(return_value=own)
        own.align_points = Mock(return_value=own)

        self.assertIs(Mobject.align_data(own, other, skip_point_alignment=True), own)

        own.null_point_align.assert_called_once_with(other)
        own.align_submobjects.assert_called_once_with(other)
        own.align_points.assert_not_called()
        child.align_data.assert_called_once_with(other_child)

    def test_null_point_alignment_rejects_leaf_to_family_promotion(self):
        own = SimpleNamespace()
        other = SimpleNamespace()
        own.has_points = Mock(return_value=False)
        other.has_points = Mock(return_value=True)

        with self.assertRaisesRegex(NotImplementedError, "point-to-family promotion"):
            Mobject.null_point_align(own, other)

    def test_align_data_rejects_point_ownership_mismatch_before_other_alignment(self):
        own = object.__new__(Mobject)
        other = object.__new__(Mobject)
        own.has_points = Mock(return_value=False)
        other.has_points = Mock(return_value=True)
        own.align_submobjects = Mock()
        own.align_points = Mock()

        with self.assertRaisesRegex(NotImplementedError, "point-to-family promotion"):
            Mobject.align_data(own, other)

        own.align_submobjects.assert_not_called()
        own.align_points.assert_not_called()

    def test_null_point_alignment_is_a_noop_when_point_ownership_matches(self):
        own = SimpleNamespace()
        other = SimpleNamespace()
        own.has_points = Mock(return_value=False)
        other.has_points = Mock(return_value=False)

        self.assertIs(Mobject.null_point_align(own, other), own)


if __name__ == "__main__":
    unittest.main()
