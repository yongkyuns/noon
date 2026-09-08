#!/usr/bin/env python3
"""Package-policy tests plus real, dependency-free, offline Cargo workspaces."""

import contextlib
import copy
import io
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest import mock

import layer_dependency_ratchet as ratchet


RATCHET = Path(__file__).resolve().with_name("layer-dependency-ratchet.sh")
PACKAGES = (*ratchet.FORBIDDEN, "noon-native", "noon-web", "helper")


def write_workspace(root: Path) -> None:
    (root / "Cargo.toml").write_text(
        '[workspace]\nmembers = ["crates/*"]\nresolver = "2"\n', encoding="utf-8"
    )
    for name in PACKAGES:
        path = root / "crates" / name
        (path / "src").mkdir(parents=True, exist_ok=True)
        (path / "src/lib.rs").write_text("// Manifest-only fixture.\n", encoding="utf-8")
        (path / "Cargo.toml").write_text(
            f'[package]\nname = "{name}"\nversion = "0.0.0"\nedition = "2021"\n',
            encoding="utf-8",
        )


def append_manifest(root: Path, name: str, text: str) -> None:
    with (root / "crates" / name / "Cargo.toml").open("a", encoding="utf-8") as manifest:
        manifest.write("\n" + text + "\n")


def metadata_fixture(root: Path) -> dict:
    # Hand-authored metadata for validator tests, not Cargo-resolution evidence.
    packages = [dict(id=f"fixture:{name}", name=name,
                     manifest_path=str(root / "crates" / name / "Cargo.toml"),
                     dependencies=[]) for name in PACKAGES]
    return dict(version=1, workspace_root=str(root), packages=packages,
                workspace_members=[package["id"] for package in packages], resolve=None)


def dependency(name: str, **fields) -> dict:
    return dict(name=name, **{**dict(kind=None, rename=None, optional=False, target=None), **fields})


class WorkspaceTest(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="noon-layer-fixture-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name).resolve()
        write_workspace(self.root)
        self.metadata = metadata_fixture(self.root)

    def add_dependency(self, source: str, edge: dict):
        next(package for package in self.metadata["packages"]
             if package["name"] == source)["dependencies"].append(edge)


class PolicyTests(WorkspaceTest):
    def test_original_direct_edge_policy_is_preserved(self):
        engine = ["noon-core", "noon-compile", "noon-runtime", "noon-render-wgpu", "noon"]
        expected = {name: set(engine[index + 1:] + ["noon-native", "noon-web"])
                    for index, name in enumerate(engine)}
        self.assertEqual(ratchet.FORBIDDEN, expected)

    def test_clean_metadata_and_unrelated_new_fields(self):
        self.metadata["future_field"] = {"ignored": True}
        self.assertEqual(ratchet.violations(self.metadata, self.root), [])

    def test_every_forbidden_direction_for_all_dependency_kinds(self):
        for source, targets in ratchet.FORBIDDEN.items():
            for target in sorted(targets):
                for kind in (None, "build", "dev"):
                    with self.subTest(source=source, target=target, kind=kind):
                        self.metadata = metadata_fixture(self.root)
                        self.add_dependency(source, dependency(target, kind=kind))
                        failures = ratchet.violations(self.metadata, self.root)
                        self.assertEqual(len(failures), 1)
                        self.assertIn(f"{source} must not depend on {target}", failures[0])

    def test_alias_optional_and_target_conditions_do_not_exempt_an_edge(self):
        self.add_dependency("noon-runtime", dependency(
            "noon-web", rename="browser_host", optional=True,
            target='cfg(target_arch = "wasm32")', kind="build"))
        failure, = ratchet.violations(self.metadata, self.root)
        for value in ("noon-web", "alias=browser_host", "kind=build", "wasm32", "optional"):
            self.assertIn(value, failure)

    def test_dependency_key_is_not_package_identity(self):
        self.add_dependency("noon-runtime", dependency("helper", rename="noon-web"))
        self.assertEqual(ratchet.violations(self.metadata, self.root), [])

    def test_all_violations_are_reported(self):
        self.add_dependency("noon-core", dependency("noon-runtime"))
        self.add_dependency("noon", dependency("noon-native"))
        self.assertEqual(len(ratchet.violations(self.metadata, self.root)), 2)

    def test_resolve_graph_cannot_hide_a_declared_edge(self):
        self.metadata["resolve"] = {"nodes": []}
        self.add_dependency("noon-runtime", dependency("noon-web", optional=True))
        self.assertEqual(len(ratchet.violations(self.metadata, self.root)), 1)

    def test_invalid_metadata_envelope_fails_closed(self):
        broken = [None, [], {}, {**self.metadata, "version": True},
                  {**self.metadata, "version": 2}, {**self.metadata, "packages": []},
                  {**self.metadata, "packages": [None]}, {**self.metadata, "workspace_members": []},
                  {**self.metadata, "workspace_members": ["missing"]},
                  {**self.metadata, "workspace_members": [None]},
                  {**self.metadata, "workspace_root": str(self.root / "elsewhere")},
                  {**self.metadata, "packages": self.metadata["packages"] * 2},
                  {**self.metadata, "workspace_members": self.metadata["workspace_members"] * 2}]
        for value in broken:
            with self.subTest(value=value):
                with self.assertRaises(ValueError):
                    ratchet.violations(value, self.root)

    def test_missing_or_moved_or_renamed_protected_package_fails_closed(self):
        for field, value in (("name", "renamed-core"), ("manifest_path", str(self.root / "elsewhere")),
                             ("dependencies", None)):
            with self.subTest(field=field):
                metadata = copy.deepcopy(self.metadata)
                metadata["packages"][0][field] = value
                with self.assertRaises(ValueError):
                    ratchet.violations(metadata, self.root)
        self.metadata["workspace_members"].pop(0)
        with self.assertRaises(ValueError):
            ratchet.violations(self.metadata, self.root)

    def test_missing_or_malformed_dependency_fields_fail_closed(self):
        edge = dependency("noon-web")
        broken = [None, {}, *(dict((k, v) for k, v in edge.items() if k != field) for field in edge),
                  {**edge, "name": []}, {**edge, "kind": "future-kind"},
                  {**edge, "optional": "false"}, {**edge, "target": []}, {**edge, "rename": 1}]
        for value in broken:
            with self.subTest(value=value):
                self.metadata = metadata_fixture(self.root)
                self.add_dependency("noon-runtime", value)
                with self.assertRaises(ValueError):
                    ratchet.violations(self.metadata, self.root)


class CommandTests(WorkspaceTest):
    def invoke(self, *, result=None, error=None):
        stdout, stderr = io.StringIO(), io.StringIO()
        if result is None:
            result = subprocess.CompletedProcess([], 0, json.dumps(self.metadata), "")
        with mock.patch.dict(os.environ, {"NOON_ROOT": str(self.root)}), \
                mock.patch.object(ratchet.subprocess, "run", return_value=result, side_effect=error) as run, \
                contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
            status = ratchet.main()
        if status != 0:
            self.assertNotIn("ratchet passed", stdout.getvalue() + stderr.getvalue())
        return status, stdout.getvalue(), stderr.getvalue(), run

    def test_success_uses_offline_unfiltered_manifest_metadata(self):
        status, stdout, _, run = self.invoke()
        self.assertEqual(status, 0)
        self.assertIn("ratchet passed", stdout)
        run.assert_called_once_with(
            ["cargo", "metadata", "--no-deps", "--format-version", "1", "--offline",
             "--manifest-path", str(self.root / "Cargo.toml")],
            cwd=self.root, capture_output=True, text=True, check=False)

    def test_violation_is_status_one(self):
        self.add_dependency("noon-runtime", dependency("noon-web", rename="browser_host"))
        status, _, stderr, _ = self.invoke()
        self.assertEqual(status, 1)
        self.assertIn("noon-runtime must not depend on noon-web", stderr)

    def test_missing_cargo_is_status_two(self):
        status, _, stderr, _ = self.invoke(error=FileNotFoundError("cargo unavailable"))
        self.assertEqual(status, 2)
        self.assertIn("could not check", stderr)

    def test_nonzero_cargo_is_not_mistaken_for_valid_metadata(self):
        status, _, stderr, _ = self.invoke(result=subprocess.CompletedProcess(
            [], 101, json.dumps(self.metadata), "manifest is invalid"))
        self.assertEqual(status, 2)
        self.assertIn("manifest is invalid", stderr)

    def test_empty_truncated_and_wrong_shape_json_fail_closed(self):
        for text in ("", "{", "null", "{}", "[]", '{"version":1}'):
            with self.subTest(text=text):
                status, _, stderr, _ = self.invoke(result=subprocess.CompletedProcess([], 0, text, ""))
                self.assertEqual(status, 2)
                self.assertIn("could not check", stderr)

    def test_missing_manifest_fails_before_cargo(self):
        (self.root / "crates/noon-core/Cargo.toml").unlink()
        status, _, _, run = self.invoke()
        self.assertEqual(status, 2)
        run.assert_not_called()

    def test_unreadable_manifest_fails_even_for_privileged_test_runner(self):
        with mock.patch.object(Path, "open", side_effect=PermissionError("unreadable manifest")):
            status, _, stderr, run = self.invoke()
        self.assertEqual(status, 2)
        self.assertIn("unreadable manifest", stderr)
        run.assert_not_called()


class CargoFixtureTests(WorkspaceTest):
    """Invoke the production shell entrypoint with real Cargo, never a TOML imitation."""

    @classmethod
    def setUpClass(cls):
        # Fail, do not skip, if the complete suite cannot test its actual parser.
        subprocess.run(["cargo", "--version"], check=True)

    def check(self, expected: int, diagnostic: str):
        result = subprocess.run(
            ["bash", str(RATCHET)],
            env={**os.environ, "NOON_ROOT": str(self.root), "CARGO_HOME": str(self.root / "cargo-home")},
            cwd=self.root, capture_output=True, text=True, timeout=30,
        )
        output = result.stdout + result.stderr
        self.assertEqual(result.returncode, expected, output)
        self.assertIn(diagnostic, output)
        if expected:
            self.assertNotIn("ratchet passed", output)

    def test_clean_real_workspace(self):
        self.check(0, "ratchet passed")
        self.assertFalse((self.root / "target").exists(), "metadata must not compile the fixture")
        self.assertFalse((self.root / "Cargo.lock").exists(), "no-deps should not resolve dependencies")

    def test_all_allowed_engine_directions(self):
        ordered = ["noon-core", "noon-compile", "noon-runtime", "noon-render-wgpu", "noon"]
        for i, source in enumerate(ordered):
            append_manifest(self.root, source, "[dependencies]\n" + "\n".join(
                f'{target} = {{ path = "../{target}" }}' for target in ordered[:i]))
        self.check(0, "ratchet passed")

    def test_every_forbidden_direction_is_a_policy_error_not_a_cargo_error(self):
        for source, targets in ratchet.FORBIDDEN.items():
            for target in sorted(targets):
                with self.subTest(source=source, target=target):
                    write_workspace(self.root)
                    append_manifest(self.root, source, f'[dependencies]\n{target} = {{ path = "../{target}" }}')
                    self.check(1, f"{source} must not depend on {target}")

    def test_inline_alias(self):
        append_manifest(self.root, "noon-runtime", '[dependencies]\nbrowser_host = { package = "noon-web", path = "../noon-web" }')
        self.check(1, "alias=browser_host")

    def test_table_alias(self):
        append_manifest(self.root, "noon-runtime", '[dependencies.browser_host]\npackage = "noon-web"\npath = "../noon-web"')
        self.check(1, "alias=browser_host")

    def test_quoted_dependency_key(self):
        append_manifest(self.root, "noon-runtime", '[dependencies]\n"noon-web" = { path = "../noon-web" }')
        self.check(1, "noon-runtime must not depend on noon-web")

    def test_workspace_inherited_alias_inline_and_table(self):
        for section in ('[dependencies]\nbrowser_host = { workspace = true }',
                        '[dependencies.browser_host]\nworkspace = true'):
            with self.subTest(section=section):
                write_workspace(self.root)
                with (self.root / "Cargo.toml").open("a", encoding="utf-8") as manifest:
                    manifest.write('\n[workspace.dependencies]\nbrowser_host = { package = "noon-web", path = "crates/noon-web" }\n')
                append_manifest(self.root, "noon-runtime", section)
                self.check(1, "alias=browser_host")

    def test_unused_workspace_dependency_is_not_an_edge(self):
        with (self.root / "Cargo.toml").open("a", encoding="utf-8") as manifest:
            manifest.write('\n[workspace.dependencies]\nnoon-web = { path = "crates/noon-web" }\n')
        self.check(0, "ratchet passed")

    def test_disabled_optional_dependency(self):
        append_manifest(self.root, "noon-runtime", '[dependencies]\nbrowser_host = { package = "noon-web", path = "../noon-web", optional = true }\n[features]\ndefault = []')
        self.check(1, "optional")

    def test_other_target_dependencies_for_all_kinds_and_table_syntax(self):
        for section, kind in (("dependencies", "normal"), ("build-dependencies", "build"), ("dev-dependencies", "dev")):
            for table in (False, True):
                with self.subTest(kind=kind, table=table):
                    write_workspace(self.root)
                    header = f'[target.\'cfg(target_arch = "wasm32")\'.{section}'
                    text = (header + '.browser_host]\npackage = "noon-web"\npath = "../noon-web"'
                            if table else header + ']\nbrowser_host = { package = "noon-web", path = "../noon-web" }')
                    append_manifest(self.root, "noon-runtime", text)
                    self.check(1, f"kind={kind}, target=cfg(target_arch = \"wasm32\")")

    def test_build_and_dev_dependencies(self):
        for kind in ("build", "dev"):
            with self.subTest(kind=kind):
                write_workspace(self.root)
                append_manifest(self.root, "noon-runtime", f'[{kind}-dependencies]\nbrowser_host = {{ package = "noon-web", path = "../noon-web" }}')
                self.check(1, f"kind={kind}")

    def test_allowed_package_with_forbidden_alias_is_not_rejected(self):
        append_manifest(self.root, "noon-runtime", '[dependencies]\nnoon-web = { package = "helper", path = "../helper" }')
        self.check(0, "ratchet passed")

    def test_malformed_member_and_root_toml_fail_closed(self):
        for path in ("Cargo.toml", "crates/noon-runtime/Cargo.toml"):
            with self.subTest(path=path):
                write_workspace(self.root)
                (self.root / path).write_text("[invalid TOML\n", encoding="utf-8")
                self.check(2, "cargo metadata failed")

    def test_missing_inherited_dependency_fails_closed(self):
        append_manifest(self.root, "noon-runtime", '[dependencies]\nbrowser_host = { workspace = true }')
        self.check(2, "cargo metadata failed")

    def test_missing_and_unreadable_manifests_fail_closed(self):
        for path in ("Cargo.toml", "crates/noon-core/Cargo.toml"):
            with self.subTest(path=path):
                manifest = self.root / path
                manifest.unlink()
                self.check(2, "could not check")
                manifest.mkdir()
                self.check(2, "could not check")
                manifest.rmdir()
                write_workspace(self.root)

    def test_protected_package_must_be_a_workspace_member(self):
        (self.root / "Cargo.toml").write_text(
            '[workspace]\nresolver = "2"\nmembers = ["crates/noon"]\nexclude = ["crates/noon-core", "crates/noon-compile", "crates/noon-runtime", "crates/noon-render-wgpu", "crates/noon-native", "crates/noon-web", "crates/helper"]\n',
            encoding="utf-8")
        self.check(2, "missing or mismatched workspace package noon-core")


if __name__ == "__main__":
    unittest.main(verbosity=2)
