"""Wrapper-only contracts; Rust and browser tests qualify actual publication."""
import os
from pathlib import Path
import subprocess
import sys
import textwrap
import unittest


SETUP = r'''
from types import SimpleNamespace
from unittest.mock import patch
import _manim_compat as compat
compat.install()
import _manim_phase_b
import _manim_semantic_handles as handles

class Handle:
    def __init__(self, slot, generation=1):
        self.semanticSlot = slot
        self.semanticGeneration = generation

class Family(Handle):
    def __init__(self, keys):
        super().__init__(90)
        self.keys = keys
        self.memberCount = len(keys)
    def memberSlot(self, index):
        return self.keys[index][0]
    def memberGeneration(self, index):
        return self.keys[index][1]

class Batch:
    def __init__(self):
        self.operands = []
    def appendMobject(self, wrapper_id, handle):
        assert wrapper_id == ""
        self.operands.append(("mobject", handle))
    def appendFamily(self, handle):
        self.operands.append(("family", handle))

class Context:
    def __init__(self):
        self.batches = []
        self.publications = 0
        self.fail = False
        self.reverse = False
    def beginMembershipBatch(self, kind):
        assert kind == "add"
        batch = Batch()
        self.batches.append(batch)
        return batch
    def createLiveFamily(self, batch):
        if self.fail:
            raise ValueError("publication rejected")
        self.publications += 1
        keys = list(dict.fromkeys((h.semanticSlot, h.semanticGeneration) for _, h in batch.operands))
        return Family(list(reversed(keys)) if self.reverse else keys)

context = Context()
def leaf(slot, generation=1):
    obj = object.__new__(compat.Circle)
    obj._semantic_handle = Handle(slot, generation)
    return obj

def forbidden_raw():
    raise AssertionError("live construction bypassed the canonical owner")
handles._create_family_handle = forbidden_raw
owner = object.__new__(compat.Group)
first, second = leaf(1), leaf(2)
'''


class LiveFamilyConstructionTests(unittest.TestCase):
    def run_case(self, body: str) -> None:
        env = dict(os.environ, PYTHONPATH=str(Path(__file__).resolve().parent))
        result = subprocess.run(
            [sys.executable, "-B", "-c", SETUP + "\n" + textwrap.dedent(body)],
            env=env, capture_output=True, text=True, timeout=15,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_live_constructor_uses_one_publication_and_shared_order(self):
        self.run_case('''
            with patch.object(handles, "_live_constructor_context", return_value=context):
                handles._group_init(owner, first, second, first)
            assert context.publications == 1
            assert len(context.batches[0].operands) == 3
            assert owner.submobjects == [first, second]
            assert owner._semantic_family_handle.memberCount == 2
        ''')

    def test_nested_family_and_generational_identity_are_preserved(self):
        self.run_case('''
            nested = object.__new__(compat.Group)
            nested._semantic_family_handle = Handle(1, 2)
            with patch.object(handles, "_live_constructor_context", return_value=context):
                handles._group_init(owner, nested, first)
            assert [kind for kind, _ in context.batches[0].operands] == ["family", "mobject"]
            assert owner.submobjects == [nested, first]
        ''')

    def test_wrapper_reads_result_order_instead_of_planning_membership(self):
        self.run_case('''
            context.reverse = True
            with patch.object(handles, "_live_constructor_context", return_value=context):
                handles._group_init(owner, first, second)
            assert owner.submobjects == [second, first]
        ''')

    def test_invalid_member_and_missing_identity_do_not_publish(self):
        self.run_case('''
            for invalid in (object(), object.__new__(compat.Circle), owner):
                with patch.object(handles, "_live_constructor_context", return_value=context):
                    try:
                        handles._group_init(owner, first, invalid)
                    except (ValueError, TypeError, RuntimeError):
                        pass
                    else:
                        raise AssertionError("invalid member accepted")
                assert context.publications == 0
                assert not hasattr(owner, "_semantic_family_handle")
        ''')

    def test_failed_publication_does_not_commit_wrapper_metadata(self):
        self.run_case('''
            context.fail = True
            with patch.object(handles, "_live_constructor_context", return_value=context):
                try:
                    handles._group_init(owner, first, second)
                except ValueError as error:
                    assert "publication rejected" in str(error)
                else:
                    raise AssertionError("publication failure swallowed")
            assert not hasattr(owner, "_semantic_family_handle")
            assert not hasattr(owner, "submobjects")
        ''')

    def test_constructor_free_copy_publishes_without_mutating_original(self):
        self.run_case('''
            original = Family([(1, 1), (2, 1)])
            owner._semantic_family_handle = original
            owner.submobjects = [first, second]
            def delegate(source):
                assert not hasattr(source, "_semantic_family_handle")
                clone = object.__new__(compat.Group)
                clone.submobjects = list(source.submobjects)
                return clone
            with patch.object(handles, "_live_constructor_context", return_value=context), \\
                 patch.object(handles, "_GROUP_COPY_DELEGATE", delegate):
                clone = handles._group_copy(owner)
            assert context.publications == 1
            assert clone.submobjects == [first, second]
            assert owner._semantic_family_handle is original
            assert clone._semantic_family_handle is not original
        ''')

    def test_transferred_execution_rejects_before_raw_construction(self):
        self.run_case('''
            import sys
            scope = SimpleNamespace(_current_authoring_scene=lambda: SimpleNamespace(
                _canonical_authoring_context=SimpleNamespace(liveExecutionOwnership=lambda: "transferred")))
            with patch.dict(sys.modules, {"_manim_reactive": scope}):
                try:
                    handles._group_init(owner, first)
                except RuntimeError as error:
                    assert "family" in str(error) and "transferred" in str(error)
                else:
                    raise AssertionError("transferred owner allowed construction")
            assert not hasattr(owner, "_semantic_family_handle")
        ''')


if __name__ == "__main__":
    unittest.main()
