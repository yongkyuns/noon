"""Check the production method-target route at the shared Rust boundary.

The real ImageGlow browser fixture qualifies execution and pixels. This focused
test records the request sent by the production composition branch and uses the
real observation adapter, so it also protects effective-content routing without
requiring a browser or reproducing an interpolation algorithm in Python.
"""
import ast
import math
from pathlib import Path
from types import SimpleNamespace as NS
import unittest
from unittest.mock import Mock

import noon
import _manim_semantic_handles as shared


class MethodTarget:
    pass


class MethodTargetContentRoutingTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        path = Path(__file__).with_name("_manim_scene.py")
        tree = ast.parse(path.read_text())
        candidate = next(node for node in tree.body
                         if isinstance(node, ast.FunctionDef)
                         and node.name == "_build_canonical_composition_candidate")
        branch = next(node for node in ast.walk(candidate)
                      if isinstance(node, ast.If)
                      and ast.unparse(node.test) == "affine is not None")
        route = ast.FunctionDef(
            name="route",
            args=ast.arguments(posonlyargs=[], args=[ast.arg(arg=name) for name in
                               ("builder", "affine", "child_kwargs", "reserve")],
                               kwonlyargs=[], kw_defaults=[], defaults=[]),
            body=branch.body, decorator_list=[],
        )
        module = ast.fix_missing_locations(ast.Module(body=[route], type_ignores=[]))
        namespace = {
            "math": math,
            "_semantic_handles": shared,
            "_animate": NS(_AlignedAnimationBuilder=MethodTarget),
            "_typst": NS(_RetainedTextMobject=type("RetainedText", (), {})),
            "_canonical_transform_options": lambda leaf, kwargs: NS(
                run_time=1.5, rate_func="smooth", path_arc=0.0,
                reverse_rate_function=False,
            ),
        }
        # Preserve the real source filename for source-based coverage reporting.
        # A synthetic suffix cannot be resolved by coverage.py at report time.
        exec(compile(module, str(path), "exec"), namespace)
        cls.route = staticmethod(namespace["route"])

    def setUp(self):
        # A generic wrapper deliberately cannot identify its semantic content.
        self.source = object.__new__(noon.Mobject)
        self.query = Mock(return_value=True)
        self.authored_query = Mock(side_effect=AssertionError("stale authored content"))
        self.source_handle = NS(rotation=0.0, requiresContentBoundary=self.authored_query)
        self.source._semantic_handle = self.source_handle
        self.source._semantic_handle_fresh = True
        self.source._scene = NS(_canonical_authoring_context=NS(
            queryMobjectRequiresContentBoundary=self.query,
        ))
        self.source._object = object()
        self.target_handle = NS(rotation=math.pi / 12)
        self.target = NS(_semantic_handle=self.target_handle)
        self.builder = Mock()
        self.reserve = Mock(return_value=NS(object=NS(id=77)))

    def invoke(self, leaf=None):
        self.route(self.builder, (self.source, self.target,
                                 MethodTarget() if leaf is None else leaf), {}, self.reserve)

    def test_isolated_route_uses_resolvable_production_source_filename(self):
        self.assertEqual(
            Path(self.route.__code__.co_filename),
            Path(__file__).with_name("_manim_scene.py"),
        )

    def test_bound_content_uses_live_rust_classification_for_affine_method_target(self):
        self.invoke()
        self.query.assert_called_once_with(self.source_handle)
        self.authored_query.assert_not_called()
        self.builder.appendMethodTransformTo.assert_called_once_with(
            None, self.source_handle, self.target_handle, False, 1.5, "smooth", 0.0, False,
        )
        self.reserve.assert_not_called()

    def test_vector_content_keeps_point_correspondence_for_rotating_method_target(self):
        self.query.return_value = False
        self.invoke()
        self.builder.appendMethodTransformTo.assert_called_once_with(
            None, self.source_handle, self.target_handle, True, 1.5, "smooth", 0.0, False,
        )

    def test_detached_content_uses_shared_handle_before_membership_reservation(self):
        self.source._scene = self.source._object = None
        self.authored_query.side_effect = None
        self.authored_query.return_value = True
        self.invoke()
        self.authored_query.assert_called_once_with()
        self.query.assert_not_called()
        self.builder.appendMethodTransformTo.assert_called_once_with(
            "77", self.source_handle, self.target_handle, False, 1.5, "smooth", 0.0, False,
        )

    def test_nonrotating_method_target_does_not_add_content_observation(self):
        self.target_handle.rotation = self.source_handle.rotation
        self.invoke()
        self.query.assert_not_called()
        self.assertFalse(self.builder.appendMethodTransformTo.call_args.args[3])

    def test_explicit_transform_keeps_ordinary_affine_routing(self):
        self.invoke(leaf=object())
        self.query.assert_not_called()
        self.builder.appendMethodTransformTo.assert_not_called()
        self.builder.appendTransformTo.assert_called_once_with(
            self.source_handle, self.target_handle, 1.5, "smooth", 0.0, False,
        )

    def test_failed_shared_classification_does_not_append_or_reserve(self):
        error = RuntimeError("stale publication")
        self.query.side_effect = error
        with self.assertRaises(RuntimeError) as raised:
            self.invoke()
        self.assertIs(raised.exception, error)
        self.assertEqual(self.builder.mock_calls, [])
        self.reserve.assert_not_called()


if __name__ == "__main__":
    unittest.main()
