"""The normal unittest gate must exercise shared-boundary requirements."""
from pathlib import Path
import unittest

import _manim_compat as compat
import _manim_typst as typst
import noon


class SharedBindingTests(unittest.TestCase):
    def test_phase_specific_installation_layer_is_removed(self):
        self.assertFalse((Path(__file__).parent / "_manim_phase_b.py").exists())

    def test_text_construction_requires_shared_rust(self):
        compat.install()
        typst.install()
        with self.assertRaisesRegex(RuntimeError, "Text requires Noon's shared Rust authoring runtime"):
            typst.Text("AB")


if __name__ == "__main__":
    unittest.main()
