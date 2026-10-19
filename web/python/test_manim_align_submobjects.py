import unittest
from unittest.mock import PropertyMock, call, patch

from _manim_compat import Group


class AlignSubmobjectsTests(unittest.TestCase):
    def test_pads_the_smaller_family_by_the_size_delta_and_returns_self(self):
        cases = (
            (2, 4, 2, 0),
            (4, 2, 0, 2),
            (3, 3, 0, 0),
        )
        for own_count, other_count, own_growth, other_growth in cases:
            with self.subTest(own_count=own_count, other_count=other_count):
                own = object.__new__(Group)
                other = object.__new__(Group)
                with (
                    patch.object(Group, "submobjects", new_callable=PropertyMock) as children,
                    patch.object(Group, "add_n_more_submobjects", autospec=True) as grow,
                ):
                    children.side_effect = [
                        [object() for _ in range(own_count)],
                        [object() for _ in range(other_count)],
                    ]
                    result = Group.align_submobjects(own, other)

                self.assertIs(result, own)
                self.assertEqual(
                    grow.call_args_list,
                    [call(own, own_growth), call(other, other_growth)],
                )

    def test_mixed_family_growth_rejects_before_mutating_the_supported_side(self):
        own = object.__new__(Group)
        other = object()
        with (
            patch.object(Group, "submobjects", new_callable=PropertyMock) as children,
            patch.object(Group, "add_n_more_submobjects", autospec=True) as grow,
        ):
            children.return_value = [object(), object()]
            with self.assertRaisesRegex(NotImplementedError, "family-backed growth"):
                Group.align_submobjects(own, other)

        grow.assert_not_called()


if __name__ == "__main__":
    unittest.main()
