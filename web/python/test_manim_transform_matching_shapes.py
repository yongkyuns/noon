import unittest
from pathlib import Path

import _manim_animate as animate


class TransformMatchingShapesAdapterTests(unittest.TestCase):
    def test_constructor_is_inert_and_matches_supported_manim_call_shape(self) -> None:
        source = object()
        target = object()
        request = animate.TransformMatchingShapes(source, target, run_time=2.0)
        self.assertIs(request.source, source)
        self.assertIs(request.target, target)
        self.assertIs(request.mobject, source)
        self.assertIs(request.target_mobject, target)
        self.assertEqual(request.anim_args, {"run_time": 2.0})
        self.assertEqual(request.key_map, {})

    def test_unsupported_mismatch_modes_fail_closed(self) -> None:
        source = object()
        target = object()
        with self.assertRaises(NotImplementedError):
            animate.TransformMatchingShapes(source, target, transform_mismatches=True)
        with self.assertRaises(NotImplementedError):
            animate.TransformMatchingShapes(source, target, fade_transform_mismatches=True)
        with self.assertRaises(NotImplementedError):
            animate.TransformMatchingShapes(source, target, key_map={"a": "b"})
        with self.assertRaises(TypeError):
            animate.TransformMatchingShapes(source, target, key_map=[("a", "b")])
        animate.TransformMatchingShapes(source, target, key_map={})

    def test_python_only_selects_the_shared_matching_family_request(self) -> None:
        python_dir = Path(__file__).resolve().parent
        scene_source = (python_dir / "_manim_scene.py").read_text()
        rust_source = (
            python_dir.parent.parent
            / "crates"
            / "noon-web"
            / "src"
            / "canonical_authoring_scene.rs"
        ).read_text()
        self.assertIn("appendMatchingFamilyTransformTo", scene_source)
        self.assertIn("MatchingFamilyTransformTo", rust_source)
        self.assertNotIn("matching_shape_key", scene_source)
        self.assertNotIn("matching_shape_correspondence", scene_source)


    def test_completion_associations_at_the_python_boundary(self):
        import subprocess
        import sys
        completed = subprocess.run(
            [sys.executable, str(Path(__file__).resolve()), "--completion"],
            capture_output=True, text=True,
        )
        self.assertEqual(completed.returncode, 0, completed.stdout + completed.stderr)


def completion_association_suite():
    # Isolate browser-import test doubles from the ordinary discovery process.
    class MatchingCompletionAssociationTests(unittest.IsolatedAsyncioTestCase):
        """Exercise real facade control flow; mock only the Rust boundary receipt."""

        def setUp(self):
            import sys
            from types import SimpleNamespace, ModuleType
            from unittest.mock import Mock, patch
            bridge = ModuleType("js")
            bridge.noonResolveAnimationOptions = Mock(side_effect=AssertionError("unexpected option resolution"))
            bridge.noonResolveTransformAnimationOptions = bridge.noonResolveAnimationOptions
            imports = patch.dict(sys.modules, {"js": bridge})
            imports.start()
            self.addCleanup(imports.stop)
            import noon
            import _manim_scene as scene_api
            import _manim_compat as compat
            from _typed_geometry_test_support import identity_only_wrapper

            self.scene_api = scene_api
            self.scene = noon.Scene()
            self.source_leaf = identity_only_wrapper(compat.VMobject)
            self.target_leaf = identity_only_wrapper(compat.VMobject)
            for index, leaf in enumerate((self.source_leaf, self.target_leaf), 1):
                leaf._semantic_handle = SimpleNamespace(semanticSlot=index, semanticGeneration=0)
                leaf._semantic_handle_fresh = True
            def family(slot, leaf):
                result = identity_only_wrapper(compat.VGroup)
                key = scene_api._semantic_wrapper_key(leaf)
                result._semantic_family_handle = SimpleNamespace(
                    semanticSlot=slot, semanticGeneration=0,
                    memberKeys=lambda: [key], memberCount=1,
                )
                result._semantic_member_wrappers = {key: leaf}
                return result
            self.source = family(3, self.source_leaf)
            self.target = family(4, self.target_leaf)
            self.published = False
            self.associations = []
            def batch():
                value = SimpleNamespace(bindings=[])
                value.reserveMobjectBinding = lambda key, handle: value.bindings.append((key, handle))
                return value
            def associate(value):
                self.assertTrue(self.published)
                self.assertIsNone(self.target_leaf._scene)
                self.associations.append(value.bindings)
            self.builder = Mock()
            self.context = SimpleNamespace(
                beginOrdinaryCompositionBuilder=Mock(return_value=self.builder),
                ordinaryCanPlayComposition=Mock(return_value=True),
                ordinaryPlayComposition=Mock(side_effect=self.complete),
                beginOrdinaryComposition=Mock(),
                beginMembershipBatch=Mock(side_effect=lambda kind: batch()),
                associatePublishedBindings=Mock(side_effect=associate),
                containsMobject=Mock(side_effect=lambda handle: (
                    handle is self.target_leaf._semantic_handle if self.published
                    else handle is self.source_leaf._semantic_handle
                )),
                rootMembershipKeys=Mock(side_effect=lambda: ['4:0'] if self.published else ['3:0']),
                editMembership=Mock(side_effect=AssertionError('association must not replay membership')),
            )
            self.scene._canonical_authoring_context = self.context
            reservation = scene_api._reserve_typed_binding(
                self.source_leaf, self.scene, self.source_leaf._semantic_handle, None,
            )
            scene_api._commit_typed_binding(
                self.source_leaf, self.scene, reservation, self.source_leaf._semantic_handle,
            )
            scene_api._register_membership_wrappers(self.scene, self.source)
            self.options = SimpleNamespace(run_time=1.0, rate_func='linear', lag_ratio=0.0, path_arc=0.0)

        def complete(self, *args):
            self.published = True

        def play(self):
            from unittest.mock import patch
            with patch.object(self.scene_api, '_canonical_transform_options', return_value=self.options):
                return self.scene.play(animate.TransformMatchingShapes(self.source, self.target))

        def assert_unassociated(self):
            self.assertIs(self.source_leaf._scene, self.scene)
            self.assertIsNone(self.target_leaf._scene)
            self.assertIsNone(self.target_leaf._object)
            self.assertEqual(self.scene._next_object_id, 1)
            self.assertEqual(self.associations, [])
            self.assertNotIn('4:0', self.scene_api._membership_registry(self.scene))

        def assert_associated(self):
            self.assertIsNone(self.source_leaf._scene)
            self.assertIs(self.source_leaf._canonical_live_target_context, self.context)
            self.assertIs(self.target_leaf._scene, self.scene)
            self.assertEqual(self.scene.mobjects, [self.target])
            self.assertIs(self.scene._binding_handles[1], self.target_leaf._semantic_handle)
            self.assertEqual(self.associations, [[('1', self.target_leaf._semantic_handle)]])
            self.context.editMembership.assert_not_called()
            request = animate.Indicate(self.target)
            self.assertEqual(
                self.scene_api._canonical_indicate_animation(self.scene, request),
                (self.target, True),
            )

        def test_endpoint_completion_associates_original_target_for_next_animation(self):
            self.assertIs(self.play(), self.scene)
            self.builder.appendMatchingFamilyTransformTo.assert_called_once()
            self.assert_associated()

        def test_rejected_activation_does_not_attach_target(self):
            self.context.ordinaryPlayComposition.side_effect = RuntimeError('rejected activation')
            with self.assertRaisesRegex(ValueError, 'rejected activation'):
                self.play()
            self.assert_unassociated()
            self.context.associatePublishedBindings.assert_not_called()

        async def test_async_completion_waits_for_successful_shared_receipt(self):
            from unittest.mock import patch, AsyncMock
            self.scene_api._begin_async_continuation_construct(self.scene)
            try:
                with patch.object(self.scene_api, '_require_semantic_continuation_active'), \
                     patch.object(self.scene_api, '_prepare_semantic_continuation_callbacks'):
                    pending = self.play()
                self.assert_unassociated()
                with patch.object(self.scene_api, '_await_semantic_continuation',
                                  AsyncMock(side_effect=self.complete)):
                    self.assertIs(await pending, self.scene)
                self.assert_associated()
            finally:
                self.scene_api._finish_async_continuation_construct(self.scene)

        async def test_failed_completion_does_not_commit_wrapper_associations(self):
            from unittest.mock import patch, AsyncMock
            self.scene_api._begin_async_continuation_construct(self.scene)
            try:
                with patch.object(self.scene_api, '_require_semantic_continuation_active'), \
                     patch.object(self.scene_api, '_prepare_semantic_continuation_callbacks'):
                    pending = self.play()
                with patch.object(self.scene_api, '_await_semantic_continuation',
                                  AsyncMock(side_effect=RuntimeError('completion rejected'))):
                    with self.assertRaisesRegex(RuntimeError, 'completion rejected'):
                        await pending
                self.assert_unassociated()
                self.context.associatePublishedBindings.assert_not_called()
            finally:
                self.scene_api._finish_async_continuation_construct(self.scene)

        def test_failed_binding_association_does_not_partially_commit_python_registry(self):
            self.context.associatePublishedBindings.side_effect = RuntimeError('binding rejected')
            with self.assertRaisesRegex(RuntimeError, 'binding rejected'):
                self.play()
            self.assert_unassociated()
    return unittest.defaultTestLoader.loadTestsFromTestCase(MatchingCompletionAssociationTests)


if __name__ == "__main__":
    import sys
    if "--completion" in sys.argv:
        result = unittest.TextTestRunner(verbosity=2).run(completion_association_suite())
        sys.exit(0 if result.wasSuccessful() else 1)
    unittest.main()
