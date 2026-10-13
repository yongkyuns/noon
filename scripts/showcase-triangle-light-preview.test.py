"""Bundle integrity and non-publication; these are not browser rendering tests."""
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("bundle", Path(__file__).with_name("showcase-triangle-light-preview.py"))
bundle = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bundle)


class BundleTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.web = self.root / "web"
        (self.web / "python/examples").mkdir(parents=True)
        (self.web / "pkg").mkdir()
        (self.web / "thumbnails/showcase").mkdir(parents=True)
        self.entry = {"id": "showcase-triangle-light", "path": "python/examples/showcase_triangle_light.py"}
        (self.web / "python/examples/showcase_triangle_light.candidate.json").write_text(json.dumps(self.entry))
        (self.web / self.entry["path"]).write_text("# source\n")
        (self.web / "python/examples/noon_showcase_manifest.json").write_text("original public manifest")
        (self.web / "thumbnails/showcase/capture-evidence.json").write_text("old public evidence")
        files = {}
        for name, path in bundle.PATHS.items():
            raw = f"fake integrity fixture for {name}".encode()
            (self.web / path).write_bytes(raw)
            files[name] = {"path": path, "sha256": hashlib.sha256(raw).hexdigest()}
        self.core = {"schema": 1, "sourceRevision": "a" * 40, "files": files}
        self.stamp()

    def stamp(self):
        identity = dict(self.core, buildId=hashlib.sha256(json.dumps(self.core, separators=(",", ":")).encode()).hexdigest())
        (self.web / "runtime-build-identity.json").write_text(json.dumps(identity))

    def test_bundle_keeps_original_catalog_and_tracks_separate_source_engine(self):
        output = self.root / "review"
        info = bundle.package_preview(self.root, self.web, output, "b" * 40)
        self.assertEqual(info["sourceRevision"], "b" * 40)
        self.assertEqual(info["runtimeBuildIdentity"]["sourceRevision"], "a" * 40)
        self.assertEqual((self.web / "python/examples/noon_showcase_manifest.json").read_text(), "original public manifest")
        self.assertEqual((self.web / "thumbnails/showcase/capture-evidence.json").read_text(), "old public evidence")
        candidate = json.loads((output / "web/python/examples/noon_showcase_manifest.json").read_text())
        self.assertEqual(candidate["entries"], [self.entry])
        self.assertFalse((output / "web/thumbnails/showcase/capture-evidence.json").exists())
        self.assertIn('127.0.0.1', (output / "serve.py").read_text())
        compile((output / "serve.py").read_text(), "serve.py", "exec")
        with self.assertRaises(FileExistsError):
            bundle.package_preview(self.root, self.web, output, "b" * 40)

    def test_mixed_runtime_bytes_are_rejected(self):
        (self.web / "pkg/noon_web_bg.wasm").write_bytes(b"different build")
        with self.assertRaisesRegex(ValueError, "hash mismatch"):
            bundle.package_preview(self.root, self.web, self.root / "review", "b" * 40)
        self.assertFalse((self.root / "review").exists())

    def test_runtime_paths_cannot_escape_the_bundle(self):
        self.core["files"]["wasm"]["path"] = "../private"
        self.stamp()
        with self.assertRaisesRegex(ValueError, "file path"):
            bundle.verify_runtime(self.web)

    def test_revision_is_not_a_floating_branch_name(self):
        with self.assertRaisesRegex(ValueError, "exact"):
            bundle.package_preview(self.root, self.web, self.root / "review", "master")


if __name__ == "__main__":
    unittest.main()
