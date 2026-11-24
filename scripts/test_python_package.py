#!/usr/bin/env python3
"""Failure controls for artifact checks; no native build or GPU is required."""
import importlib.util
from pathlib import Path
import tempfile
import unittest
import zipfile

SPEC = importlib.util.spec_from_file_location("python_package_check", Path(__file__).with_name("check-python-package.py"))
CHECK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECK)


class WheelContract(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        settings = CHECK.checked_configuration()
        project = settings["project"]
        self.version = CHECK.source_version()
        self.dist = f"{project['name']}-{self.version}.dist-info"
        self.files = {
            f"{name}.py": (CHECK.ROOT / "web/python" / f"{name}.py").read_bytes()
            for name in settings["tool"]["setuptools"]["py-modules"]
        }
        self.files["_noon_native.cpython-312-x86_64-linux-gnu.so"] = b"native-placeholder-for-archive-test"
        metadata = ["Metadata-Version: 2.4", f"Name: {project['name']}", f"Version: {self.version}",
                    f"Requires-Python: {project['requires-python']}"]
        metadata += [f"Requires-Dist: {value}" for value in project["dependencies"]]
        metadata += [f"License-File: {value}" for value in project["license-files"]]
        self.files[f"{self.dist}/METADATA"] = ("\n".join(metadata) + "\n\n").encode()
        self.files[f"{self.dist}/WHEEL"] = b"Wheel-Version: 1.0\nRoot-Is-Purelib: false\nTag: cp312-cp312-linux_x86_64\n"
        self.files[f"{self.dist}/entry_points.txt"] = b"[console_scripts]\nnoon = noon_native:main\n"
        for name in project["license-files"]:
            self.files[f"{self.dist}/licenses/{name}"] = (CHECK.ROOT / name).read_bytes()

    def validate(self):
        path = Path(self.temporary.name) / "candidate.whl"
        with zipfile.ZipFile(path, "w") as archive:
            for name, content in self.files.items():
                archive.writestr(name, content)
        return CHECK.validate_wheel(path)

    def test_complete_archive_passes_structural_check(self):
        self.assertEqual(self.validate()["version"], self.version)

    def test_repair_inventory_covers_versioned_native_libraries(self):
        library = "noon.libs/libexample-aabb.so.2.0.0"
        self.files[library] = b"vendored-library-placeholder"
        report = self.validate()
        self.assertEqual(report["additional_native_libraries"]["paths"], [library])
        payload, = [item for item in report["native_payloads"] if item["path"] == library]
        self.assertEqual(payload["bytes"], len(self.files[library]))
        self.assertEqual(payload["sha256"], CHECK.hashlib.sha256(self.files[library]).hexdigest())
        self.assertIn(f"{self.dist}/licenses/licenses/typst-assets-0.15.1/NOTICE", report["license_notice_files"])

    def test_missing_extension_fails(self):
        self.files.pop("_noon_native.cpython-312-x86_64-linux-gnu.so")
        with self.assertRaisesRegex(CHECK.PackageError, "mandatory _noon_native"):
            self.validate()

    def test_missing_lazy_helper_fails(self):
        self.files.pop("_manim_user_vectors.py")
        with self.assertRaisesRegex(CHECK.PackageError, "source mismatch"):
            self.validate()

    def test_development_helper_leak_fails(self):
        self.files["_test_scene_support.py"] = b""
        with self.assertRaisesRegex(CHECK.PackageError, "source mismatch"):
            self.validate()

    def test_missing_numerical_dependency_fails(self):
        name = f"{self.dist}/METADATA"
        self.files[name] = b"\n".join(line for line in self.files[name].split(b"\n")
                                       if not line.startswith(b"Requires-Dist: scipy"))
        with self.assertRaisesRegex(CHECK.PackageError, "NumPy/SciPy"):
            self.validate()

    def test_provider_notice_mutation_fails(self):
        self.files[f"{self.dist}/licenses/licenses/typst-assets-0.15.1/NOTICE"] = b"Apache-2.0 only"
        with self.assertRaisesRegex(CHECK.PackageError, "license/notice"):
            self.validate()

    def test_pure_python_fallback_fails(self):
        self.files[f"{self.dist}/WHEEL"] = b"Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n"
        with self.assertRaisesRegex(CHECK.PackageError, "optional pure wheel"):
            self.validate()


class SourceClosure(unittest.TestCase):
    def setUp(self):
        self.files = {
            "Cargo.toml": b'[workspace]\nmembers = ["crates/engine"]\n',
            "crates/engine/Cargo.toml": b'[package]\nname = "engine"\n',
            "crates/engine/src/lib.rs": b'const FONT: &[u8] = include_bytes!("../../../fixtures/font.otf");',
            "fixtures/font.otf": b"font-placeholder",
        }

    def test_workspace_resource_closure(self):
        self.assertEqual(CHECK.validate_source_closure(self.files), 1)

    def test_missing_external_asset_fails(self):
        self.files.pop("fixtures/font.otf")
        with self.assertRaisesRegex(CHECK.PackageError, "compile-time resource"):
            CHECK.validate_source_closure(self.files)

    def test_missing_workspace_member_fails(self):
        self.files.pop("crates/engine/Cargo.toml")
        with self.assertRaisesRegex(CHECK.PackageError, "workspace member"):
            CHECK.validate_source_closure(self.files)

    def test_archive_path_escape_fails(self):
        with self.assertRaisesRegex(CHECK.PackageError, "unsafe archive path"):
            CHECK.check_file_names(["noon-0.1.0/../../escape"])


if __name__ == "__main__":
    unittest.main()
