"""Missing font fallbacks must never count as reference qualification."""
import importlib.util
import logging
import os
from pathlib import Path
import sys
from types import SimpleNamespace
import unittest
from unittest import mock

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

    def run_font_registration(self, environment, available_fonts):
        manim = SimpleNamespace(logger=mock.Mock(), Text=mock.Mock(), MarkupText=mock.Mock())
        pango = SimpleNamespace(list_fonts=lambda: available_fonts)
        with mock.patch.dict(os.environ, environment, clear=True), mock.patch.dict(
            sys.modules, {"manim": manim, "manimpango": pango}
        ):
            reference_run.register_fonts()
        return manim

    def test_explicit_default_applies_to_both_text_constructors(self):
        manim = self.run_font_registration(
            {"NOON_MANIM_REFERENCE_TEXT_FONT": "DejaVu Sans Mono"}, ["DejaVu Sans Mono"]
        )
        manim.Text.set_default.assert_called_once_with(font="DejaVu Sans Mono")
        manim.MarkupText.set_default.assert_called_once_with(font="DejaVu Sans Mono")

    def test_original_reference_without_profile_keeps_host_defaults(self):
        manim = self.run_font_registration({}, [])
        manim.Text.set_default.assert_not_called()
        manim.MarkupText.set_default.assert_not_called()

    def test_unavailable_explicit_default_fails_before_rendering(self):
        with self.assertRaisesRegex(RuntimeError, "reference default font is unavailable"):
            self.run_font_registration({"NOON_MANIM_REFERENCE_TEXT_FONT": "Missing"}, [])

    def test_blank_or_padded_default_does_not_silently_select_a_fallback(self):
        for family in ["", " ", " DejaVu Sans Mono"]:
            with self.subTest(family=family), self.assertRaisesRegex(ValueError, "explicit font family"):
                self.run_font_registration({"NOON_MANIM_REFERENCE_TEXT_FONT": family}, [family])
