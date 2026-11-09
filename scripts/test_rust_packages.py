#!/usr/bin/env python3
"""Negative release-artifact controls; no Cargo compilation or network needed."""
import copy
import importlib.util
import io
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest import mock

SPEC = importlib.util.spec_from_file_location("rust_package_check", Path(__file__).with_name("check-rust-packages.py"))
CHECK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECK)


def package(name="engine", publish=True):
    return {"name": name, "version": "0.1.0", "publish": publish}


def manifest(name="engine"):
    return {"package": {"name": name, "version": "0.1.0", "description": "Animation engine",
                        "license": "MIT", "repository": "https://example.org/engine",
                        "edition": "2021", "rust-version": "1.98.1", "readme": "README.md"}}


class ManifestContract(unittest.TestCase):
    def setUp(self):
        self.files = {"README.md": b"Read me", "LICENSE": b"MIT license"}
        self.workspace = {"engine": package(), "core": package("core")}
        self.manifest = manifest()

    def validate(self):
        return CHECK.validate_manifest(self.manifest, self.files, self.workspace["engine"], self.workspace)

    def test_complete_metadata_and_internal_registry_edge(self):
        self.manifest["dependencies"] = {"core": {"version": "0.1.0", "optional": True}}
        self.assertEqual(self.validate(), ["core"])

    def test_optional_target_build_path_cannot_escape_validation(self):
        self.manifest["target"] = {'cfg(target_os = "linux")': {
            "build-dependencies": {"core": {"version": "0.1.0", "path": "../core", "optional": True}}}}
        with self.assertRaisesRegex(CHECK.PackageError, "nonregistry dependency"):
            self.validate()

    def test_versionless_production_dependency_fails(self):
        self.manifest["dependencies"] = {"core": {"optional": True}}
        with self.assertRaisesRegex(CHECK.PackageError, "no registry version"):
            self.validate()

    def test_unpublished_dependency_fails(self):
        self.workspace["core"]["publish"] = False
        self.manifest["dependencies"] = {"core": {"version": "0.1.0"}}
        with self.assertRaisesRegex(CHECK.PackageError, "unpublished workspace"):
            self.validate()

    def test_mismatched_internal_version_fails(self):
        self.manifest["dependencies"] = {"core": {"version": "0.2.0"}}
        with self.assertRaisesRegex(CHECK.PackageError, "internal dependency version differs"):
            self.validate()

    def test_unresolved_workspace_metadata_fails(self):
        self.manifest["package"]["license"] = {"workspace": True}
        with self.assertRaisesRegex(CHECK.PackageError, "missing license"):
            self.validate()

    def test_missing_packaged_license_fails(self):
        self.files.pop("LICENSE")
        with self.assertRaisesRegex(CHECK.PackageError, "LICENSE is missing"):
            self.validate()

    def test_missing_packaged_readme_fails(self):
        self.files.pop("README.md")
        with self.assertRaisesRegex(CHECK.PackageError, "README is missing"):
            self.validate()


class ArchiveContract(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)

    def archive(self, name="engine", extra=None):
        path = self.root / f"{name}-0.1.0.crate"
        body = "[package]\n" + "\n".join(f'{key} = "{value}"' for key, value in manifest(name)["package"].items())
        files = {"Cargo.toml": body.encode(), "README.md": b"Read me", "LICENSE": b"MIT", "src/lib.rs": b""}
        with tarfile.open(path, "w:gz") as output:
            for relative, data in files.items():
                info = tarfile.TarInfo(f"{name}-0.1.0/{relative}")
                info.size = len(data)
                output.addfile(info, io.BytesIO(data))
            if extra:
                output.addfile(extra, io.BytesIO(b""))
        return path

    def test_missing_archive_breaks_published_dependency_closure(self):
        workspace = {"engine": package(), "core": package("core")}
        with self.assertRaisesRegex(CHECK.PackageError, "closure is incomplete"):
            CHECK.validate_archives([self.archive()], workspace)

    def test_complete_archive_set_has_hashes(self):
        loaded, records = CHECK.validate_archives([self.archive()], {"engine": package()})
        self.assertEqual(set(loaded), {"engine"})
        self.assertEqual(len(records[0]["sha256"]), 64)

    def test_archive_path_escape_fails(self):
        extra = tarfile.TarInfo("engine-0.1.0/../../escape")
        with self.assertRaisesRegex(CHECK.PackageError, "unsafe archive"):
            CHECK.read_archive(self.archive(extra=extra))

    def test_archive_symlink_fails(self):
        extra = tarfile.TarInfo("engine-0.1.0/outside")
        extra.type = tarfile.SYMTYPE
        extra.linkname = "/etc/passwd"
        with self.assertRaisesRegex(CHECK.PackageError, "links/special"):
            CHECK.read_archive(self.archive(extra=extra))


class ExternalConsumerContract(unittest.TestCase):
    def setUp(self):
        self.lock = {"package": [{"name": "serde", "version": "1.0.0",
                                  "source": "registry+https://github.com/rust-lang/crates.io-index",
                                  "checksum": "a" * 64}]}

    def test_reviewed_registry_graph_passes(self):
        self.assertEqual(CHECK.validate_registry_lock(self.lock, copy.deepcopy(self.lock)), 1)

    def test_dependency_upgrade_is_not_package_qualification(self):
        resolved = copy.deepcopy(self.lock)
        resolved["package"][0]["version"] = "1.0.1"
        with self.assertRaisesRegex(CHECK.PackageError, "unreviewed dependency"):
            CHECK.validate_registry_lock(self.lock, resolved)

    def test_same_version_with_different_checksum_fails(self):
        resolved = copy.deepcopy(self.lock)
        resolved["package"][0]["checksum"] = "b" * 64
        with self.assertRaisesRegex(CHECK.PackageError, "checksum changed"):
            CHECK.validate_registry_lock(self.lock, resolved)

    def test_workspace_path_cannot_replace_an_extracted_archive(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            consumer, extracted = root / "consumer", root / "archives"
            metadata = {"packages": [{"name": "engine", "source": None,
                                      "manifest_path": str(root / "workspace/engine/Cargo.toml")}]}
            with self.assertRaisesRegex(CHECK.PackageError, "workspace/nonarchive dependency"):
                CHECK.validate_external_graph(metadata, consumer, extracted, {"engine": package()})

    def test_expected_extracted_archive_path_passes(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            consumer, extracted = root / "consumer", root / "archives"
            metadata = {"packages": [{"name": "engine", "source": None,
                                      "manifest_path": str(extracted / "engine-0.1.0/Cargo.toml")}]}
            CHECK.validate_external_graph(metadata, consumer, extracted, {"engine": package()})


class ConsumerDependencyCache(unittest.TestCase):
    """Model the clean runner's missing tarball without requiring a network."""
    def setUp(self):
        self.reviewed = Path("/reviewed-workspace")
        self.consumer = Path("/outside-consumer")
        self.cached = False
        self.downloads = 0
        self.metadata_reads = 0

    def cargo(self, arguments, *, cwd, environment, capture=False):
        if arguments[:2] == ["cargo", "fetch"]:
            self.assertEqual(cwd, self.reviewed)
            self.assertIn("--locked", arguments)
            if not self.cached:
                if "--offline" in arguments:
                    raise CHECK.PackageError("dependency source is not cached; offline request forbids downloading")
                self.downloads += 1
                self.cached = True
            return None
        if arguments[:2] == ["cargo", "metadata"]:
            self.assertEqual(cwd, self.consumer)
            self.assertIn("--offline", arguments)
            self.metadata_reads += 1
            if not self.cached:
                raise CHECK.PackageError("failed to download ab_glyph: --offline was specified")
            return '{"packages": []}'
        self.fail(f"unexpected Cargo operation: {arguments}")

    def resolve(self, offline):
        with mock.patch.object(CHECK, "command", side_effect=self.cargo):
            return CHECK.resolve_consumer_metadata(self.consumer, root=self.reviewed,
                                                   environment={}, offline=offline)

    def test_clean_cache_is_prepared_before_offline_consumer_resolution(self):
        self.assertEqual(self.resolve(offline=False), {"packages": []})
        self.assertEqual((self.downloads, self.metadata_reads), (1, 1))

    def test_explicit_offline_request_never_downloads_missing_sources(self):
        with self.assertRaisesRegex(CHECK.PackageError, "offline request forbids downloading"):
            self.resolve(offline=True)
        self.assertEqual((self.downloads, self.metadata_reads), (0, 0))

    def test_cached_sources_support_an_explicit_offline_request(self):
        self.cached = True
        self.assertEqual(self.resolve(offline=True), {"packages": []})
        self.assertEqual((self.downloads, self.metadata_reads), (0, 1))


if __name__ == "__main__":
    unittest.main()
