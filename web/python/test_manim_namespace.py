from __future__ import annotations

import unittest

import noon as _base
import _manim_namespace as namespace


class ManimNamespaceTests(unittest.TestCase):
    def setUp(self) -> None:
        namespace.install()

    def test_optional_alias_detection_uses_python_syntax(self) -> None:
        self.assertEqual(
            namespace.required_packages(
                "from noon import *\nclass Example(Scene):\n    def construct(self):\n        value = np.log(2.0)\n"
            ),
            ("numpy",),
        )
        self.assertEqual(
            namespace.required_packages(
                "from noon import *\nclass Example(Scene):\n    def construct(self):\n        self.add(Circle())\n"
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


if __name__ == "__main__":
    unittest.main()
