"""Regression fixtures for the active provider dependency graph gate."""
import importlib.util
from pathlib import Path
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


if __name__ == "__main__":
    unittest.main()
