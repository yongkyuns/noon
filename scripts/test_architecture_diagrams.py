#!/usr/bin/env python3
"""Regression tests with the real pinned D2 renderer, not a copied SVG fixture."""
from __future__ import annotations

from contextlib import redirect_stderr, redirect_stdout
import io
import os
from pathlib import Path
import tempfile
import unittest

import architecture_diagrams as diagrams


class ArchitectureDiagramsTest(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory(prefix="noon-diagram-test-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.directory = self.root / "docs/diagrams"
        self.directory.mkdir(parents=True)
        self.tool = os.environ.get("D2", "d2")
        self.source = self.directory / "sample.d2"
        self.source.write_text("a -> b\n", encoding="utf-8")
        (self.root / "README.md").write_text("![sample](docs/diagrams/sample.svg)\n", encoding="utf-8")
        (self.root / "docs/architecture.md").write_text("[source](diagrams/sample.d2)\n", encoding="utf-8")

    def render(self, check: bool = False) -> None:
        with redirect_stdout(io.StringIO()), redirect_stderr(io.StringIO()):
            diagrams.render(self.root, self.tool, check)

    def snapshot(self) -> dict[str, bytes]:
        return {str(path.relative_to(self.root)): path.read_bytes()
                for path in self.root.rglob("*") if path.is_file()}

    def test_regeneration_is_deterministic_and_check_is_readonly(self) -> None:
        self.render()
        before = self.snapshot()
        self.render()
        self.assertEqual(before, self.snapshot())
        self.render(check=True)
        self.assertEqual(before, self.snapshot())

    def test_stale_output_fails_without_repairing(self) -> None:
        self.render()
        self.source.write_text("a -> c\n", encoding="utf-8")
        before = self.snapshot()
        with self.assertRaisesRegex(ValueError, "stale/missing"):
            self.render(check=True)
        self.assertEqual(before, self.snapshot())

    def test_missing_output_fails_without_creating_it(self) -> None:
        with self.assertRaisesRegex(ValueError, "stale/missing"):
            self.render(check=True)
        self.assertFalse(self.source.with_suffix(".svg").exists())

    def test_orphan_output_is_rejected(self) -> None:
        (self.directory / "orphan.svg").write_text("orphan", encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "orphan"):
            self.render()

    def test_imported_style_changes_invalidate_output(self) -> None:
        style = self.directory / "_style.d2"
        style.write_text("a.style.stroke-width: 2\n", encoding="utf-8")
        self.source.write_text("...@_style\na -> b\n", encoding="utf-8")
        self.render()
        style.write_text("a.style.stroke-width: 3\n", encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "stale/missing"):
            self.render(check=True)

    def test_broken_link_is_rejected(self) -> None:
        self.render()
        (self.root / "README.md").write_text("![sample](docs/diagrams/missing.svg)\n", encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "broken diagram link"):
            self.render(check=True)

    def test_unreferenced_diagram_is_rejected(self) -> None:
        self.render()
        (self.root / "README.md").write_text("No image\n", encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "unreferenced"):
            self.render(check=True)

    def test_invalid_source_does_not_partially_regenerate(self) -> None:
        self.render()
        self.source.write_text("a -> c\n", encoding="utf-8")
        (self.directory / "z-invalid.d2").write_text("broken: {\n", encoding="utf-8")
        old_svg = self.source.with_suffix(".svg").read_bytes()
        with self.assertRaises(diagrams.subprocess.CalledProcessError):
            self.render()
        self.assertEqual(old_svg, self.source.with_suffix(".svg").read_bytes())

    def test_environment_cannot_enable_sketch_or_change_scale(self) -> None:
        from unittest.mock import patch
        self.render()
        before = self.snapshot()
        with patch.dict(os.environ, {"D2_SKETCH": "true", "SCALE": "0.5", "D2_FONT_REGULAR": "/absent.ttf"}):
            self.render(check=True)
        self.assertEqual(before, self.snapshot())

    def test_wrong_version_rejected_before_rendering(self) -> None:
        from unittest.mock import patch
        with patch.object(diagrams.subprocess, "check_output", return_value="v0.0.0\n"):
            with self.assertRaisesRegex(ValueError, "expected D2"):
                self.render()
        self.assertFalse(self.source.with_suffix(".svg").exists())


if __name__ == "__main__":
    unittest.main()
