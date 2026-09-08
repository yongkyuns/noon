#!/usr/bin/env python3
"""Offline Cargo integration fixtures plus fail-closed metadata unit tests."""
from __future__ import annotations

import copy
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import layer_dependencies as ratchet

ROOT = Path(__file__).resolve().parents[1]


class MetadataTests(unittest.TestCase):
    def setUp(self):
        self.root = Path("/fixture")
        self.data = {
            "version": 1, "workspace_root": str(self.root),
            "workspace_members": list(ratchet.FORBIDDEN),
            "packages": [
                {"name": name, "id": name, "manifest_path": f"/fixture/crates/{name}/Cargo.toml", "dependencies": []}
                for name in ratchet.FORBIDDEN
            ],
        }
        self.dep = {"name": "noon-runtime", "rename": "engine", "kind": None, "optional": True, "target": 'cfg(target_os = "none")'}
        self.data["packages"][0]["dependencies"] = [self.dep]

    def test_declared_identity_not_alias_or_resolve_graph(self):
        self.data["resolve"] = None
        messages = ratchet.violations(self.data, self.root)
        self.assertEqual(len(messages), 1)
        for text in ("noon-core must not depend on noon-runtime", "alias=engine", "optional", "target="):
            self.assertIn(text, messages[0])

    def test_every_edge_and_kind(self):
        for owner, forbidden in ratchet.FORBIDDEN.items():
            for name in forbidden:
                for kind in (None, "build", "dev"):
                    with self.subTest(owner=owner, dependency=name, kind=kind):
                        data = copy.deepcopy(self.data)
                        for package in data["packages"]:
                            package["dependencies"] = [dict(self.dep, name=name, kind=kind)] if package["name"] == owner else []
                        self.assertEqual(len(ratchet.violations(data, self.root)), 1)

    def test_allowed_alias_named_like_forbidden_package(self):
        self.dep.update(name="serde", rename="noon-runtime")
        self.assertEqual(ratchet.violations(self.data, self.root), [])

    def test_malformed_metadata_never_passes(self):
        invalid = [None, {}, dict(self.data, version=2), dict(self.data, packages=None),
                   dict(self.data, workspace_members=[]), dict(self.data, workspace_root="/different")]
        for key in ("name", "manifest_path", "dependencies", "id"):
            data = copy.deepcopy(self.data)
            del data["packages"][0][key]
            invalid.append(data)
        for key in self.dep:
            data = copy.deepcopy(self.data)
            del data["packages"][0]["dependencies"][0][key]
            invalid.append(data)
        for key, value in (("kind", "future"), ("optional", 1), ("target", 17), ("rename", False)):
            data = copy.deepcopy(self.data)
            data["packages"][0]["dependencies"][0][key] = value
            invalid.append(data)
        invalid.append(dict(self.data, packages=self.data["packages"] * 2))
        for data in invalid:
            with self.subTest(data=data), self.assertRaises(ValueError):
                ratchet.violations(data, self.root)

    def test_cargo_failure_and_invalid_json_are_errors(self):
        for result in (subprocess.CompletedProcess([], 101, "", "unreadable manifest"),
                       subprocess.CompletedProcess([], 0, "not JSON", "")):
            with patch.object(ratchet.subprocess, "run", return_value=result), self.assertRaises(ValueError):
                ratchet.check(self.root)
        with patch.object(ratchet.subprocess, "run", side_effect=FileNotFoundError), self.assertRaises(OSError):
            ratchet.check(self.root)

    def test_cargo_invocation_does_not_resolve_active_graph(self):
        self.dep["name"] = "serde"
        result = subprocess.CompletedProcess([], 0, json.dumps(self.data), "")
        with patch.object(ratchet.subprocess, "run", return_value=result) as run:
            self.assertEqual(ratchet.check(self.root), 0)
        self.assertEqual(run.call_args.args[0], ["cargo", "metadata", "--no-deps", "--format-version", "1", "--offline", "--manifest-path", "/fixture/Cargo.toml"])


class CargoTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        if shutil.which("cargo") is None:
            raise RuntimeError("Cargo is required: integration fixtures must not be silently skipped")

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="noon layer fixtures ")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        (self.root / "Cargo.toml").write_text('[workspace]\nmembers = ["crates/*"]\nresolver = "2"\n')
        for name in (*ratchet.FORBIDDEN, "noon-native", "noon-web", "utility"):
            path = self.root / "crates" / name
            (path / "src").mkdir(parents=True)
            (path / "src/lib.rs").write_text("")
            (path / "Cargo.toml").write_text(f'[package]\nname = "{name}"\nversion = "0.1.0"\nedition = "2021"\n')
        self.manifest = self.root / "crates/noon-core/Cargo.toml"
        self.clean = self.manifest.read_text()

    def run_ratchet(self, code, contains=""):
        result = subprocess.run(["bash", str(ROOT / "scripts/layer-dependency-ratchet.sh")],
                                cwd=self.root.parent, env=dict(os.environ, NOON_ROOT=str(self.root)),
                                text=True, capture_output=True)
        self.assertEqual(result.returncode, code, result.stdout + result.stderr)
        self.assertIn(contains, result.stdout + result.stderr)

    def test_clean_and_allowed_downward_edges(self):
        for owner, dep in (("noon-compile", "noon-core"), ("noon-runtime", "noon-compile"),
                           ("noon-render-wgpu", "noon-runtime"), ("noon", "noon-core")):
            with (self.root / "crates" / owner / "Cargo.toml").open("a") as manifest:
                manifest.write(f'\n[dependencies]\n{dep} = {{ path = "../{dep}" }}\n')
        self.run_ratchet(0, "ratchet passed")

    def test_direct_and_quoted_edges_for_each_owner(self):
        for owner, dep in (("noon-core", "noon-runtime"), ("noon-compile", "noon-render-wgpu"),
                           ("noon-runtime", "noon-web"), ("noon-render-wgpu", "noon"), ("noon", "noon-native")):
            path = self.root / "crates" / owner / "Cargo.toml"
            clean = path.read_text()
            for quote in ('', '"', "'"):
                with self.subTest(owner=owner, quote=quote):
                    path.write_text(clean + f'\n[dependencies]\n{quote}{dep}{quote} = {{ path = "../{dep}" }}\n')
                    self.run_ratchet(1, f"{owner} must not depend on {dep}")
            path.write_text(clean)

    def test_aliased_inline_table_optional_target_and_kinds(self):
        for kind in ("dependencies", "build-dependencies", "dev-dependencies"):
            for prefix in ("", "target.'cfg(target_os = \"none\")'."):
                optional = ', optional = true' if kind != "dev-dependencies" else ""
                declarations = [
                    f'[{prefix}{kind}]\nengine = {{ package = "noon-runtime", path = "../noon-runtime"{optional} }}\n',
                    f'[{prefix}{kind}.engine]\npackage = "noon-runtime"\npath = "../noon-runtime"\n' + ('optional = true\n' if optional else ''),
                ]
                for declaration in declarations:
                    with self.subTest(declaration=declaration):
                        self.manifest.write_text(self.clean + '\n' + declaration)
                        self.run_ratchet(1, "noon-core must not depend on noon-runtime")

    def test_workspace_inherited_aliases(self):
        with (self.root / "Cargo.toml").open("a") as root:
            root.write('\n[workspace.dependencies]\nengine = { package = "noon-runtime", path = "crates/noon-runtime" }\n')
        for section in ("dependencies", "build-dependencies", "dev-dependencies", 'target.\'cfg(target_arch = "wasm32")\'.dependencies'):
            with self.subTest(section=section):
                self.manifest.write_text(self.clean + f'\n[{section}]\nengine.workspace = true\n')
                self.run_ratchet(1, "noon-core must not depend on noon-runtime")

    def test_allowed_alias_does_not_match_its_key(self):
        self.manifest.write_text(self.clean + '\n[dependencies]\nnoon-runtime = { package = "utility", path = "../utility" }\n')
        self.run_ratchet(0, "ratchet passed")

    def test_malformed_missing_and_misidentified_manifests(self):
        self.manifest.write_text(self.clean + '\n[dependencies\n')
        self.run_ratchet(2, "cargo metadata failed")
        self.manifest.unlink()
        self.run_ratchet(2, "cargo metadata failed")
        self.manifest.write_text(self.clean.replace('"noon-core"', '"renamed-core"'))
        self.run_ratchet(2, "missing or misidentified")


if __name__ == "__main__":
    unittest.main(verbosity=2)
