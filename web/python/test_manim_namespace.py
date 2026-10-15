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

    def test_standard_library_exports_preserve_identity_and_metadata_errors(self) -> None:
        import __future__
        import importlib.metadata as metadata
        import sys
        import tempfile
        from pathlib import Path
        from unittest.mock import patch

        for name, expected in [("annotations", __future__.annotations),
                               ("PackageNotFoundError", metadata.PackageNotFoundError),
                               ("version", metadata.version)]:
            self.assertIn(name, _base.__all__)
            self.assertIs(getattr(_base, name), expected)
        with tempfile.TemporaryDirectory() as directory:
            info = Path(directory) / "noon_namespace_fixture-1.2.3.dist-info"
            info.mkdir()
            (info / "METADATA").write_text(
                "Metadata-Version: 2.1\nName: noon-namespace-fixture\nVersion: 1.2.3\n"
            )
            with patch.object(sys, "path", [directory, *sys.path]):
                self.assertEqual(_base.version("noon-namespace-fixture"), "1.2.3")
                with self.assertRaises(_base.PackageNotFoundError) as caught:
                    _base.version("noon-namespace-absent")
                self.assertEqual(caught.exception.name, "noon-namespace-absent")

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

    def test_slow_into_loads_numpy_only_when_used(self):
        for source in ["slow_into(0.5)", "from manim import slow_into as slow\nslow(0.5)",
                       "import noon\nnoon.slow_into(0.5)"]:
            self.assertEqual(namespace.required_packages(source), ("numpy",))
        self.assertEqual(namespace.required_packages("double_smooth(0.5)"), ("numpy",))

    def test_nonstandard_rate_data_loads_numpy_before_direct_or_aliased_calls(self):
        for name in ('smooth', 'rush_into', 'rush_from', 'there_and_back',
                     'there_and_back_with_pause', 'wiggle', 'exponential_decay'):
            for source in (f'{name}(0.5)',
                           f'from manim import {name} as helper\nhelper(0.5)',
                           f'import noon\nnoon.{name}(0.5)'):
                with self.subTest(source=source):
                    self.assertEqual(namespace.required_packages(source), ('numpy',))
        self.assertEqual(namespace.required_packages('running_start(0.5)'), ())
        self.assertEqual(namespace.required_packages("message = 'wiggle(0.5)'"), ())

    def test_default_proportion_wrapper_prepares_its_numpy_smooth_dependency(self):
        for source in ('not_quite_there()(0.5)',
                       'from manim import not_quite_there as helper\nhelper()(0.5)',
                       'import noon\nnoon.not_quite_there()(0.5)'):
            self.assertEqual(namespace.required_packages(source), ('numpy',))

    def test_interpolation_requires_numpy_only_for_division_helpers(self):
        for name in ["inverse_interpolate", "match_interpolate"]:
            self.assertEqual(namespace.required_packages(f"from manim import {name} as helper\nhelper(0, 1, 0.5)"), ("numpy",))
        for name in ["interpolate", "integer_interpolate", "mid"]:
            self.assertEqual(namespace.required_packages(f"{name}(0, 1, 0.5)"), ())

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
