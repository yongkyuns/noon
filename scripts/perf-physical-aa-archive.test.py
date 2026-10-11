"""Offline fail-closed tests for source-pinned physical A/A archive intake."""
from io import BytesIO
import importlib.util
import json
from pathlib import Path
import stat
import tempfile
import unittest
import warnings
import zipfile

MODULE = Path(__file__).with_name("perf-physical-aa-archive.py")
SPEC = importlib.util.spec_from_file_location("aa_physical_archive", MODULE)
ARCHIVE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(ARCHIVE)
SOURCE = "0473399d1cba9fc1f217c8b88b93c73c69991597"


def fixture(source=SOURCE, extra=None, omit=None):
    data = {
        "pkg/noon_web_bg.wasm": b"\\0asm",
        "pkg/noon_web.js": b"export const wasm = true;",
        "python-worker.js": b"import './pkg/noon_web.js';",
        "runtime-build-identity.json": json.dumps({"buildId": "x" * 64}).encode(),
        "ci-artifact.json": json.dumps({"source": source}).encode(),
        "ci-Cargo.lock": b"# frozen lock\\n",
    }
    if omit:
        data.pop(omit)
    if extra:
        data.update(extra)
    output = BytesIO()
    with zipfile.ZipFile(output, "w") as z:
        for name, content in data.items():
            z.writestr(name, content)
    return output.getvalue()


class ArchiveIntegrity(unittest.TestCase):
    def test_valid_original_zip_and_source(self):
        z, identity = ARCHIVE.verified_archive(fixture(), SOURCE)
        self.assertIn("pkg/noon_web_bg.wasm", z.namelist())
        self.assertEqual(identity["buildId"], "x" * 64)

    def test_source_substitution_fails(self):
        with self.assertRaisesRegex(ValueError, "source"):
            ARCHIVE.verified_archive(fixture(source="0" * 40), SOURCE)

    def test_missing_critical_executable_fails(self):
        with self.assertRaisesRegex(ValueError, "missing"):
            ARCHIVE.verified_archive(fixture(omit="pkg/noon_web_bg.wasm"), SOURCE)

    def test_parent_traversal_and_backslash_fail(self):
        for name in ("../escape", "pkg\\\\escape", "/rooted"):
            with self.subTest(path=name), self.assertRaisesRegex(ValueError, "unsafe"):
                ARCHIVE.verified_archive(fixture(extra={name: b"unsafe"}), SOURCE)

    def test_symlink_fails(self):
        with zipfile.ZipFile(BytesIO(fixture()), "r") as original:
            entries = [(i.filename, original.read(i.filename)) for i in original.infolist()]
        out = BytesIO()
        with zipfile.ZipFile(out, "w") as z:
            for name, content in entries:
                z.writestr(name, content)
            info = zipfile.ZipInfo("unsafe-link")
            info.create_system = 3
            info.external_attr = (stat.S_IFLNK | 0o777) << 16
            z.writestr(info, "target")
        with self.assertRaisesRegex(ValueError, "symlink"):
            ARCHIVE.verified_archive(out.getvalue(), SOURCE)

    def test_duplicate_zip_entry_fails(self):
        out = BytesIO()
        with warnings.catch_warnings():
            warnings.simplefilter("ignore", UserWarning)
            with zipfile.ZipFile(out, "w") as z:
                for name in ARCHIVE.FILES:
                    contents = json.dumps({"source": SOURCE}).encode() if name == "ci-artifact.json" else (
                        json.dumps({"buildId": "x" * 64}).encode() if name == "runtime-build-identity.json" else b"ok")
                    z.writestr(name, contents)
                z.writestr("ci-artifact.json", json.dumps({"source": SOURCE}))
        with self.assertRaisesRegex(ValueError, "duplicate"):
            ARCHIVE.verified_archive(out.getvalue(), SOURCE)

    def test_evidence_is_written_exclusively(self):
        with tempfile.TemporaryDirectory() as td:
            path = Path(td) / "inputs" / "original.zip"
            ARCHIVE.exclusive_write(path, b"immutable")
            self.assertEqual(path.read_bytes(), b"immutable")
            with self.assertRaises(FileExistsError):
                ARCHIVE.exclusive_write(path, b"new")
            self.assertEqual(path.read_bytes(), b"immutable")

    def test_extract_creates_only_web_path_and_refuses_overwrite(self):
        with tempfile.TemporaryDirectory() as td:
            src = Path(td) / "baseline"
            src.mkdir()
            z, _ = ARCHIVE.verified_archive(fixture(), SOURCE)
            ARCHIVE.extract(z, src)
            self.assertTrue((src / "web" / "pkg" / "noon_web_bg.wasm").exists())
            with self.assertRaises(FileExistsError):
                ARCHIVE.extract(z, src)


if __name__ == "__main__":
    unittest.main()
