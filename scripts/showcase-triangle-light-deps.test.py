"""Pinned loader resolution and complete-only archives, without network access."""
import importlib.util
import json
from pathlib import Path
import re
import tempfile
import unittest

SCRIPT = Path(__file__).with_name("showcase-triangle-light-deps.py")
SPEC = importlib.util.spec_from_file_location("triangle_deps", SCRIPT)
DEPS = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(DEPS)
BASE = "https://cdn.jsdelivr.net/pyodide/v314.0.5/full/"


class DependencyArchiveTests(unittest.TestCase):
    def make_root(self, directory):
        root = Path(directory)
        (root / "web").mkdir()
        (root / "web/python-worker.js").write_text(f'import {{ loadPyodide }} from "{BASE}pyodide.mjs";')
        return root

    def test_es_module_filename_comes_from_the_pinned_loader(self):
        with tempfile.TemporaryDirectory() as directory:
            root = self.make_root(directory)
            # Same module expression observed in the actual v314.0.5 loader.
            resources = {"pyodide.mjs": b'let t=`${e.indexURL}pyodide.asm.mjs`;return(await O(t)).default'}
            resources.update({name: name.encode() for name in
                              ("pyodide.asm.mjs", "pyodide.asm.wasm", "python_stdlib.zip", "pyodide-lock.json")})
            requested = []
            def fetch(url):
                self.assertTrue(url.startswith(BASE))
                requested.append(url)
                return resources[url.removeprefix(BASE)]  # Old .asm.js is a hard failure.
            output = DEPS.collect_resources(root, fetch=fetch)
            self.assertEqual(len(requested), 5)
            self.assertEqual(json.loads((output / "url-map.json").read_text()),
                             {BASE + name: name for name in resources})
            for name, content in resources.items():
                self.assertEqual((output / name).read_bytes(), content)

    def test_failed_download_leaves_no_publishable_partial_archive(self):
        with tempfile.TemporaryDirectory() as directory:
            root = self.make_root(directory)
            def fetch(url):
                if url.endswith("pyodide.mjs"):
                    return b'"pyodide.asm.mjs"'
                raise OSError("HTTP 404 test failure")
            with self.assertRaisesRegex(OSError, "404"):
                DEPS.collect_resources(root, fetch=fetch)
            self.assertEqual(list((root / "browser-smoke-artifacts").iterdir()), [])

    def test_unknown_or_ambiguous_loader_is_not_guessed(self):
        for loader in (b'"unrecognized.mjs"', b'"pyodide.asm.js" "pyodide.asm.mjs"'):
            with self.subTest(loader=loader), tempfile.TemporaryDirectory() as directory:
                root = self.make_root(directory)
                with self.assertRaisesRegex(ValueError, "unique assembly"):
                    DEPS.collect_resources(root, fetch=lambda url: loader)

    def test_unpinned_distribution_cannot_be_archived(self):
        with tempfile.TemporaryDirectory() as directory:
            root = self.make_root(directory)
            (root / "web/python-worker.js").write_text('import "https://example.test/pyodide.mjs";')
            with self.assertRaisesRegex(ValueError, "pinned Pyodide"):
                DEPS.collect_resources(root, fetch=lambda url: self.fail("Unexpected download"))

    def test_workflow_is_classified_and_delivers_preview_only_after_actual_pass(self):
        root = SCRIPT.parents[1]
        workflow = root / ".github/workflows/playground-triangle-light-review.yml"
        self.assertFalse((workflow.parent / "triangle-light-review.yml").exists())
        self.assertTrue(workflow.name.startswith("playground-"))
        text = workflow.read_text()
        self.assertIn("      - '.github/workflows/playground-triangle-light-review.yml'", text)
        steps = re.split(r"^      - name: ", text, flags=re.M)
        capture = next(i for i, step in enumerate(steps) if "id: candidate" in step)
        package = next(i for i, step in enumerate(steps) if "--output browser-smoke-artifacts/triangle-light-preview" in step)
        self.assertLess(capture, package)
        self.assertIn("success() && steps.candidate.outcome == 'success'", steps[package])
        self.assertIn("steps.deps.outcome == 'success'", text)


if __name__ == "__main__":
    unittest.main()
