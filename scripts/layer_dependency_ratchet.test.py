#!/usr/bin/env python3
"""Real offline Cargo fixtures plus fail-closed metadata/command regressions."""

from contextlib import redirect_stderr, redirect_stdout
from copy import deepcopy
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import layer_dependency_ratchet as ratchet

SCRIPT = Path(__file__).with_name("layer-dependency-ratchet.sh")
PACKAGES = (*ratchet.FORBIDDEN, "noon-native", "noon-web")


def metadata(root):
    return {
        "version": 1,
        "workspace_root": str(root),
        "workspace_members": list(PACKAGES),
        "packages": [
            {"name": name, "id": name, "manifest_path": str(root / "crates" / name / "Cargo.toml"),
             "dependencies": []} for name in PACKAGES
        ],
        "resolve": None,
    }


def dependency(name="noon-web", **overrides):
    return {"name": name, "kind": None, "rename": None, "optional": False,
            "target": None, **overrides}


class MetadataTests(unittest.TestCase):
    def setUp(self):
        self.root = Path("/workspace").resolve()
        self.data = metadata(self.root)

    def test_empty_and_allowed_edges(self):
        for package in self.data["packages"]:
            package["dependencies"] = [dependency("serde")]
        self.assertEqual(ratchet.inspect_metadata(self.data, self.root), [])

    def test_every_policy_edge_and_kind_uses_package_identity(self):
        for source, targets in ratchet.FORBIDDEN.items():
            for target in targets:
                for kind in (None, "normal", "build", "dev"):
                    with self.subTest(source=source, target=target, kind=kind):
                        data = deepcopy(self.data)
                        package = next(p for p in data["packages"] if p["name"] == source)
                        package["dependencies"] = [dependency(target, rename="alias", kind=kind,
                                                             optional=True, target="cfg(windows)")]
                        failures = ratchet.inspect_metadata(data, self.root)
                        self.assertEqual(len(failures), 1)
                        self.assertIn(f"{source} must not depend on {target}", failures[0])
                        self.assertIn("alias=alias", failures[0])

    def test_forbidden_looking_alias_for_allowed_package_is_not_an_edge(self):
        self.data["packages"][0]["dependencies"] = [dependency("serde", rename="noon-web")]
        self.assertEqual(ratchet.inspect_metadata(self.data, self.root), [])

    def test_malformed_or_incomplete_metadata_fails(self):
        cases = [None, [], {}, {**self.data, "version": 2}, {**self.data, "version": True},
                 {**self.data, "workspace_root": "/elsewhere"},
                 {**self.data, "workspace_members": []}, {**self.data, "packages": []},
                 {**self.data, "packages": self.data["packages"][1:]},
                 {**self.data, "packages": self.data["packages"] * 2}]
        for data in cases:
            with self.subTest(data=data):
                with self.assertRaises(ratchet.InputError):
                    ratchet.inspect_metadata(data, self.root)

    def test_malformed_dependency_never_becomes_no_dependencies(self):
        cases = [None, {}, [], {**dependency(), "name": ""},
                 {**dependency(), "kind": "future"}, {**dependency(), "optional": "false"},
                 {**dependency(), "rename": []}, {**dependency(), "target": 1}]
        cases += [{k: v for k, v in dependency().items() if k != field}
                  for field in dependency()]
        for value in cases:
            with self.subTest(dependency=value):
                data = deepcopy(self.data)
                data["packages"][0]["dependencies"] = [value]
                with self.assertRaises(ratchet.InputError):
                    ratchet.inspect_metadata(data, self.root)

    def test_missing_dependency_list_fails(self):
        del self.data["packages"][0]["dependencies"]
        with self.assertRaises(ratchet.InputError):
            ratchet.inspect_metadata(self.data, self.root)


class CommandTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        (self.root / "Cargo.toml").write_text("[workspace]\n", encoding="utf-8")

    def run_main(self):
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            code = ratchet.main([str(self.root)])
        return code, out.getvalue(), err.getvalue()

    def test_command_is_offline_no_deps_and_pinned_format(self):
        response = subprocess.CompletedProcess([], 0, json.dumps(metadata(self.root)), "")
        with patch.object(ratchet.subprocess, "run", return_value=response) as run:
            self.assertEqual(self.run_main()[0], 0)
        self.assertEqual(run.call_args.args[0], [
            "cargo", "metadata", "--no-deps", "--format-version", "1", "--offline",
            "--manifest-path", str(self.root / "Cargo.toml")])

    def test_missing_cargo_unreadable_output_timeout_and_command_failure(self):
        cases = [FileNotFoundError("cargo not installed"), PermissionError("unreadable"),
                 UnicodeError("invalid UTF-8"), subprocess.TimeoutExpired("cargo", 60),
                 subprocess.CompletedProcess([], 17, "{}", "fixture cargo error"),
                 subprocess.CompletedProcess([], 0, "not JSON", ""),
                 subprocess.CompletedProcess([], 0, "{}", "")]
        for value in cases:
            with self.subTest(value=value):
                kwargs = {"side_effect": value} if isinstance(value, Exception) else {"return_value": value}
                with patch.object(ratchet.subprocess, "run", **kwargs):
                    code, out, err = self.run_main()
                self.assertEqual(code, 2)
                self.assertNotIn("passed", out)
                self.assertIn("cannot check workspace", err)

    def test_unreadable_or_missing_root_manifest(self):
        manifest = self.root / "Cargo.toml"
        manifest.unlink()
        self.assertEqual(self.run_main()[0], 2)
        manifest.mkdir()
        self.assertEqual(self.run_main()[0], 2)


class CargoTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        if not shutil.which("cargo"):
            raise RuntimeError("real Cargo fixtures require cargo; none are skipped")

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="noon layers ")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.workspace = '[workspace]\nmembers = ["crates/*"]\nresolver = "2"\n'
        self.write_workspace()
        for name in PACKAGES:
            source = self.root / "crates" / name / "src"
            source.mkdir(parents=True)
            (source / "lib.rs").write_text("// Metadata-only fixture.\n", encoding="utf-8")
            self.write_package(name)
        self.env = {**os.environ, "NOON_ROOT": str(self.root),
                    "CARGO_HOME": str(self.root / "empty-cargo-home")}

    def write_workspace(self, suffix=""):
        (self.root / "Cargo.toml").write_text(self.workspace + suffix, encoding="utf-8")

    def write_package(self, name, dependencies=""):
        manifest = self.root / "crates" / name / "Cargo.toml"
        manifest.write_text(f'[package]\nname = "{name}"\nversion = "0.0.0"\nedition = "2021"\n'
                            + dependencies, encoding="utf-8")

    def check(self, expected, diagnostic=""):
        result = subprocess.run(["bash", str(SCRIPT)], cwd=self.root, env=self.env,
                                capture_output=True, text=True, timeout=75)
        output = result.stdout + result.stderr
        self.assertEqual(result.returncode, expected, output)
        self.assertIn(diagnostic, output)
        if expected:
            self.assertNotIn("ratchet passed", output)

    def test_clean_and_allowed_downward_graph(self):
        self.check(0)
        for source, target in (("noon-compile", "noon-core"), ("noon-runtime", "noon-compile"),
                               ("noon-render-wgpu", "noon-runtime"), ("noon", "noon-render-wgpu")):
            self.write_package(source, f'[dependencies]\nlower = {{ package = "{target}", path = "../{target}" }}\n')
        self.check(0)

    def test_original_layer_boundaries(self):
        for source, target in (("noon-core", "noon-runtime"), ("noon-compile", "noon-render-wgpu"),
                               ("noon-runtime", "noon-web"), ("noon-render-wgpu", "noon"),
                               ("noon", "noon-native")):
            with self.subTest(source=source):
                self.write_package(source, f'[dependencies]\n"{target}" = {{ path = "../{target}" }}\n')
                self.check(1, f"{source} must not depend on {target}")
                self.write_package(source)

    def test_inline_and_table_aliases_for_every_dependency_kind(self):
        for section in ("dependencies", "build-dependencies", "dev-dependencies"):
            for declaration in (
                f'[{section}]\nbrowser_host = {{ package = "noon-web", path = "../noon-web" }}\n',
                f'[{section}.browser_host]\npackage = "noon-web"\npath = "../noon-web"\n',
            ):
                with self.subTest(declaration=declaration):
                    self.write_package("noon-runtime", declaration)
                    self.check(1, "alias=browser_host")

    def test_workspace_inherited_aliases(self):
        for workspace_dep in (
            '[workspace.dependencies]\nbrowser_host = { package = "noon-web", path = "crates/noon-web" }\n',
            '[workspace.dependencies.browser_host]\npackage = "noon-web"\npath = "crates/noon-web"\n',
        ):
            self.write_workspace(workspace_dep)
            for section in ("dependencies", "build-dependencies", "dev-dependencies"):
                for declaration in (f'[{section}]\nbrowser_host = {{ workspace = true }}\n',
                                    f'[{section}.browser_host]\nworkspace = true\n'):
                    with self.subTest(workspace_dep=workspace_dep, declaration=declaration):
                        self.write_package("noon-runtime", declaration)
                        self.check(1, "alias=browser_host")

    def test_inactive_targets_and_optional_edges(self):
        for target in ('cfg(target_arch = "wasm32")', 'cfg(windows)'):
            for section in ("dependencies", "build-dependencies", "dev-dependencies"):
                options = (False, True) if section != "dev-dependencies" else (False,)
                for optional in options:
                    with self.subTest(target=target, section=section, optional=optional):
                        self.write_package("noon-runtime", f"[target.'{target}'.{section}.browser_host]\n"
                                           'package = "noon-web"\npath = "../noon-web"\n'
                                           + ('optional = true\n' if optional else ''))
                        self.check(1, f"target={target}")

    def test_unused_workspace_alias_is_not_an_edge(self):
        self.write_workspace('[workspace.dependencies]\nbrowser_host = { package = "noon-web", path = "crates/noon-web" }\n')
        self.check(0)

    def test_metadata_values_do_not_count_as_dependencies(self):
        self.write_package("noon-runtime", '[package.metadata]\nnoon-web = "not a dependency"\n')
        self.check(0)

    def test_malformed_manifest(self):
        self.write_package("noon-runtime", '[dependencies\nbroken = {\n')
        self.check(2, "cargo metadata failed")

    def test_missing_member_manifest(self):
        (self.root / "crates/noon-runtime/Cargo.toml").unlink()
        self.check(2)

    def test_missing_policy_package_fails_instead_of_partial_success(self):
        shutil.rmtree(self.root / "crates/noon-runtime")
        self.check(2, "missing policy packages: noon-runtime")


if __name__ == "__main__":
    unittest.main(verbosity=2)
