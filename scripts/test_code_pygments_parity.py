"""Independent ManimCE 0.21.0 Code syntax-color oracle (Pygments 2.19.2).

This checks selected lexer/theme token colors, NOT Manim raster/pixel parity.
The engine's native Syntect classifier is checked separately by Rust unit tests
against the same frozen, versioned observations.
"""
import ast
import json
from pathlib import Path
import unittest

import pygments
from pygments.lexers import get_lexer_by_name
from pygments.styles import get_style_by_name

ROOT = Path(__file__).resolve().parents[1]
ORACLE = ROOT / "fixtures/code/pygments-2.19.2-code-colors.json"
SHOWCASE = ROOT / "web/python/examples/showcase_code_blocks.py"


def showcase_sources():
    source = ast.parse(SHOWCASE.read_text(encoding="utf-8"))
    named = {}
    for node in source.body:
        if isinstance(node, ast.Assign) and len(node.targets) == 1 and isinstance(node.targets[0], ast.Name):
            if node.targets[0].id in ("PYTHON_SOURCE", "CUDA_SOURCE"):
                named[node.targets[0].id] = ast.literal_eval(node.value)
    return named


class CodePygmentsParity(unittest.TestCase):
    def test_pinned_reference_matches_actual_pygments(self):
        self.assertEqual(pygments.__version__, "2.19.2", "Use ManimCE 0.21.0's pinned Pygments version")
        oracle = json.loads(ORACLE.read_text(encoding="utf-8"))
        self.assertEqual(oracle["reference"]["manim"], "0.21.0")
        self.assertEqual(oracle["reference"]["pygments"], pygments.__version__)
        gallery = showcase_sources()
        self.assertEqual(len(gallery), 2)
        ids = [case["id"] for case in oracle["cases"]]
        self.assertEqual(len(ids), len(set(ids)), "duplicate reference case")
        for case in oracle["cases"]:
            with self.subTest(case=case["id"]):
                if case["id"] == "python-monokai-gallery":
                    self.assertEqual(case["source"], gallery["PYTHON_SOURCE"])
                if case["id"] == "cuda-emacs-gallery":
                    self.assertEqual(case["source"], gallery["CUDA_SOURCE"])
                source = case["source"]
                tokens = list(get_lexer_by_name(case["language"]).get_tokens_unprocessed(source))
                palette = get_style_by_name(case["style"])
                lines = source.split("\n")
                for probe in case["probes"]:
                    line = probe["line"]
                    start = probe["start"]
                    needle = probe["text"]
                    self.assertEqual(lines[line][start : start + len(needle)], needle)
                    absolute = sum(len(s) + 1 for s in lines[:line]) + start
                    matches = [(token, text) for offset, token, text in tokens
                               if offset <= absolute < offset + len(text)]
                    self.assertEqual(len(matches), 1, (case["id"], probe))
                    token, _ = matches[0]
                    self.assertEqual(str(token), probe["token"])
                    color = palette.style_for_token(token).get("color")
                    self.assertIsNotNone(color)
                    self.assertEqual(color.lower(), probe["rgb"])


if __name__ == "__main__":
    unittest.main()
