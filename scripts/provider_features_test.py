"""Regression fixtures for the active provider dependency graph gate."""
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location("provider_features", Path(__file__).with_name("provider-features.py"))
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


def tree(*extra):
    return "\n".join(name + " v0.1.0" for name in sorted(module.COMMON | set(extra)))


class GraphTests(unittest.TestCase):
    def test_allowed_configurations(self):
        module.check_graph("minimal", tree())
        module.check_graph("native-text", tree("noon-text-native", "swash"))
        module.check_graph("native-bundled", tree("noon-text-native", "swash", "typst-assets"))
        module.check_graph("typst", tree("noon-typst", "typst-library", "typst-layout"))
        module.check_graph("product", tree("noon-text-native", "swash", "noon-typst", "typst-library", "typst-layout", "typst-assets"))

    def test_minimal_rejects_provider_and_asset_edges(self):
        for dependency in ("swash", "noon-text-native", "noon-typst", "typst", "typst-library", "typst-assets"):
            with self.subTest(dependency=dependency), self.assertRaises(ValueError):
                module.check_graph("minimal", tree(dependency))

    def test_native_rejects_layout_or_bundles(self):
        for dependency in ("noon-typst", "typst-library", "typst-assets"):
            with self.subTest(dependency=dependency), self.assertRaises(ValueError):
                module.check_graph("native-text", tree("noon-text-native", "swash", dependency))

    def test_native_bundles_do_not_enable_typst(self):
        with self.assertRaises(ValueError):
            module.check_graph("native-bundled", tree("noon-text-native", "swash", "typst-assets", "noon-typst"))

    def test_typst_rejects_native_provider_and_bundles(self):
        for dependency in ("noon-text-native", "typst-assets"):
            with self.subTest(dependency=dependency), self.assertRaises(ValueError):
                module.check_graph("typst", tree("noon-typst", "typst-library", "typst-layout", dependency))

    def test_missing_malformed_and_unknown_fail_closed(self):
        for graph in ("", "not a cargo tree", "noon v0.1.0", tree() + "\nbroken line"):
            with self.subTest(graph=graph), self.assertRaises(ValueError):
                module.check_graph("minimal", graph)
        with self.assertRaises(ValueError):
            module.check_graph("unknown", tree())

    def test_deduplicated_path_and_version_records(self):
        module.check_graph("minimal", tree() + "\nnoon v0.1.0 (/some/path) (*)\nserde v1.0.0")


if __name__ == "__main__":
    unittest.main()
