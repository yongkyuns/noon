import json
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
WEB_ROOT = REPO_ROOT / "web"
MANIFEST = WEB_ROOT / "python" / "examples" / "manim_tutorial_manifest.json"
UPSTREAM = REPO_ROOT / "parity" / "manim-v0.21" / "upstream-examples" / "rotating_demo.py"
PUBLIC = WEB_ROOT / "python" / "examples" / "manim_example_rotating_demo.py"


class ManimRotatingDemoSourceTests(unittest.TestCase):
    def test_public_rotating_demo_is_import_only_adaptation(self) -> None:
        upstream = UPSTREAM.read_text(encoding="utf-8")
        public = PUBLIC.read_text(encoding="utf-8")
        self.assertEqual(upstream.count("from manim import *"), 1)
        self.assertEqual(
            public,
            upstream.replace("from manim import *", "from noon import *", 1),
        )
        self.assertEqual(upstream.count("Rotating("), 5)
        self.assertIn("axis=UP", upstream)
        self.assertIn("axis=RIGHT", upstream)
        self.assertIn("about_edge=UP", upstream)

    def test_manifest_keeps_rotating_demo_as_candidate_until_3d_parity(self) -> None:
        manifest = json.loads(MANIFEST.read_text(encoding="utf-8"))
        entry = next(item for item in manifest["entries"] if item["id"] == "manim-rotating-demo")
        self.assertEqual(entry["status"], "ready")
        self.assertEqual(entry["parity_status"], "candidate")
        self.assertEqual(entry["expected_duration"], 18.0)
        self.assertEqual(
            entry["upstream_source"],
            "parity/manim-v0.21/upstream-examples/rotating_demo.py",
        )


if __name__ == "__main__":
    unittest.main()
