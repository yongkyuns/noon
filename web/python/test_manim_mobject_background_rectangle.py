import unittest
from unittest.mock import Mock, patch

import noon


class ManimMobjectBackgroundRectangleTests(unittest.TestCase):
    def test_family_object_prepends_the_shared_background_and_returns_self(self):
        family = type("Family", (), {})()
        background = object()
        family.add_to_back = Mock(return_value=family)

        with patch(
            "_manim_shared_geometry.BackgroundRectangle",
            return_value=background,
        ) as constructor:
            returned = noon.Mobject.add_background_rectangle(
                family, color=noon.WHITE, opacity=0.4, buff=0.25
            )

        self.assertIs(returned, family)
        self.assertIs(family.background_rectangle, background)
        constructor.assert_called_once_with(
            family, color=noon.WHITE, fill_opacity=0.4, buff=0.25
        )
        family.add_to_back.assert_called_once_with(background)

    def test_bare_object_rejects_before_constructing_a_rectangle(self):
        mobject = object.__new__(noon.Mobject)

        with patch("_manim_shared_geometry.BackgroundRectangle") as constructor:
            with self.assertRaisesRegex(
                NotImplementedError,
                "add_background_rectangle requires a shared family object",
            ):
                mobject.add_background_rectangle(buff=0.25)

        constructor.assert_not_called()


if __name__ == "__main__":
    unittest.main()
