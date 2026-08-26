import ast
import json
import unittest
from pathlib import Path

WEB_ROOT = Path(__file__).parents[1]
REPO_ROOT = WEB_ROOT.parent
MANIFEST_PATH = WEB_ROOT / "python" / "examples" / "manim_tutorial_manifest.json"
PARITY_MANIFEST_PATH = REPO_ROOT / "parity" / "manim-v0.21" / "manifest.json"


def load_manifest():
    return json.loads(MANIFEST_PATH.read_text(encoding="utf-8"))


def load_parity_manifest():
    return json.loads(PARITY_MANIFEST_PATH.read_text(encoding="utf-8"))


def noon_source_from_upstream(source: str) -> str:
    upstream_import = "from manim import *"
    noon_import = "from noon import *"
    if source.count(upstream_import) != 1:
        raise AssertionError("canonical upstream example must contain exactly one Manim star import")
    return source.replace(upstream_import, noon_import, 1)


class ManimTutorialExampleTests(unittest.TestCase):
    def test_manifest_is_versioned_and_has_unique_ids(self) -> None:
        manifest = load_manifest()
        self.assertEqual(manifest["reference"]["version"], "0.21.0")
        entries = manifest["entries"]
        ids = [entry["id"] for entry in entries]
        self.assertEqual(len(ids), len(set(ids)))
        self.assertGreaterEqual(len(entries), 10)

    def test_every_ready_tutorial_exists_compiles_and_matches_upstream_source(self) -> None:
        ready = [entry for entry in load_manifest()["entries"] if entry["status"] == "ready"]
        self.assertGreaterEqual(len(ready), 1)
        paths = []
        for entry in ready:
            with self.subTest(entry=entry["id"]):
                self.assertEqual(entry["reuse"], "source-equivalent-manim-v0.21")
                self.assertIn(entry["parity_status"], {"candidate", "parity-qualified"})
                self.assertTrue(entry["upstream_source"])

                relative_path = entry["path"]
                paths.append(relative_path)
                path = WEB_ROOT / relative_path
                upstream_path = REPO_ROOT / entry["upstream_source"]
                self.assertTrue(path.is_file())
                self.assertTrue(upstream_path.is_file())
                self.assertTrue((WEB_ROOT / entry["thumbnail"]).is_file())

                source = path.read_text(encoding="utf-8")
                upstream_source = upstream_path.read_text(encoding="utf-8")
                self.assertEqual(source, noon_source_from_upstream(upstream_source))
                compile(ast.parse(source, filename=str(path)), str(path), "exec")
        self.assertEqual(len(paths), len(set(paths)))

    def test_unready_tutorials_explain_their_dependency(self) -> None:
        entries = load_manifest()["entries"]
        for entry in entries:
            if entry["status"] in {"blocked", "deferred"}:
                with self.subTest(entry=entry["id"]):
                    self.assertIn("dependency", entry)
                    self.assertTrue(entry["dependency"].startswith("#"))

    def test_reuse_provenance_is_explicit(self) -> None:
        for entry in load_manifest()["entries"]:
            if entry["status"] == "ready":
                with self.subTest(entry=entry["id"]):
                    self.assertIn("upstream", entry)
                    self.assertIn("upstream_source", entry)
                    self.assertIn("reuse", entry)

    def test_only_qualified_examples_require_canonical_raster_fixtures(self) -> None:
        entries = load_manifest()["entries"]
        canonical_fixtures = {
            fixture["id"] for fixture in load_parity_manifest()["fixtures"]
        }
        linked_fixtures = []
        for entry in entries:
            if entry.get("status") != "ready":
                continue
            with self.subTest(entry=entry["id"]):
                fixture = entry.get("parity_fixture")
                if fixture is not None:
                    linked_fixtures.append(fixture)
                    self.assertIn(fixture, canonical_fixtures)
                if entry["parity_status"] == "parity-qualified":
                    self.assertIsNotNone(fixture)
                else:
                    self.assertTrue(
                        fixture is not None or isinstance(entry.get("expected_duration"), (int, float)),
                        "candidate without a raster fixture needs a runtime duration contract",
                    )

        self.assertEqual(len(linked_fixtures), len(set(linked_fixtures)))

    def test_synthetic_parity_probes_are_not_public_examples(self) -> None:
        ready_ids = {
            entry["id"]
            for entry in load_manifest()["entries"]
            if entry["status"] == "ready"
        }
        for probe_id in {
            "parity-dot-ellipse",
            "parity-add-wait-lagged-start-map",
            "parity-grow-point-center-edge",
            "parity-uncreate-styled-square",
            "parity-focus-on-point",
            "parity-rotating-centered",
            "parity-show-increasing-subsets-two-shapes",
            "parity-show-submobjects-one-by-one-two-shapes",
            "parity-indicate-square",
        }:
            self.assertNotIn(probe_id, ready_ids)

    def test_no_ready_example_is_a_noon_adaptation(self) -> None:
        ready = [entry for entry in load_manifest()["entries"] if entry["status"] == "ready"]
        self.assertTrue(ready)
        self.assertTrue(
            all(entry.get("reuse") == "source-equivalent-manim-v0.21" for entry in ready)
        )


if __name__ == "__main__":
    unittest.main()
