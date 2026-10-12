import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimArrowAnimateTargetTests(unittest.TestCase):
    def test_arrow_target_copy_rebinds_only_through_copied_family(self) -> None:
        python_dir = Path(__file__).resolve().parent
        env = os.environ.copy()
        existing_pythonpath = env.get("PYTHONPATH")
        env["PYTHONPATH"] = (
            str(python_dir)
            if not existing_pythonpath
            else os.pathsep.join((str(python_dir), existing_pythonpath))
        )

        source = textwrap.dedent(
            """
            import sys
            import types

            fake_js = types.ModuleType("js")
            next_slot = 1

            def allocate_slot():
                global next_slot
                slot = next_slot
                next_slot += 1
                return slot

            class Leaf:
                def __init__(self):
                    self.semanticSlot = allocate_slot()
                    self.semanticGeneration = 0

                def identity(self):
                    return ("mobject", self.semanticSlot, self.semanticGeneration)

                def targetEditor(self):
                    return Leaf()

            class Family:
                def __init__(self, members=()):
                    self.semanticSlot = allocate_slot()
                    self.semanticGeneration = 0
                    self.members = list(members)
                    self.calls = []

                def identity(self):
                    return ("family", self.semanticSlot, self.semanticGeneration)

                @property
                def memberCount(self):
                    return len(self.members)

                def memberKeys(self):
                    return [f"{member.semanticSlot}:{member.semanticGeneration}" for member in self.members]

                def layout(self):
                    return self

                def shiftBy(self, x, y):
                    self.calls.append(("shiftBy", float(x), float(y)))

                def copyFamily(self, references):
                    self.calls.append("copyFamily")
                    return FamilyCopy(self, references)

            class FamilyCopy:
                def __init__(self, root, references):
                    self.copied = {}
                    self.copy(root)
                    for reference in references.members:
                        self.copy(reference)

                def copy(self, source):
                    existing = self.copied.get(id(source))
                    if existing is not None:
                        return existing
                    if isinstance(source, Family):
                        target = Family()
                        self.copied[id(source)] = target
                        target.members = [self.copy(member) for member in source.members]
                    else:
                        target = source.targetEditor()
                        self.copied[id(source)] = target
                    return target

                def familyFor(self, source):
                    return self.copied[id(source)]

                def mobjectFor(self, source):
                    return self.copied[id(source)]

                def arrowFor(self, source, index=None):
                    return source.rebind(self, index)

            class OpaqueArrowHandle:
                def __deepcopy__(self, memo):
                    raise TypeError("opaque Arrow JS capability must not be copied")

            class CreatedArrow(OpaqueArrowHandle):
                def __init__(self):
                    self._shaft = Leaf()
                    self._start = Leaf()
                    self._end = Leaf()
                    self._family = Family((self._shaft, self._start, self._end))
                    self.hasStartTip = True
                    self.calls = []

                def family(self): return self._family
                def shaft(self): return self._shaft
                def startTip(self): return self._start
                def endTip(self): return self._end
                def startX(self): return -1.0
                def startY(self): return 0.0
                def endX(self): return 1.0
                def endY(self): return 0.0
                def length(self): return 2.0
                def scale(self, factor, scale_tips):
                    self.calls.append(("scale", float(factor), bool(scale_tips)))

                def rebind(self, copied, index=None):
                    assert index is None
                    return ReboundArrow(self, copied.familyFor(self._family))

            class ReboundArrow(OpaqueArrowHandle):
                def __init__(self, source, family, start=(-1.0, 0.0), end=(1.0, 0.0)):
                    self.source = source
                    self._family = family
                    self._start = start
                    self._end = end
                    self.hasStartTip = True
                    self.calls = []

                def family(self): return self._family
                def startX(self): return self._start[0]
                def startY(self): return self._start[1]
                def endX(self): return self._end[0]
                def endY(self): return self._end[1]
                def length(self): return self._end[0] - self._start[0]
                def scale(self, factor, scale_tips):
                    self.calls.append(("scale", float(factor), bool(scale_tips)))

            class OpaqueFieldHandle(OpaqueArrowHandle):
                def __init__(self, family, vector_families):
                    self._family = family
                    self._vector_families = vector_families
                    self.rebind_calls = []
                    self.scale_calls = []

                def family(self): return self._family

                def rebind(self, copied, index=None):
                    self.rebind_calls.append(index)
                    if index is None:
                        return ReboundField(copied.familyFor(self._family))
                    return ReboundArrow(
                        self,
                        copied.familyFor(self._vector_families[index]),
                        start=(2.0, 0.0),
                        end=(4.0, 0.0),
                    )

                def scale(self, factor, scale_tips):
                    self.scale_calls.append(("scale", float(factor), bool(scale_tips)))

            class ReboundField(OpaqueArrowHandle):
                def __init__(self, family):
                    self._family = family
                    self.scale_calls = []

                def family(self): return self._family
                def vectorStartX(self, index): assert index == 0; return 2.0
                def vectorStartY(self, index): assert index == 0; return 0.0
                def vectorEndX(self, index): assert index == 0; return 4.0
                def vectorEndY(self, index): assert index == 0; return 0.0
                def vectorLength(self, index): assert index == 0; return 2.0
                def scale(self, factor, scale_tips):
                    self.scale_calls.append(("scale", float(factor), bool(scale_tips)))

            class Options:
                def setBuff(self, value): pass
                def setTipLength(self, value): pass
                def setMaxTipLengthToLengthRatio(self, value): pass
                def setMaxStrokeWidthToLengthRatio(self, value): pass
                def setStrokeWidth(self, value): pass

            class ArrowOptions:
                @staticmethod
                def doubleArrow(sx, sy, ex, ey): return Options()

            class GeometryOptions:
                @staticmethod
                def emptyPath(): return types.SimpleNamespace()

            class MembershipBatch:
                def __init__(self):
                    self.members = []

                def appendFamily(self, handle):
                    self.members.append(handle)

                def appendMobject(self, wrapper_id, handle):
                    assert wrapper_id == ""
                    self.members.append(handle)

            fake_js.noonAuthoringArrowOptions = ArrowOptions
            fake_js.noonCreateAuthoringArrowHandle = lambda options: CreatedArrow()
            fake_js.noonAuthoringGeometryOptions = GeometryOptions
            fake_js.noonAuthoringVectorPath = lambda: None
            fake_js.noonCreateAuthoringGeometryHandle = lambda value: value
            fake_js.noonCreateAuthoringFamilyHandle = lambda batch, z: None
            fake_js.noonAuthoringMembershipBatch = lambda kind: MembershipBatch()
            sys.modules["js"] = fake_js

            import _manim_compat
            import _manim_geometry  # installs semantic wrapper copy hooks
            import _manim_semantic_handles  # installs family copy operations
            import _manim_animate  # installs Group.animate
            import _manim_arrow as arrows

            source = arrows.DoubleArrow((-1.0, 0.0), (1.0, 0.0))
            source_family = source._semantic_family_handle
            source_members = list(source.submobjects)

            builder = source.animate
            target = builder.target

            assert target is not source
            assert target._semantic_family_handle is not source_family
            assert source_family.calls == ["copyFamily"]
            assert target._semantic_arrow_handle is not source._semantic_arrow_handle
            assert target._semantic_arrow_handle.family() is target._semantic_family_handle
            assert target._shaft is target.submobjects[0]
            assert target.start_tip is target.submobjects[1]
            assert target.tip is target.submobjects[2]
            assert all(copied is not original for copied, original in zip(target.submobjects, source_members))
            assert [member._semantic_handle for member in target.submobjects] != [
                member._semantic_handle for member in source_members
            ]
            assert tuple(target.get_start()) == (-1.0, 0.0)
            assert tuple(target.get_end()) == (1.0, 0.0)
            assert target.get_length() == 2.0

            assert target.scale(0.5, scale_tips=True) is target
            assert target._semantic_arrow_handle.calls == [("scale", 0.5, True)]
            assert source._semantic_arrow_handle.calls == []

            builder.shift((0.25, -0.5))
            assert source_family.calls == ["copyFamily"]
            assert target._semantic_family_handle.calls == [("shiftBy", 0.25, -0.5)]

            field_vector = object.__new__(arrows.Vector)
            field_vector._shaft = source._shaft
            field_vector.tip = source.tip
            field_vector.start_tip = None
            field_vector._semantic_family_handle = Family((
                source._shaft._semantic_handle, source.tip._semantic_handle,
            ))
            field_vector._semantic_member_wrappers = {
                _manim_semantic_handles._family_wrapper_key(source._shaft): source._shaft,
                _manim_semantic_handles._family_wrapper_key(source.tip): source.tip,
            }
            field = object.__new__(_manim_compat.Group)
            field._semantic_family_handle = Family((field_vector._semantic_family_handle,))
            field_handle = OpaqueFieldHandle(
                field._semantic_family_handle, [field_vector._semantic_family_handle],
            )
            field._semantic_arrow_handle = field_handle
            field_vector._semantic_arrow_handle = field_handle
            field_vector._semantic_arrow_index = 0
            field._semantic_member_wrappers = {
                _manim_semantic_handles._family_wrapper_key(field_vector): field_vector,
            }
            field_target = field.animate.target
            field_target_vector = field_target.submobjects[0]
            assert field_handle.rebind_calls == [None, 0]
            assert field_target._semantic_arrow_handle is not field._semantic_arrow_handle
            assert field_target._semantic_arrow_handle.family() is field_target._semantic_family_handle
            assert field_target_vector._semantic_arrow_handle is not field_target._semantic_arrow_handle
            assert not hasattr(field_target_vector, "_semantic_arrow_index")
            assert tuple(field_target_vector.get_start()) == (2.0, 0.0)
            assert field_target_vector.get_length() == 2.0
            assert field_target_vector.scale(0.5, scale_tips=True) is field_target_vector
            assert field_target_vector._semantic_arrow_handle.calls == [("scale", 0.5, True)]
            assert field_handle.scale_calls == []

            selected_target = field_vector.animate.target
            assert field_handle.rebind_calls == [None, 0, 0]
            assert not hasattr(selected_target, "_semantic_arrow_index")
            assert tuple(selected_target.get_start()) == (2.0, 0.0)
            assert selected_target.get_length() == 2.0
            assert selected_target.scale(0.5, scale_tips=True) is selected_target
            assert selected_target._semantic_arrow_handle.calls == [("scale", 0.5, True)]
            assert field_handle.scale_calls == []
            """
        )
        completed = subprocess.run(
            [sys.executable, "-c", source],
            cwd=python_dir,
            env=env,
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(
            completed.returncode,
            0,
            "Arrow animate-target subprocess failed:\n"
            f"stdout:\n{completed.stdout}\nstderr:\n{completed.stderr}",
        )


if __name__ == "__main__":
    unittest.main()
