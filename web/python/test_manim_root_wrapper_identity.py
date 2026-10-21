"""Wrapper identity resolution keeps Rust as membership/order authority."""
from types import SimpleNamespace
import unittest
import _manim_compat as compat
import _manim_semantic_handles as handles
from _typed_geometry_test_support import identity_only_wrapper

class RootWrapperLookupTests(unittest.TestCase):

    def wrappers(self):
        root, child, grandchild = [identity_only_wrapper(compat.VMobject) for _ in range(3)]
        nested = identity_only_wrapper(compat.Group)
        for slot, value in enumerate([root, child, grandchild], 1):
            value._semantic_handle = SimpleNamespace(semanticSlot=slot, semanticGeneration=0)
            value._semantic_handle_fresh = True
        root._semantic_handle.familyKeys = lambda: ['1:0', '4:0', '2:0', '3:0']
        child._semantic_member_wrappers = {'3:0': grandchild}
        nested._semantic_family_handle = SimpleNamespace(semanticSlot=4, semanticGeneration=0, memberKeys=lambda: ['2:0', '3:0'])
        nested._semantic_member_wrappers = {'2:0': child, '3:0': grandchild}
        root._semantic_member_wrappers = {'2:0': child, '4:0': nested}
        return (root, child, grandchild, nested)

    def test_lookup_reaches_registered_wrappers_below_ordinary_objects(self):
        root, child, grandchild, nested = self.wrappers()
        family = handles._get_family(root)
        self.assertEqual(len(family), 4)
        for actual, expected in zip(family, [root, nested, child, grandchild], strict=True):
            self.assertIs(actual, expected)

    def test_rust_keys_exclude_stale_registry_entries_before_refresh(self):
        root, child, grandchild, nested = self.wrappers()
        extra = identity_only_wrapper(compat.Group)

        def forbidden():
            self.fail('An unrelated registry entry must not become a membership query')
        extra._semantic_family_handle = SimpleNamespace(semanticSlot=9, semanticGeneration=0, memberKeys=forbidden)
        extra._semantic_member_wrappers = {}
        root._semantic_member_wrappers['9:0'] = extra
        self.assertEqual(handles._get_family(root), [root, nested, child, grandchild])

    def test_selected_family_refresh_is_preserved(self):
        root, child, grandchild, nested = self.wrappers()
        nested._semantic_member_wrappers = {}
        observed = []

        def refresh():
            observed.append(True)
            nested._semantic_member_wrappers = {'2:0': child, '3:0': grandchild}
        nested._refresh_semantic_members = refresh
        self.assertEqual(handles._get_family(root), [root, nested, child, grandchild])
        self.assertEqual(observed, [True])

    def test_registry_cycle_does_not_duplicate_rust_selected_identity(self):
        root, child, grandchild, nested = self.wrappers()
        grandchild._semantic_member_wrappers = {'1:0': root}
        self.assertEqual(handles._get_family(root), [root, nested, child, grandchild])

    def test_unselected_registry_family_does_not_refresh(self):
        root, child, grandchild, nested = self.wrappers()
        extra = identity_only_wrapper(compat.Group)
        extra._semantic_family_handle = SimpleNamespace(semanticSlot=9, semanticGeneration=0)
        extra._refresh_semantic_members = lambda: self.fail('unselected wrapper refresh')
        root._semantic_member_wrappers['9:0'] = extra
        self.assertEqual(handles._get_family(root), [root, nested, child, grandchild])

    def test_unselected_registry_entry_does_not_read_stale_handle_metadata(self):
        root, child, grandchild, nested = self.wrappers()
        owner = self

        class StaleHandle:

            @property
            def semanticSlot(self):
                owner.fail('unselected wrapper handle metadata read')
        extra = identity_only_wrapper(compat.VMobject)
        extra._semantic_handle = StaleHandle()
        root._semantic_member_wrappers['9:0'] = extra
        self.assertEqual(handles._get_family(root), [root, nested, child, grandchild])

    def test_missing_selected_wrapper_is_rejected_without_fabrication(self):
        root, child, grandchild, nested = self.wrappers()
        root._semantic_handle.familyKeys = lambda: ['1:0', '99:0']
        with self.assertRaisesRegex(RuntimeError, 'shared family identity has no Python wrapper'):
            handles._get_family(root)

    def test_order_comes_only_from_the_rust_selected_keys(self):
        root, child, grandchild, nested = self.wrappers()
        root._semantic_handle.familyKeys = lambda: ['1:0', '3:0', '4:0', '2:0']
        self.assertEqual(handles._get_family(root), [root, grandchild, nested, child])

    def test_staged_callback_rejects_before_rust_or_wrapper_reads(self):
        from unittest.mock import patch, Mock
        root, child, grandchild, nested = self.wrappers()
        root._semantic_handle.familyKeys = Mock(side_effect=AssertionError('staged read'))
        with patch('_manim_updaters.canonical_callback_phase_active', return_value=True):
            with self.assertRaisesRegex(NotImplementedError, 'staged callback'):
                handles._get_family(root)
        root._semantic_handle.familyKeys.assert_not_called()
if __name__ == '__main__':
    unittest.main()
