import importlib.util
import json
import types
import unittest
from pathlib import Path


ROOT = Path(__file__).parents[2]
SCRIPT = ROOT / "scripts" / "manim-api-coverage.py"
SPEC = importlib.util.spec_from_file_location("manim_api_coverage", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
coverage = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(coverage)


class ManimApiCoveragePolicyTests(unittest.TestCase):
    def test_exported_facade_with_ready_browser_evidence_cannot_remain_blocked(self) -> None:
        policy = {
            "reference": {"version": "0.21.0"},
            "statuses": ["supported", "partial", "blocked", "deferred", "missing"],
            "overrides": {},
            "default": {"status": "missing", "category": "other"},
        }
        rows = {
            "Text": {
                "status": "blocked",
                "category": "text-math",
                "noon_exported": True,
            },
            "Axes": {
                "status": "blocked",
                "category": "plotting",
                "noon_exported": False,
            },
            "ThreeDScene": {
                "status": "deferred",
                "category": "3d",
                "noon_exported": False,
            },
        }
        tutorial_examples = [
            {"id": "text-fixture", "status": "ready", "features": ["Text"]},
            {"id": "plot-backlog", "status": "blocked", "features": ["Axes"]},
        ]

        errors = coverage.validate(
            policy,
            types.SimpleNamespace(__version__="0.21.0"),
            rows,
            tutorial_examples,
        )

        self.assertTrue(any("Text" in error and "ready browser evidence" in error for error in errors))
        self.assertFalse(any("Axes" in error and "ready browser evidence" in error for error in errors))
        self.assertFalse(any("ThreeDScene" in error and "ready browser evidence" in error for error in errors))

    def test_current_policy_records_bounded_coordinate_subsets(self) -> None:
        policy = json.loads(
            (ROOT / "compat" / "manim-v0.21.0.json").read_text(encoding="utf-8")
        )
        self.assertEqual(policy["overrides"]["Text"]["status"], "partial")
        self.assertEqual(policy["overrides"]["Typst"]["status"], "partial")
        for name in (
            "NumberLine", "UnitInterval", "NumberPlane", "ImplicitFunction",
            "ComplexPlane",
            "PolarPlane",
            "ParametricFunction",
            "FunctionGraph",
        ):
            with self.subTest(name=name):
                self.assertEqual(policy["overrides"][name]["status"], "partial")
                self.assertTrue(policy["overrides"][name]["evidence"])
                self.assertTrue(policy["overrides"][name]["reason"])

    def test_module_only_space_helpers_do_not_enter_the_public_policy(self) -> None:
        policy = json.loads((ROOT / "compat" / "manim-v0.21.0.json").read_text())
        exports = coverage.noon_public_exports()
        for name in ('normalize_along_axis', 'rotation_matrix_from_quaternion', 'rotation_matrix_transpose_from_quaternion'):
            with self.subTest(name=name):
                self.assertNotIn(name, exports)
                self.assertNotIn(name, policy["overrides"])

    def test_iterable_submodule_helpers_are_not_top_level_manim_symbols(self) -> None:
        policy = json.loads((ROOT / "compat" / "manim-v0.21.0.json").read_text())
        for name in ("batch_by_property", "uniq_chain", "hash_obj"):
            with self.subTest(name=name):
                self.assertNotIn(name, policy["overrides"])

    def test_mobject_methods_are_not_classified_as_top_level_exports(self) -> None:
        policy = json.loads((ROOT / "compat" / "manim-v0.21.0.json").read_text())
        for name in ("add_background_rectangle", "match_z"):
            with self.subTest(name=name):
                self.assertNotIn(name, policy["overrides"])

    def test_mobject_always_disposition_covers_live_callback_invocation(self) -> None:
        policy = json.loads((ROOT / "compat" / "manim-v0.21.0.json").read_text())
        reason = policy["overrides"]["Mobject"]["reason"]
        self.assertIn("Mobject.always returns a builder", reason)
        self.assertIn("Detached and scene-owned objects register each updater before its immediate call", reason)
        self.assertIn("scripts/manim-differential.py:always_builder", policy["overrides"]["Mobject"]["evidence"])
        self.assertIn("scripts/manim-differential.py:always_builder_live", policy["overrides"]["Mobject"]["evidence"])

    def test_curves_as_submobjects_is_exported_with_a_bounded_disposition(self) -> None:
        policy = json.loads((ROOT / "compat" / "manim-v0.21.0.json").read_text())
        self.assertIn("CurvesAsSubmobjects", coverage.noon_public_exports())
        disposition = policy["overrides"]["CurvesAsSubmobjects"]
        self.assertEqual(disposition["status"], "partial")
        self.assertIn("point_from_proportion", disposition["reason"])
        self.assertIn("scripts/manim-differential.py:curves_as_submobjects_live", disposition["evidence"])

    def test_group_membership_growth_is_disposed_with_atomic_rust_evidence(self) -> None:
        policy = json.loads((ROOT / "compat" / "manim-v0.21.0.json").read_text())
        reason = policy["overrides"]["Group"]["reason"]
        self.assertIn("add_n_more_submobjects()", reason)
        self.assertIn("align_submobjects()", reason)
        self.assertIn("one shared Rust transaction", reason)
        self.assertIn("scripts/manim-differential.py:group_add_n_more_submobjects_live",
                      policy["overrides"]["Group"]["evidence"])
        self.assertIn("scripts/manim-differential.py:group_align_submobjects_live",
                      policy["overrides"]["Group"]["evidence"])

    def test_leaf_arrangement_records_shared_move_and_live_fixture(self) -> None:
        policy = json.loads((ROOT / "compat" / "manim-v0.21.0.json").read_text())
        reason = policy["overrides"]["Mobject"]["reason"]
        self.assertIn("arrange_submobjects() center a point-owning leaf", reason)
        self.assertIn("align_submobjects() matches the leaf-to-leaf no-op", reason)
        self.assertIn("Mobject.align_data() composes null-point", reason)
        self.assertIn("different point-ownership states reject before mutation", reason)
        self.assertIn("scripts/manim-differential.py:leaf_arrange_submobjects_live",
                      policy["overrides"]["Mobject"]["evidence"])
        self.assertIn("scripts/manim-differential.py:leaf_align_submobjects_live",
                      policy["overrides"]["Mobject"]["evidence"])
        self.assertIn("scripts/manim-differential.py:group_align_data_live",
                      policy["overrides"]["Mobject"]["evidence"])

    def test_vdict_is_exported_with_shared_family_evidence_and_show_keys_limit(self) -> None:
        policy = json.loads((ROOT / "compat" / "manim-v0.21.0.json").read_text())
        self.assertIn("VDict", coverage.noon_public_exports())
        disposition = policy["overrides"]["VDict"]
        self.assertEqual(disposition["status"], "partial")
        self.assertIn("show_keys", disposition["reason"])
        self.assertIn("scripts/manim-differential.py:vdict_mapping_live", disposition["evidence"])

    def test_static_export_audit_captures_dynamic_public_mapping(self) -> None:
        self.assertIn("Write", coverage.noon_public_exports())
        self.assertIn("Unwrite", coverage.noon_public_exports())

    def test_standard_easings_stay_module_only_in_exports_and_policy(self) -> None:
        policy = json.loads((ROOT / "compat" / "manim-v0.21.0.json").read_text())
        exports = coverage.noon_public_exports()
        self.assertIn("rate_functions", exports)
        self.assertEqual(policy["overrides"]["rate_functions"]["status"], "partial")
        for family in ("quad", "cubic", "quart", "quint", "sine", "expo", "circ", "back", "elastic", "bounce"):
            for direction in ("in", "out", "in_out"):
                name = f"ease_{direction}_{family}"
                with self.subTest(name=name):
                    self.assertNotIn(name, exports)
                    self.assertNotIn(name, policy["overrides"])

    def test_static_export_audit_captures_installed_namespace(self) -> None:
        exports = coverage.noon_public_exports()
        for name in (
            "np", "PURE_RED", "PURE_GREEN", "PURE_BLUE", "PURE_CYAN",
            "PURE_MAGENTA", "PURE_YELLOW",
        ):
            with self.subTest(name=name):
                self.assertIn(name, exports)

    def test_temporary_export_list_does_not_leak_between_scopes(self) -> None:
        tree = coverage.ast.parse(
            "def install():\n"
            "    exports = list(module.__all__)\n"
            "    for name, value in {'Public': object()}.items():\n"
            "        exports.append(name)\n"
            "    module.__all__ = exports\n"
            "def internal():\n"
            "    exports = []\n"
            "    for name, value in {'Internal': object()}.items():\n"
            "        exports.append(name)\n"
        )
        self.assertEqual(coverage._dynamic_public_mapping_exports(tree, {}), {"Public"})

    def test_static_export_audit_ignores_internal_mapping_loops(self) -> None:
        tree = coverage.ast.parse(
            "for name, value in {'Internal': object()}.items():\n"
            "    setattr(module, name, value)\n"
        )
        loop = tree.body[0]
        self.assertIsInstance(loop, coverage.ast.For)
        self.assertEqual(coverage._public_mapping_keys(loop.iter), {"Internal"})
        self.assertFalse(coverage._loop_registers_public_name(loop))


if __name__ == "__main__":
    unittest.main()
