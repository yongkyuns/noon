"""Missing font fallbacks must never count as reference qualification."""
import importlib.util
import logging
import os
from pathlib import Path
import sys
import json
import tempfile
from types import SimpleNamespace
import unittest
from unittest import mock

source = Path(os.environ.get("NOON_FONT_REFERENCE_RUN_SOURCE", Path(__file__).with_name("manim-reference-run.py")))
spec = importlib.util.spec_from_file_location("reference_run", source)
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
        pango = SimpleNamespace(list_fonts=lambda: available_fonts, __version__="0.6.1")
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

    def test_reference_provider_version_is_checked_before_rendering(self):
        self.run_font_registration({"NOON_MANIM_REFERENCE_PANGO_VERSION": "0.6.1"}, [])
        with self.assertRaisesRegex(RuntimeError, "expected ManimPango 0.7.0, found 0.6.1"):
            self.run_font_registration({"NOON_MANIM_REFERENCE_PANGO_VERSION": "0.7.0"}, [])

    def test_blank_or_padded_default_does_not_silently_select_a_fallback(self):
        for family in ["", " ", " DejaVu Sans Mono"]:
            with self.subTest(family=family), self.assertRaisesRegex(ValueError, "explicit font family"):
                self.run_font_registration({"NOON_MANIM_REFERENCE_TEXT_FONT": family}, [family])


fonts_spec = importlib.util.spec_from_file_location("reference_fonts_under_test", Path(__file__).with_name("manim_reference_fonts.py"))
fonts = importlib.util.module_from_spec(fonts_spec)
fonts_spec.loader.exec_module(fonts)
run = reference_run

class ReferenceRendererFontTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.expected = Path(self.temporary.name) / "expected.ttf"
        self.fallback = Path(self.temporary.name) / "fallback.ttf"
        self.expected.write_bytes(b"exact reference face")
        self.fallback.write_bytes(b"different fallback face")

    def face(self, path=None, family="Explicit Mono"):
        return dict(family=family, style="Book", file=str(path or self.expected))

    def test_exact_rendered_font_passes_and_retains_file_hash(self):
        with mock.patch.object(fonts, "_fontconfig_face", return_value=self.face()) as resolve:
            observed = fonts.verify_fontconfig_reference("Explicit Mono", [self.expected])
        resolve.assert_called_once_with("Explicit Mono")
        self.assertEqual(observed["file"], str(self.expected))
        self.assertEqual(len(observed["sha256"]), 64)

    def test_same_family_with_different_bytes_is_rejected(self):
        with mock.patch.object(fonts, "_fontconfig_face", return_value=self.face(self.fallback)):
            with self.assertRaisesRegex(RuntimeError, "rendering font mismatch"):
                fonts.verify_fontconfig_reference("Explicit Mono", [self.expected])

    def test_other_face_in_same_collection_is_rejected(self):
        with mock.patch.object(fonts, "_fontconfig_face", return_value=self.face(family="Fallback")):
            with self.assertRaisesRegex(RuntimeError, "rendering font mismatch"):
                fonts.verify_fontconfig_reference("Explicit Mono", [self.expected])

    def test_unavailable_renderer_api_fails_closed(self):
        for error in (AttributeError("FcFontMatch"), OSError("missing library")):
            with self.subTest(error=error), mock.patch.object(fonts, "_fontconfig_face", side_effect=error):
                with self.assertRaisesRegex(RuntimeError, "could not verify"):
                    fonts.verify_fontconfig_reference("Explicit Mono", [self.expected])

    def test_multiple_supplied_faces_allow_the_resolved_regular_face(self):
        with mock.patch.object(fonts, "_fontconfig_face", return_value=self.face()):
            fonts.verify_fontconfig_reference("Explicit Mono", [self.fallback, self.expected])

    def test_missing_resolved_file_is_rejected(self):
        missing = Path(self.temporary.name) / "missing.ttf"
        with mock.patch.object(fonts, "_fontconfig_face", return_value=self.face(missing)):
            with self.assertRaisesRegex(RuntimeError, "could not verify"):
                fonts.verify_fontconfig_reference("Explicit Mono", [self.expected])

    def registration(self, face):
        manim = SimpleNamespace(logger=mock.Mock(), Text=mock.Mock(), MarkupText=mock.Mock())
        pango = SimpleNamespace(list_fonts=lambda: ["Explicit Mono"], register_font=mock.Mock(return_value=True))
        environment = {"NOON_MANIM_REFERENCE_FONTS": json.dumps([str(self.expected)]),
                       "NOON_MANIM_REFERENCE_TEXT_FONT": "Explicit Mono", "PANGOCAIRO_BACKEND": "fc"}
        return manim, pango, environment, mock.patch.object(fonts, "_fontconfig_face", return_value=face)

    def test_registration_rejects_renderer_fallback_even_when_family_is_listed(self):
        manim, pango, environment, patch = self.registration(self.face(self.fallback, "Fallback"))
        with patch, mock.patch.dict(os.environ, environment, clear=True), mock.patch.dict(
                sys.modules, {"manim": manim, "manimpango": pango, "manim_reference_fonts": fonts}):
            with self.assertRaisesRegex(RuntimeError, "rendering font mismatch"):
                run.register_fonts()
        pango.register_font.assert_called_once_with(str(self.expected.resolve()))
        manim.Text.set_default.assert_not_called()
        manim.MarkupText.set_default.assert_not_called()

    def test_verified_face_is_selected_for_both_text_constructors(self):
        manim, pango, environment, patch = self.registration(self.face())
        with patch, mock.patch.dict(os.environ, environment, clear=True), mock.patch.dict(
                sys.modules, {"manim": manim, "manimpango": pango, "manim_reference_fonts": fonts}):
            run.register_fonts()
        manim.Text.set_default.assert_called_once_with(font="Explicit Mono")
        manim.MarkupText.set_default.assert_called_once_with(font="Explicit Mono")

    def test_font_guard_changes_select_the_required_reference_preflight(self):
        source = Path(os.environ.get("NOON_FONT_WORKFLOW_SOURCE", Path(__file__).resolve().parents[1] / ".github/workflows/manim-raster-differential.yml"))
        workflow = source.read_text()
        for path in ("scripts/manim-reference-run.py", "scripts/manim_reference_fonts.py", "scripts/test_manim_reference_run.py"):
            self.assertIn('      - "' + path + '"', workflow)
        self.assertIn("python3 -m unittest scripts.test_manim_reference_run", workflow)

    def test_non_fontconfig_profiles_keep_existing_host_registration(self):
        manim, pango, environment, patch = self.registration(self.face(self.fallback, "Fallback"))
        environment["PANGOCAIRO_BACKEND"] = "coretext"
        with patch as resolver, mock.patch.dict(os.environ, environment, clear=True), mock.patch.dict(
                sys.modules, {"manim": manim, "manimpango": pango, "manim_reference_fonts": fonts}):
            run.register_fonts()
        resolver.assert_not_called()
        manim.Text.set_default.assert_called_once_with(font="Explicit Mono")
