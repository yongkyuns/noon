from __future__ import annotations

import unittest

import noon as _base
import _manim_namespace as namespace


class ManimNamespaceTests(unittest.TestCase):
    def setUp(self) -> None:
        namespace.install()

    def test_optional_alias_detection_uses_python_ast(self) -> None:
        self.assertEqual(
            namespace.required_packages(
                "from noon import *\nclass Example(Scene):\n"
                "    def construct(self):\n        value = np.log(2.0)\n"
            ),
            ("numpy",),
        )
        self.assertEqual(
            namespace.required_packages(
                "from noon import *\nclass Example(Scene):\n"
                "    def construct(self):\n        self.add(Circle())\n"
            ),
            (),
        )
        self.assertEqual(
            namespace.required_packages("message = 'np.log is only text here'\n"),
            (),
        )

    def test_install_exposes_pending_alias_without_importing_numpy(self) -> None:
        self.assertIn("np", _base.__all__)
        pending = _base.np
        if type(pending).__name__ == "_PendingOptionalNamespace":
            with self.assertRaisesRegex(RuntimeError, "requires optional package 'numpy'"):
                pending.log

    def test_array_helpers_load_numpy_for_direct_aliased_and_module_calls(self) -> None:
        for source in ["stretch_array_to_length(values, 3)",
                       "from manim import stretch_array_to_length as stretch\nstretch(values, 3)",
                       "import noon\nnoon.stretch_array_to_length(values, 3)",
                       "np.array([1]); stretch_array_to_length(values, 3)"]:
            self.assertEqual(namespace.required_packages(source), ("numpy",))
        self.assertEqual(namespace.required_packages("message = 'stretch_array_to_length'"), ())

    def test_pure_colors_match_manim_v021(self) -> None:
        expected = {
            "PURE_RED": (1.0, 0.0, 0.0),
            "PURE_GREEN": (0.0, 1.0, 0.0),
            "PURE_BLUE": (0.0, 0.0, 1.0),
            "PURE_CYAN": (0.0, 1.0, 1.0),
            "PURE_MAGENTA": (1.0, 0.0, 1.0),
            "PURE_YELLOW": (1.0, 1.0, 0.0),
        }
        for name, rgb in expected.items():
            color = getattr(_base, name)
            self.assertEqual((color.red, color.green, color.blue), rgb)
            self.assertIn(name, _base.__all__)

    def test_named_shade_aliases_and_independent_pinned_values(self) -> None:
        for first, second in (("GRAY_B", "LIGHT_GREY"), ("GRAY_E", "DARKER_GRAY"), ("BLUE_E", "DARK_BLUE")):
            self.assertEqual(getattr(_base, first), getattr(_base, second))
        self.assertEqual(_base.GOLD_E, _base.color_from_hex("#C78D46"))
        self.assertEqual(_base.MAROON_C, _base.color_from_hex("#C55F73"))
        self.assertEqual(_base.LOGO_RED, _base.color_from_hex("#E07A5F"))

    def test_font_tokens_defaults_and_axis_values(self) -> None:
        self.assertEqual((_base.BOLD, _base.ITALIC, _base.ULTRALIGHT), ("BOLD", "ITALIC", "ULTRALIGHT"))
        self.assertEqual((_base.DEFAULT_FONT_SIZE, _base.DEFAULT_STROKE_WIDTH, _base.DEFAULT_WAIT_TIME), (48, 4, 1.0))
        self.assertEqual(_base.SCALE_FACTOR_PER_FONT_POINT, 1 / 960)
        self.assertIs(_base.X_AXIS, _base.RIGHT)
        self.assertIs(_base.Y_AXIS, _base.UP)
        self.assertIs(_base.Z_AXIS, _base.OUT)
        namespace.install()
        self.assertEqual(len(_base.__all__), len(set(_base.__all__)))


if __name__ == "__main__":
    unittest.main()
