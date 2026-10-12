"""Missing font fallbacks must never count as reference qualification."""
import importlib.util
import logging
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location("reference_run", Path(__file__).with_name("manim-reference-run.py"))
reference_run = importlib.util.module_from_spec(spec)
spec.loader.exec_module(reference_run)


class ExactFontTests(unittest.TestCase):
    def test_missing_font_fallback_fails_closed(self):
        record = logging.LogRecord("manim", logging.WARNING, "", 0, "Font %s not in %s", ("Missing", ["Installed"]), None)
        with self.assertRaisesRegex(RuntimeError, "unavailable font"):
            reference_run.ExactFontFilter().filter(record)

    def test_other_reference_diagnostics_are_preserved(self):
        record = logging.LogRecord("manim", logging.WARNING, "", 0, "Other diagnostic", (), None)
        self.assertTrue(reference_run.ExactFontFilter().filter(record))
