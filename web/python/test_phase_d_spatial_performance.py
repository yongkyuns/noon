"""Bounded-context tests for the existing Phase D performance-corpus fixture."""

import ast
import asyncio
import json
import math
import unittest
from pathlib import Path
from unittest.mock import AsyncMock, Mock

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / "web/python/examples/perf_spatial_workloads.py"
MANIFEST = ROOT / "benchmarks/performance-scenes.json"


def load_context_validator():
    tree = ast.parse(SOURCE.read_text())
    definitions = [
        node for node in tree.body
        if isinstance(node, (ast.Assign, ast.AnnAssign))
        and any(
            isinstance(target, ast.Name)
            and target.id in {"_WORKLOADS", "_ALLOWED_CONTEXT_KEYS", "_DEFAULT_DURATION"}
            for target in (node.targets if isinstance(node, ast.Assign) else [node.target])
        )
    ]
    function = next(
        node for node in tree.body
        if isinstance(node, ast.FunctionDef) and node.name == "_validated_workload_context"
    )
    namespace = {"math": math}
    code = compile(ast.Module(body=[*definitions, function], type_ignores=[]), str(SOURCE), "exec")
    exec(code, namespace)
    return namespace[function.name]


class PhaseDSpatialPerformanceTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.validate_context = staticmethod(load_context_validator())

    def test_catalog_selects_all_five_workloads_with_bounded_contexts(self):
        manifest = json.loads(MANIFEST.read_text())
        cases = [
            case for case in manifest["cases"]
            if case["source"] == "./python/examples/perf_spatial_workloads.py"
        ]
        self.assertEqual(len(cases), 5)
        self.assertEqual(
            {case["context"]["workload"] for case in cases},
            {
                "camera-only",
                "moving-mesh-many-static",
                "dense-surface-family",
                "moving-point-light",
                "mixed-depth-text-hud",
            },
        )
        for case in cases:
            with self.subTest(case=case["id"]):
                parsed = self.validate_context(case["context"])
                self.assertGreaterEqual(parsed["duration"], 8.0)
                self.assertLessEqual(parsed["duration"], 10.0)

    def test_defaults_and_upper_bounds_are_explicit(self):
        self.assertEqual(
            self.validate_context({}),
            {
                "workload": "camera-only",
                "duration": 10.0,
                "static_mesh_count": 600,
                "surface_resolution": 24,
            },
        )
        self.assertEqual(
            self.validate_context({"workload": "moving-mesh-many-static", "static_mesh_count": 600})[
                "static_mesh_count"
            ],
            600,
        )
        self.assertEqual(
            self.validate_context({"workload": "dense-surface-family", "surface_resolution": 32})[
                "surface_resolution"
            ],
            32,
        )

    def test_dense_surface_workload_uses_camera_motion_for_retained_family(self):
        tree = ast.parse(SOURCE.read_text())
        scene_class = next(node for node in tree.body if isinstance(node, ast.ClassDef)
                           and node.name == "PhaseDSpatialPerformance")
        method = next(node for node in scene_class.body
                      if isinstance(node, ast.AsyncFunctionDef)
                      and node.name == "_dense_surface_family")
        surface = object()
        make_surface = Mock(return_value=surface)
        world_endpoint = Mock(side_effect=AssertionError("families have no pose endpoint"))
        camera = AsyncMock()
        scene = Mock(move_camera=camera)
        namespace = {"Surface": make_surface, "BLUE_D": object(), "BLUE_E": object(),
                     "Color": lambda *values: values, "WorldTransformTo": world_endpoint,
                     "math": math, "linear": object()}
        exec(compile(ast.Module(body=[method], type_ignores=[]), str(SOURCE), "exec"), namespace)
        asyncio.run(namespace[method.name](scene, 9.0, 32))

        make_surface.assert_called_once()
        options = make_surface.call_args.kwargs
        self.assertEqual(options["resolution"], (32, 32))
        self.assertEqual(options["checkerboard_colors"], (namespace["BLUE_D"], namespace["BLUE_E"]))
        self.assertEqual(options["fill_color"], (0.16, 0.56, 0.86))
        self.assertEqual((options["fill_opacity"], options["stroke_width"], options["shade_in_3d"]), (1.0, 0.0, False))
        self.assertEqual(tuple(make_surface.call_args.args[0](0.2, 0.3)), (0.2, 0.3, 0.16 * math.sin(0.4) * math.cos(0.6)))
        scene.add_world_mobjects.assert_called_once_with(surface)
        camera.assert_awaited_once()
        self.assertEqual(camera.call_args.kwargs["run_time"], 9.0)
        self.assertIs(camera.call_args.kwargs["rate_func"], namespace["linear"])
        world_endpoint.assert_not_called()

    def test_invalid_context_is_rejected_before_construction(self):
        invalid = [
            ({"workload": "unknown"}, ValueError),
            ({"duration": True}, TypeError),
            ({"duration": 10.1}, ValueError),
            ({"static_mesh_count": True}, TypeError),
            ({"static_mesh_count": 601}, ValueError),
            ({"surface_resolution": 33}, ValueError),
            ({"unexpected": 1}, ValueError),
        ]
        for context, error in invalid:
            with self.subTest(context=context):
                with self.assertRaises(error):
                    self.validate_context(context)


if __name__ == "__main__":
    unittest.main()
