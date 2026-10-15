import unittest
from types import ModuleType
from unittest.mock import patch

import noon
from _manim_color_palettes import _Palette


class ColorPalettes(unittest.TestCase):
    def test_constant_lookup_is_lazy_cached_and_unknown_names_fail(self):
        with patch.object(noon, "color_from_hex", wraps=noon.color_from_hex) as construct:
            palette = _Palette("test", {"RED": 0xFF0000, "BLUE": 0x0000FF})
            construct.assert_not_called()
            self.assertIn("BLUE", dir(palette))
            self.assertNotIn("RED", vars(palette))
            first = palette.RED
            self.assertIs(palette.RED, first)
            self.assertEqual((first.red, first.green, first.blue, first.alpha), (1, 0, 0, 1))
            construct.assert_called_once_with(0xFF0000)
            with self.assertRaisesRegex(AttributeError, "test.*MISSING"):
                palette.MISSING
            self.assertNotIn("BLUE", vars(palette))

    def test_every_palette_is_a_public_namespace_with_pinned_sentinel_values(self):
        expected = {"AS2700": ("B11_RICH_BLUE", 0x2B3770),
                    "BS381": ("BS381_101", 0x94BFAC),
                    "DVIPSNAMES": ("RED", 0xED1B23),
                    "SVGNAMES": ("ALICEBLUE", 0xEFF7FF),
                    "X11": ("ALICEBLUE", 0xF0F8FF),
                    "XKCD": ("AMBER", 0xFEB308)}
        for name, (key, value) in expected.items():
            with self.subTest(name=name):
                self.assertIn(name, noon.__all__)
                palette = getattr(noon, name)
                self.assertIsInstance(palette, ModuleType)
                self.assertEqual(getattr(palette, key), noon.color_from_hex(value))


if __name__ == "__main__":
    unittest.main()
