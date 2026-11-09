"""Artifact-evidence tests use local Cargo archives and a fake pinned GitHub."""

import hashlib
import importlib.util
import io
import json
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import unittest
from unittest.mock import patch
import zipfile


SCRIPT = Path(__file__).with_name("collect-release-licenses.py")
SPEC = importlib.util.spec_from_file_location("collect_release_licenses", SCRIPT)
collector = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(collector)
SOURCE = "registry+https://github.com/rust-lang/crates.io-index"
REVISION = "a" * 40


class Fixture:
    def __init__(self, root):
        self.root, self.packages, self.nodes, self.locked = root, [], [], []
        self.members = []

    def package(self, name, files=None, *, workspace=False, repository="https://github.com/example/project", vcs=None):
        source = None if workspace else SOURCE
        directory = self.root / (f"crates/{name}" if workspace else f"cargo/registry/src/test-registry/{name}-1.0.0")
        directory.mkdir(parents=True)
        manifest = (f'[package]\nname = "{name}"\nversion = "1.0.0"\nlicense = "MIT"\nrepository = "{repository}"\n').encode()
        (directory / "Cargo.toml").write_bytes(manifest)
        pid = f"{source or 'path'}#{name}@1.0.0"
        package = {"id": pid, "name": name, "version": "1.0.0", "source": source,
                   "manifest_path": str(directory / "Cargo.toml"), "repository": repository,
                   "license": "MIT", "license_file": None}
        self.packages.append(package)
        self.nodes.append({"id": pid, "features": ["default"], "deps": []})
        lock = f'[[package]]\nname = "{name}"\nversion = "1.0.0"\n'
        if workspace:
            self.members.append(pid)
        else:
            archive = self.root / f"cargo/registry/cache/test-registry/{name}-1.0.0.crate"
            archive.parent.mkdir(parents=True, exist_ok=True)
            contents = {"Cargo.toml": manifest, **(files or {})}
            if vcs is not None:
                contents[".cargo_vcs_info.json"] = json.dumps(vcs).encode()
            with tarfile.open(archive, "w:gz") as packed:
                for path, data in contents.items():
                    info = tarfile.TarInfo(f"{name}-1.0.0/{path}")
                    info.size = len(data)
                    packed.addfile(info, io.BytesIO(data))
            lock += f'source = "{SOURCE}"\nchecksum = "{collector.file_digest(archive)}"\n'
        self.locked.append(lock)
        return pid

    def edge(self, source, destination, kind=None, target=None):
        node = next(n for n in self.nodes if n["id"] == source)
        node["deps"].append({"name": destination.split("#")[1].split("@")[0], "pkg": destination,
                             "dep_kinds": [{"kind": kind, "target": target}]})

    def metadata(self):
        (self.root / "Cargo.lock").write_text("version = 4\n\n" + "\n".join(self.locked))
        return {"workspace_root": str(self.root), "workspace_members": self.members,
                "packages": self.packages, "resolve": {"nodes": self.nodes}}


class Response(io.BytesIO):
    def __init__(self, data, url):
        super().__init__(data)
        self.url = url


class LicenseEvidenceTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.fixture = Fixture(self.root)
        self.python = self.fixture.package("noon-python", workspace=True)
        self.web = self.fixture.package("noon-web", workspace=True)

    def collect(self, name="evidence", **kwargs):
        return collector.collect(self.root, self.root / name, self.fixture.metadata(), ["noon-python", "noon-web"], **kwargs)

    def test_graph_includes_conditional_and_build_edges_but_excludes_dev_only(self):
        shared = self.fixture.package("shared", {"LICENSE-MIT": b"upstream license\r\n", "legal/NOTICE.txt": b"notice\n", "src/license.rs": b"not a notice"})
        build = self.fixture.package("generator", {"COPYING": b"generator terms"})
        conditional = self.fixture.package("platform", {"licenses/terms.txt": b"platform terms"})
        dev = self.fixture.package("test-only", {"LICENSE": b"test terms"})
        self.fixture.edge(self.python, shared)
        self.fixture.edge(self.web, shared)
        self.fixture.edge(shared, build, "build")
        self.fixture.edge(shared, conditional, target='cfg(target_os = "windows")')
        self.fixture.edge(shared, dev, "dev")
        result = self.collect()
        self.assertEqual({p["name"] for p in result["packages"]}, {"noon-python", "noon-web", "shared", "generator", "platform"})
        item = next(p for p in result["packages"] if p["name"] == "shared")
        self.assertEqual(item["roots"], ["noon-python", "noon-web"])
        self.assertEqual(len(item["noticeFiles"]), 2)
        copied = next(p for p in item["noticeFiles"] if p.endswith("LICENSE-MIT"))
        self.assertEqual((self.root / "evidence" / copied).read_bytes(), b"upstream license\r\n")
        self.assertEqual(result["summary"]["packagesWithEvidenceGaps"], 0)
        for line in (self.root / "evidence/SHA256SUMS").read_text().splitlines():
            expected, relative = line.split("  ", 1)
            self.assertEqual(collector.file_digest(self.root / "evidence" / relative), expected)
        self.assertEqual(result, self.collect("second"))

    def test_missing_text_is_explicit_and_strict_cli_fails_after_writing_report(self):
        dependency = self.fixture.package("missing")
        self.fixture.edge(self.python, dependency)
        metadata = self.root / "metadata.json"
        metadata.write_text(json.dumps(self.fixture.metadata()))
        command = [sys.executable, str(SCRIPT), "--root", str(self.root), "--metadata", str(metadata)]
        strict = subprocess.run([*command, "--output", str(self.root / "strict")], capture_output=True, text=True)
        self.assertEqual(strict.returncode, 1, strict.stderr)
        self.assertTrue((self.root / "strict/inventory.json").is_file())
        report = subprocess.run([*command, "--output", str(self.root / "report"), "--report-only"], capture_output=True, text=True)
        self.assertEqual(report.returncode, 0, report.stderr)
        self.assertIn("no license or notice text collected", (self.root / "report/REVIEW.md").read_text())

    def test_registry_checksum_and_metadata_mismatch_never_bless_loose_source(self):
        dependency = self.fixture.package("changed", {"LICENSE": b"original"})
        self.fixture.edge(self.python, dependency)
        package = next(p for p in self.fixture.packages if p["id"] == dependency)
        package["license"] = "Apache-2.0"
        result = self.collect()
        item = next(p for p in result["packages"] if p["name"] == "changed")
        self.assertTrue(any("metadata differs" in issue for issue in item["issues"]))
        package["license"] = "MIT"
        package["license_file"] = "LICENSE"
        result = self.collect("changed-license-file")
        item = next(p for p in result["packages"] if p["name"] == "changed")
        self.assertTrue(any("manifest: license-file" in issue for issue in item["issues"]))
        package["license_file"] = None
        archive = self.root / "cargo/registry/cache/test-registry/changed-1.0.0.crate"
        archive.write_bytes(archive.read_bytes() + b"changed archive")
        result = self.collect("tampered")
        item = next(p for p in result["packages"] if p["name"] == "changed")
        self.assertTrue(any("checksum" in issue for issue in item["issues"]))
        self.assertEqual(item["noticeFiles"], [])

    def test_empty_registry_and_pinned_upstream_files_do_not_satisfy_notice_evidence(self):
        dependency = self.fixture.package("empty", {"LICENSE": b" \r\n"}, vcs={"git": {"sha1": REVISION}})
        self.fixture.edge(self.python, dependency)
        tree = {"truncated": False, "tree": [{"type": "blob", "mode": "100644", "path": "LICENSE",
                "sha": hashlib.sha1(b"blob 0\0").hexdigest()}]}
        fetcher = collector.PinnedGitHub(self.root / "cache")
        with patch.object(fetcher, "request", side_effect=[json.dumps(tree).encode(), b""]):
            result = self.collect(fetcher=fetcher)
        item = next(p for p in result["packages"] if p["name"] == "empty")
        self.assertTrue(any("empty license or notice text" in issue for issue in item["issues"]))
        self.assertTrue(any("empty upstream license or notice text" in issue for issue in item["issues"]))
        self.assertEqual(item["noticeFiles"], [])
        self.assertEqual(result["summary"]["packagesWithEvidenceGaps"], 1)

    def test_pinned_upstream_ancestor_notices_are_hash_checked_and_cached(self):
        vcs = {"git": {"sha1": REVISION}, "path_in_vcs": "crates/member"}
        for name in ("member-a", "member-b"):
            dependency = self.fixture.package(name, vcs=vcs)
            self.fixture.edge(self.python, dependency)
        bodies = {"LICENSE-MIT": b"upstream root terms", "crates/NOTICE": b"ancestor notice", "unrelated/LICENSE": b"unrelated"}
        tree = {"truncated": False, "tree": [{"type": "blob", "mode": "100644", "path": name,
                "sha": hashlib.sha1(b"blob " + str(len(data)).encode() + b"\0" + data).hexdigest()} for name, data in bodies.items()]}
        calls = []

        def request(request, timeout):
            url = request.full_url
            calls.append(url)
            self.assertIn(REVISION, url)
            body = json.dumps(tree).encode() if "api.github.com" in url else bodies[url.split(f"/{REVISION}/", 1)[1]]
            return Response(body, url)

        with patch.object(collector, "urlopen", side_effect=request):
            result = self.collect(fetcher=collector.PinnedGitHub(self.root / "cache"))
        self.assertEqual(len(calls), 3)
        self.assertEqual(result["summary"]["packagesWithEvidenceGaps"], 0)
        self.assertEqual(result["summary"]["pinnedAncestorFilesRequiringReview"], 4)
        self.assertFalse(any("unrelated" in name for name in calls))
        self.assertTrue(all(p.get("vcs") == vcs for p in result["packages"] if not p["workspaceMember"]))

    def test_unpinned_or_dirty_sources_cannot_trigger_upstream_lookup(self):
        fetcher = collector.PinnedGitHub(self.root / "cache")
        package = {"repository": "https://github.com/example/project"}
        for vcs in ({"git": {"sha1": "main"}}, {"git": {"sha1": REVISION, "dirty": True}}):
            with self.assertRaisesRegex(ValueError, "pinned 40-hex"):
                fetcher.collect(package, vcs, self.root / "out", "crate")
        self.assertFalse((self.root / "cache").exists())

    def test_wrong_upstream_blob_is_reported_without_copying_it(self):
        dependency = self.fixture.package("missing", vcs={"git": {"sha1": REVISION}})
        self.fixture.edge(self.python, dependency)
        tree = {"truncated": False, "tree": [{"type": "blob", "mode": "100644", "path": "LICENSE", "sha": "0" * 40}]}
        fetcher = collector.PinnedGitHub(self.root / "cache")
        with patch.object(fetcher, "request", side_effect=[json.dumps(tree).encode(), b"changed"]):
            result = self.collect(fetcher=fetcher)
        item = next(p for p in result["packages"] if p["name"] == "missing")
        self.assertTrue(any("blob hash mismatch" in issue for issue in item["issues"]))
        self.assertEqual(item["noticeFiles"], [])

    def test_repaired_wheel_native_payloads_and_notices_are_exact(self):
        wheel = self.root / "noon-1.0.0-cp314-linux_x86_64.whl"
        with zipfile.ZipFile(wheel, "w") as packed:
            packed.writestr("_noon_native.abi3.so", b"extension bytes")
            packed.writestr("noon.libs/libexample-123.so.1", b"native library bytes")
            packed.writestr("noon-1.0.0.dist-info/licenses/LICENSE", b"exact wheel terms\r\n")
        result = self.collect(wheels=[wheel])
        self.assertEqual(result["summary"]["nativePayloads"], 2)
        self.assertEqual(result["summary"]["additionalNativeLibrariesRequiringReview"], 1)
        inventory = result["wheels"][0]
        self.assertEqual(inventory["sha256"], collector.file_digest(wheel))
        native = next(p for p in inventory["nativePayloads"] if not p["isNoonExtension"])
        self.assertEqual(native["sha256"], collector.digest(b"native library bytes"))
        self.assertEqual((self.root / "evidence" / inventory["noticeFiles"][0]["path"]).read_bytes(), b"exact wheel terms\r\n")

    def test_unsafe_archive_paths_and_existing_outputs_are_rejected(self):
        for path in ("../LICENSE", "/LICENSE", "C:/LICENSE", "a\\LICENSE", "LICENSE\n"):
            with self.assertRaises(ValueError):
                collector.safe_path(path)
        self.collect()
        with self.assertRaisesRegex(ValueError, "already exists"):
            self.collect()


if __name__ == "__main__":
    unittest.main()
