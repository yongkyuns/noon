import unittest

import noon


class ManimMobjectBaseLayoutMethodTests(unittest.TestCase):
    def test_base_get_point_mobject_raises_the_pinned_error(self):
        mobject = object.__new__(noon.Mobject)

        with self.assertRaisesRegex(
            NotImplementedError,
            "^get_point_mobject not implemented for Mobject$",
        ):
            mobject.get_point_mobject()

    def test_align_points_delegates_to_the_side_with_fewer_points(self):
        for left_count, right_count, expected_dispatch in (
            (2, 5, "left"),
            (5, 2, "right"),
            (3, 3, None),
        ):
            with self.subTest(left_count=left_count, right_count=right_count):
                left = object.__new__(noon.Mobject)
                right = object.__new__(noon.Mobject)
                left.get_num_points = lambda: left_count
                right.get_num_points = lambda: right_count
                dispatches = []
                left.align_points_with_larger = lambda larger: dispatches.append(
                    "left" if larger is right else "left-other"
                )
                right.align_points_with_larger = lambda larger: dispatches.append(
                    "right" if larger is left else "right-other"
                )

                self.assertIs(left.align_points(right), left)
                self.assertEqual(
                    dispatches,
                    [] if expected_dispatch is None else [expected_dispatch],
                )

    def test_base_align_points_with_larger_raises_the_pinned_error(self):
        mobject = object.__new__(noon.Mobject)
        larger = object.__new__(noon.Mobject)

        with self.assertRaisesRegex(
            NotImplementedError, "^Please override in a child class\\.$"
        ):
            mobject.align_points_with_larger(larger)


if __name__ == "__main__":
    unittest.main()
