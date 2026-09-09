"""Regression fixtures for the active provider dependency graph gate."""
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("provider_features", Path(__file__).with_name("provider-features.py"))
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


def tree(*extra, fonts=False):
    return "\n".join(
        name + " v0.1.0 features=" + ("fonts" if name == "typst-assets" and fonts else "")
        for name in sorted(module.COMMON | set(extra))
    )


class GraphTests(unittest.TestCase):
    def test_allowed_configurations(self):
        module.check_graph("minimal", tree())
        module.check_graph("native-text", tree("noon-text", "swash"))
        module.check_graph("native-bundled", tree("noon-text", "swash", "typst-assets", fonts=True))
        module.check_graph("typst", tree("noon-typst", "typst-library", "typst-layout", "typst-assets"))
        module.check_graph("product", tree("noon-text", "swash", "noon-typst", "typst-library", "typst-layout", "typst-assets", fonts=True))

    def test_minimal_rejects_provider_and_asset_edges(self):
        for dependency in ("swash", "noon-text", "noon-typst", "typst", "typst-library", "typst-assets"):
            with self.subTest(dependency=dependency), self.assertRaises(ValueError):
                module.check_graph("minimal", tree(dependency))

    def test_native_rejects_layout_or_bundles(self):
        for dependency in ("noon-typst", "typst-library", "typst-assets"):
            with self.subTest(dependency=dependency), self.assertRaises(ValueError):
                module.check_graph("native-text", tree("noon-text", "swash", dependency))

    def test_native_bundles_do_not_enable_typst(self):
        with self.assertRaises(ValueError):
            module.check_graph("native-bundled", tree("noon-text", "swash", "typst-assets", "noon-typst", fonts=True))

    def test_typst_allows_base_assets_but_rejects_fonts_and_native(self):
        dependencies = ("noon-typst", "typst-library", "typst-layout", "typst-assets")
        module.check_graph("typst", tree(*dependencies))
        with self.assertRaises(ValueError):
            module.check_graph("typst", tree(*dependencies, fonts=True))
        with self.assertRaises(ValueError):
            module.check_graph("typst", tree(*dependencies, "noon-text"))

    def test_bundled_configuration_requires_actual_font_feature(self):
        with self.assertRaises(ValueError):
            module.check_graph("native-bundled", tree("noon-text", "swash", "typst-assets"))

    def test_duplicate_features_are_unioned_not_overwritten(self):
        dependencies = ("noon-typst", "typst-library", "typst-layout", "typst-assets")
        with self.assertRaises(ValueError):
            module.check_graph("typst", tree(*dependencies, fonts=True) + "\ntypst-assets v0.15.1 features= (*)")

    def test_missing_malformed_and_unknown_fail_closed(self):
        for graph in ("", "not a cargo tree", "noon v0.1.0 features=", tree() + "\nbroken line", tree() + "\nserde v1.0.0"):
            with self.subTest(graph=graph), self.assertRaises(ValueError):
                module.check_graph("minimal", graph)
        with self.assertRaises(ValueError):
            module.check_graph("unknown", tree())

    def test_deduplicated_path_and_version_records(self):
        module.check_graph("minimal", tree() + "\nnoon v0.1.0 (/some/path) features= (*)\nserde v1.0.0 features=derive,std")


class BuildModeTests(unittest.TestCase):
    def test_correctness_preserves_caller_cache_and_target(self):
        original = {"RUSTC_WRAPPER": "sccache", "RUSTC_WORKSPACE_WRAPPER": "custom",
                    "CARGO_TARGET_DIR": "/cached/target", "CARGO_INCREMENTAL": "1"}
        env = module.build_env(Path("/evidence"), measure=False, base_env=original)
        for key, value in original.items():
            self.assertEqual(env[key], value)
        self.assertNotIn("CARGO_PROFILE_DEV_DEBUG", original)

    def test_measurement_is_cold_even_with_a_cached_caller(self):
        original = {"RUSTC_WRAPPER": "sccache", "RUSTC_WORKSPACE_WRAPPER": "custom",
                    "CARGO_TARGET_DIR": "/cached/target", "CARGO_INCREMENTAL": "1",
                    "CARGO_PROFILE_DEV_DEBUG": "2"}
        env = module.build_env(Path("/evidence"), measure=True, base_env=original)
        self.assertNotIn("RUSTC_WRAPPER", env)
        self.assertNotIn("RUSTC_WORKSPACE_WRAPPER", env)
        self.assertEqual(env["CARGO_TARGET_DIR"], "/evidence/target")
        self.assertEqual(env["CARGO_INCREMENTAL"], "0")
        self.assertEqual(env["CARGO_PROFILE_DEV_DEBUG"], "0")
        self.assertEqual(original["CARGO_TARGET_DIR"], "/cached/target")

    def test_correctness_default_target_is_reusable_across_reports(self):
        first = module.build_env(Path("/first-report"), measure=False, base_env={})
        second = module.build_env(Path("/second-report"), measure=False, base_env={})
        self.assertEqual(first["CARGO_TARGET_DIR"], second["CARGO_TARGET_DIR"])
        self.assertNotIn("CARGO_INCREMENTAL", first)


class FacadeCommandTests(unittest.TestCase):
    def test_all_configurations_keep_the_real_minimal_and_provider_contracts(self):
        features = {"minimal": None, "native-text": "native-text",
                    "native-bundled": "native-text,bundled-fonts",
                    "typst": "typst", "product": "default"}
        for target in ("x86_64-unknown-linux-gnu", "wasm32-unknown-unknown"):
            for config, selected in features.items():
                with self.subTest(config=config, target=target):
                    expected = ["cargo", "check", "--manifest-path", str(module.ROOT / "Cargo.toml"),
                                "-p", "noon", "--target", target, "--no-default-features"]
                    if selected:
                        expected += ["--features", selected]
                    self.assertEqual(module.facade_check_command(config, target),
                                     [*expected, "--all-targets"])
        with self.assertRaises(ValueError):
            module.facade_check_command("unknown", "x86_64-unknown-linux-gnu")


@unittest.skipUnless(os.environ.get("NOON_PROVIDER_COMPILE_TESTS") == "1",
                     "set NOON_PROVIDER_COMPILE_TESTS=1 to run the real Cargo example regression")
class ProviderExampleTests(unittest.TestCase):
    def test_unguarded_example_fails_minimal_and_guarded_example_passes(self):
        # Only the minimal/native workflow cell runs this integration test. Reuse
        # its compiler cache; no fixture workspace, manifest edits, or provider
        # implementation is introduced. The temporary ordinary facade example is
        # automatically discovered by the same --all-targets command used in CI.
        target = "x86_64-unknown-linux-gnu"
        env = module.build_env(module.ROOT / "target/provider-example-regression", measure=False)
        with tempfile.NamedTemporaryFile(dir=module.ROOT / "crates/noon/examples",
                                         prefix="provider_ci_probe_", suffix=".rs",
                                         delete=False) as stream:
            example = Path(stream.name)
        self.addCleanup(example.unlink, missing_ok=True)

        def check(config, source):
            example.write_text(source)
            command = [*module.facade_check_command(config, target), "--message-format=json"]
            print("+", " ".join(command), flush=True)
            return subprocess.run(command, cwd=module.ROOT, env=env, text=True,
                                  stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=600)

        unguarded = "use noon::Text;\nfn main() { let _ = core::mem::size_of::<Text>(); }\n"
        rejected = check("minimal", unguarded)
        self.assertNotEqual(rejected.returncode, 0, "unguarded noon::Text compiled without native-text")
        errors = []
        for line in rejected.stdout.splitlines():
            record = json.loads(line)
            if record.get("reason") == "compiler-message" and record["message"]["level"] == "error":
                errors.append(record["message"])
        self.assertTrue(errors, rejected.stderr)
        # A network/toolchain failure or an error elsewhere is not evidence that
        # minimal features caught this example. Require the structured unresolved
        # import code and a primary span in our exact temporary source file.
        for error in errors:
            self.assertEqual((error.get("code") or {}).get("code"), "E0432", error)
            self.assertTrue(any(span["is_primary"] and Path(span["file_name"]).name == example.name
                                for span in error["spans"]), error)
        print("PASS: unguarded noon::Text rejected by minimal all-targets (E0432)", flush=True)

        enabled = check("native-text", unguarded)
        self.assertEqual(enabled.returncode, 0, enabled.stderr + enabled.stdout)
        print("PASS: the same unguarded example compiles with native-text", flush=True)

        guarded = ('#[cfg(feature = "native-text")]\nuse noon::Text;\n'
                   'fn main() {\n    #[cfg(feature = "native-text")]\n'
                   '    let _ = core::mem::size_of::<Text>();\n}\n')
        corrected = check("minimal", guarded)
        self.assertEqual(corrected.returncode, 0, corrected.stderr + corrected.stdout)
        print("PASS: feature-guarded example compiles with minimal all-targets", flush=True)


if __name__ == "__main__":
    unittest.main()
