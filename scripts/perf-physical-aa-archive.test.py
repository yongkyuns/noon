"""Offline fail-closed tests for source-pinned physical A/A archive intake."""
from io import BytesIO
import importlib.util
import json
from pathlib import Path
import stat
import tempfile
import unittest
from unittest import mock
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
    def test_recovery_has_its_own_registry_and_preserves_the_complete_original_plan(self):
        root = MODULE.parent.parent / "benchmarks"
        original = json.loads((root / "perf-physical-aa-study.json").read_bytes())
        registration = json.loads((root / "perf-physical-aa-transport-recovery.json").read_bytes())
        recovery = {**original, "studyId": registration["studyId"], "registration": registration}
        self.assertEqual(ARCHIVE.registered_workflow(original), "perf-physical-aa.yml")
        self.assertEqual(ARCHIVE.registered_workflow(recovery), registration["workflow"])
        for changed in (
            {**recovery, "studyId": original["studyId"]},
            {**recovery, "retries": 1},
            {**recovery, "plannedPairCommands": 99},
            {**recovery, "registration": {**registration, "priorRun": 0}},
            {**recovery, "registration": {**registration, "workflow": "perf-physical-aa.yml"}},
        ):
            with self.subTest(plan=changed), self.assertRaises(ValueError):
                ARCHIVE.registered_workflow(changed)

    def test_recovery_requires_authenticated_zero_sample_original_failure(self):
        root = MODULE.parent.parent / "benchmarks"
        original = (root / "perf-physical-aa-study.json").read_bytes()
        registration = json.loads((root / "perf-physical-aa-transport-recovery.json").read_bytes())
        for change in (None, "sample", "plan", "attempt", "digest", "run", "expired"):
            with self.subTest(change=change):
                entries = {"attempt.txt": f"{registration['priorRun']}/1".encode(),
                           "registered-plan.json": original,
                           "physical-readiness.json": b"{}", "single-manual-dispatch.json": b"{}"}
                if change == "sample":
                    entries["product/report.json"] = b"{}"
                if change == "plan":
                    entries["registered-plan.json"] = original + b"\n"
                if change == "attempt":
                    entries["attempt.txt"] = f"{registration['priorRun']}/2".encode()
                out = BytesIO()
                with zipfile.ZipFile(out, "w") as z:
                    for name, content in entries.items():
                        z.writestr("evidence/inputs/" + name, content)
                raw = out.getvalue()
                r = {**registration, "priorArtifactSha256": ARCHIVE.sha256(raw)}
                run = {"id": r["priorRun"], "run_attempt": 1, "status": "completed",
                       "conclusion": "failure", "event": "workflow_dispatch", "head_branch": "master"}
                meta = {"id": r["priorArtifact"], "expired": change == "expired",
                        "workflow_run": {"id": r["priorRun"]},
                        "digest": "sha256:" + r["priorArtifactSha256"]}
                if change == "digest":
                    r["priorArtifactSha256"] = "0" * 64
                    meta["digest"] = "sha256:" + r["priorArtifactSha256"]
                if change == "run":
                    run["conclusion"] = "success"
                with mock.patch.object(ARCHIVE, "github_json", side_effect=[run, meta]), \
                        mock.patch.object(ARCHIVE, "github_zip", return_value=raw):
                    if change is None:
                        self.assertEqual(ARCHIVE.prior_setup_failure({"registration": r}, "fixture-token"), raw)
                    else:
                        with self.assertRaises(ValueError):
                            ARCHIVE.prior_setup_failure({"registration": r}, "fixture-token")

    def test_archive_redirect_keeps_api_authentication_off_signed_storage(self):
        payload = fixture()
        signed_url = "https://storage.example/original.zip?signature=fixture"

        def download(req, timeout):
            self.assertEqual(req.full_url,
                             "https://api.github.com/repos/yongkyuns/noon/actions/artifacts/123/zip")
            self.assertEqual(req.get_header("Authorization"), "Bearer fixture-token")
            redirected = ARCHIVE.urllib.request.HTTPRedirectHandler().redirect_request(
                req, None, 302, "Found", {}, signed_url)
            self.assertEqual(redirected.full_url, signed_url)
            self.assertIsNone(redirected.get_header("Authorization"))
            self.assertEqual(timeout, 180)
            return BytesIO(payload)

        with mock.patch.object(ARCHIVE.urllib.request, "urlopen", side_effect=download):
            actual = ARCHIVE.github_zip("actions/artifacts/123/zip", "fixture-token")
        self.assertEqual(actual, payload)

    def test_only_one_manual_dispatch_is_registered_for_this_study(self):
        initial = {"total_count": 1, "workflow_runs": [
            {"id": 123, "run_attempt": 1, "event": "workflow_dispatch",
             "head_branch": "master"}]}
        record = ARCHIVE.require_unique_manual_dispatch(initial, "123")
        self.assertEqual(record["runId"], 123)
        self.assertFalse(record["qualification"])
        for wrong in (
            {**initial, "total_count": 2},
            {**initial, "workflow_runs": []},
            {**initial, "workflow_runs": [dict(initial["workflow_runs"][0], id=124)]},
            {**initial, "workflow_runs": [dict(initial["workflow_runs"][0], run_attempt=2)]},
            {**initial, "workflow_runs": [dict(initial["workflow_runs"][0], event="pull_request")]},
            {**initial, "workflow_runs": [dict(initial["workflow_runs"][0], head_branch="feature")]},
        ):
            with self.subTest(registry=wrong), self.assertRaises(ValueError):
                ARCHIVE.require_unique_manual_dispatch(wrong, "123")
        with self.assertRaises(ValueError):
            ARCHIVE.require_unique_manual_dispatch(initial, "not-a-run")

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
