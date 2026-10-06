import os
import inspect
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimVectorSpaceTests(unittest.TestCase):
    def test_pinned_manim_ghosts_get_piece_movement_not_generic_apply_function(self) -> None:
        try:
            from manim.scene.vector_space_scene import LinearTransformationScene as PinnedLTS
        except ImportError:
            self.skipTest("pinned Manim is available in the Product Gate environment")
        piece_movement = inspect.getsource(PinnedLTS.get_piece_movement)
        generic_apply = inspect.getsource(PinnedLTS.apply_function)
        self.assertIn("start.copy().fade(0.7)", piece_movement)
        self.assertIn("ApplyPointwiseFunction(function, t_mob)", generic_apply)
        self.assertIn("self.get_vector_movement(function)", generic_apply)

    def test_vector_scene_add_vector_uses_grow_arrow_for_animated_default(self) -> None:
        python_dir = Path(__file__).resolve().parent
        env = os.environ.copy()
        env["PYTHONPATH"] = os.pathsep.join(
            value for value in (str(python_dir), env.get("PYTHONPATH")) if value
        )
        source = textwrap.dedent(
            """
            import _manim_arrow
            import _manim_growing
            from noon import (
                GrowArrow as PublicGrowArrow,
                LinearTransformationScene,
                VectorScene,
            )
            import inspect
            assert PublicGrowArrow is _manim_growing.GrowArrow
            assert inspect.signature(VectorScene.add_vector).parameters["animate"].default is True
            assert inspect.signature(LinearTransformationScene.add_vector).parameters["animate"].default is False

            # A family wrapper has no per-object Scene field. Admission checks
            # the leaves while Rust owns the Arrow family's membership.
            import sys
            from types import SimpleNamespace
            sys.modules["js"] = SimpleNamespace(noonResolveAnimationOptions=lambda *args: None)
            import _manim_scene
            import _manim_compat
            from noon import Mobject, Scene
            from unittest.mock import patch
            real_vector = object.__new__(_manim_arrow.Vector)
            real_vector._semantic_family_handle = object()
            real_vector._semantic_arrow_handle = object()
            assert not hasattr(real_vector, "_scene")
            leaf = object.__new__(Mobject)
            leaf._scene, leaf._semantic_handle = None, object()
            growth = object.__new__(_manim_growing.GrowArrow)
            growth.mobject = real_vector
            with patch.object(_manim_compat, "_leaf_mobjects", return_value=[leaf]):
                admitted = _manim_scene._canonical_arrow_grow_animation(Scene(), growth)
                assert admitted == (real_vector, real_vector._semantic_arrow_handle, [leaf])
                leaf._scene = Scene()
                try:
                    _manim_scene._canonical_arrow_grow_animation(Scene(), growth)
                    raise AssertionError("attached Arrow leaf was admitted")
                except NotImplementedError:
                    pass

            class Arrow:
                def get_start(self):
                    from noon import Vec2
                    return Vec2(0.0, 0.0)
            class Vector(Arrow):
                def __init__(self, vector, color):
                    self.vector, self.color = vector, color
            _manim_arrow.Arrow = Arrow
            _manim_arrow.Vector = Vector

            scene = VectorScene()
            plays, additions = [], []
            scene.play = lambda *animations, **kwargs: plays.extend(animations)
            scene.add = lambda *objects: additions.extend(objects)

            vector = scene.add_vector((2.0, 1.0))
            assert isinstance(vector, Vector)
            assert len(plays) == 1 and plays[0].mobject is vector
            assert not hasattr(plays[0], "point")
            assert additions == []

            static = scene.add_vector((1.0, 0.0), animate=False)
            assert additions == [static]
            assert len(plays) == 1
            """
        )
        completed = subprocess.run(
            [sys.executable, "-c", source], cwd=python_dir, env=env,
            capture_output=True, text=True, check=False,
        )
        self.assertEqual(
            completed.returncode, 0,
            msg=f"stdout:\n{completed.stdout}\nstderr:\n{completed.stderr}",
        )

    def test_lts_tracks_animated_vector_after_async_growth_completes(self) -> None:
        python_dir = Path(__file__).resolve().parent
        env = os.environ.copy()
        env["PYTHONPATH"] = os.pathsep.join(
            value for value in (str(python_dir), env.get("PYTHONPATH")) if value
        )
        source = textwrap.dedent(
            """
            import asyncio
            import _manim_arrow
            from noon import LinearTransformationScene, Vec2

            class Arrow:
                def get_start(self): return Vec2(0.0, 0.0)
            class Vector(Arrow):
                def __init__(self, vector, color): self.vector, self.color = vector, color
            _manim_arrow.Arrow = Arrow
            _manim_arrow.Vector = Vector

            async def main():
                scene = LinearTransformationScene(
                    include_background_plane=False,
                    include_foreground_plane=False,
                    show_basis_vectors=False,
                )
                completed = []
                async def play(*animations, **kwargs):
                    await asyncio.sleep(0)
                    completed.append(animations[0].mobject)
                scene.play = play
                pending = scene.add_vector((2.0, 1.0), animate=True)
                assert hasattr(pending, "__await__")
                assert scene.moving_vectors == [] and completed == []
                vector = await pending
                assert completed == [vector]
                assert scene.moving_vectors == [vector]

            asyncio.run(main())
            """
        )
        completed = subprocess.run(
            [sys.executable, "-c", source], cwd=python_dir, env=env,
            capture_output=True, text=True, check=False,
        )
        self.assertEqual(
            completed.returncode, 0,
            msg=f"stdout:\n{completed.stdout}\nstderr:\n{completed.stderr}",
        )

    def test_lts_accepts_coordinate_labels_and_ghost_vectors(self) -> None:
        python_dir = Path(__file__).resolve().parent
        env = os.environ.copy()
        env["PYTHONPATH"] = os.pathsep.join(
            value for value in (str(python_dir), env.get("PYTHONPATH")) if value
        )
        source = textwrap.dedent(
            """
            from noon import LinearTransformationScene, VectorScene

            scene = LinearTransformationScene()
            assert isinstance(scene, VectorScene)
            assert scene.include_background_plane
            assert scene.include_foreground_plane
            assert scene.show_basis_vectors
            assert scene.moving_vectors == []
            assert not scene.show_coordinates and not scene.leave_ghost_vectors
            enabled = LinearTransformationScene(show_coordinates=True, leave_ghost_vectors=True)
            assert enabled.show_coordinates and enabled.leave_ghost_vectors
            """
        )
        completed = subprocess.run(
            [sys.executable, "-c", source], cwd=python_dir, env=env,
            capture_output=True, text=True, check=False,
        )
        self.assertEqual(
            completed.returncode, 0,
            msg=f"stdout:\n{completed.stdout}\nstderr:\n{completed.stderr}",
        )

    def test_number_plane_coordinates_delegate_to_atomic_rust_label_seam(self) -> None:
        python_dir = Path(__file__).resolve().parent
        env = os.environ.copy()
        env["PYTHONPATH"] = os.pathsep.join(
            value for value in (str(python_dir), env.get("PYTHONPATH")) if value
        )
        source = textwrap.dedent(
            """
            import _manim_number_labels
            from _manim_number_plane import NumberPlane
            from noon import DR, RIGHT, SMALL_BUFF

            calls = []
            def add_labels(plane, x, y, **kwargs):
                calls.append((plane, x, y, kwargs))
                return plane
            _manim_number_labels.add_number_plane_coordinates = add_labels
            plane = object.__new__(NumberPlane)
            assert NumberPlane.add_coordinates(plane, (1, 2), (3,), decimal_places=1) is plane
            assert calls == [(plane, (1, 2), (3,), {
                "x_config": {"direction": DR}, "y_config": {"direction": DR},
                "config": {"decimal_places": 1, "font_size": 24, "buff": SMALL_BUFF},
            })]
            NumberPlane.add_coordinates(plane, direction=RIGHT, font_size=30, buff=0.2,
                                        y_config={"font_size": 12})
            assert calls[-1] == (plane, None, None, {
                "x_config": {}, "y_config": {"font_size": 12},
                "config": {"direction": RIGHT, "font_size": 30, "buff": 0.2},
            })
            """
        )
        completed = subprocess.run(
            [sys.executable, "-c", source], cwd=python_dir, env=env,
            capture_output=True, text=True, check=False,
        )
        self.assertEqual(
            completed.returncode, 0,
            msg=f"stdout:\n{completed.stdout}\nstderr:\n{completed.stderr}",
        )

    def test_number_plane_label_helper_uses_one_shared_rust_publication(self) -> None:
        python_dir = Path(__file__).resolve().parent
        env = os.environ.copy()
        env["PYTHONPATH"] = os.pathsep.join(
            value for value in (str(python_dir), env.get("PYTHONPATH")) if value
        )
        source = textwrap.dedent(
            """
            import _manim_number_labels as labels
            class Options:
                def __init__(self, name): self.name, self.freed = name, False
                def free(self): self.freed = True
            x_axis, y_axis = object(), object()
            class Handle:
                def numberPlaneCoordinateLabelFamilies(self, *args):
                    self.args = args
                    return ("x-family", "y-family")
            class Plane:
                x_axis, y_axis = x_axis, y_axis
                _semantic_family_handle = Handle()
            plane = Plane()

            labels._cold_labels = lambda: None
            labels._plot._array = lambda values: tuple(values)
            options = []
            def make_options(config):
                token = Options(len(options))
                options.append((token, config))
                return token, 18.0, "white"
            labels._options = make_options
            families, remembered = [], []
            labels._family = lambda handle, size, color: (handle, size, color)
            labels._remember = lambda axis, family: remembered.append((axis, family))

            result = labels.add_number_plane_coordinates(
                plane, (1, 2), (3,), x_config={"decimal_places": 1},
                y_config={"decimal_places": 2}, config={"font_size": 17, "font": "Fixture Sans"},
            )
            assert result is plane
            assert options[0][1]["direction"] is labels._base.DOWN
            assert options[1][1]["direction"] is labels._base.LEFT
            assert [entry[1]["decimal_places"] for entry in options] == [1, 2]
            assert plane._semantic_family_handle.args[:4] == ((1, 2), False, (3,), False)
            assert plane._semantic_family_handle.args[4:] == (options[0][0], options[1][0])
            assert len(remembered) == 2
            assert remembered[0][0] is x_axis and remembered[1][0] is y_axis
            """
        )
        completed = subprocess.run(
            [sys.executable, "-c", source], cwd=python_dir, env=env,
            capture_output=True, text=True, check=False,
        )
        self.assertEqual(
            completed.returncode, 0,
            msg=f"stdout:\n{completed.stdout}\nstderr:\n{completed.stderr}",
        )

    def test_lts_transformable_enrollment_deduplicates_and_classifies_arrows(self) -> None:
        python_dir = Path(__file__).resolve().parent
        env = os.environ.copy()
        env["PYTHONPATH"] = os.pathsep.join(
            value for value in (str(python_dir), env.get("PYTHONPATH")) if value
        )
        source = textwrap.dedent(
            """
            import _manim_arrow
            import _manim_semantic_handles
            from noon import LinearTransformationScene

            class Arrow: pass
            _manim_arrow.Arrow = Arrow
            _manim_semantic_handles._family_wrapper_key = lambda item: item.key
            scene = LinearTransformationScene(
                include_background_plane=False,
                include_foreground_plane=False,
                show_basis_vectors=False,
            )
            additions = []
            scene.add = lambda *objects: additions.extend(objects)
            ordinary, arrow = type("M", (), {"key": "ordinary"})(), Arrow()
            arrow.key = "arrow"
            scene.add_transformable_mobject(ordinary, ordinary, arrow, arrow)
            assert additions == [ordinary, arrow]
            assert scene.transformable_mobjects == [ordinary]
            assert scene.moving_vectors == [arrow]
            assert not hasattr(scene, "_transformable_keys")
            """
        )
        completed = subprocess.run(
            [sys.executable, "-c", source], cwd=python_dir, env=env,
            capture_output=True, text=True, check=False,
        )
        self.assertEqual(
            completed.returncode, 0,
            msg=f"stdout:\n{completed.stdout}\nstderr:\n{completed.stderr}",
        )

    def test_lts_matrix_transforms_registered_families_and_adds_ghost_after_validation(self) -> None:
        python_dir = Path(__file__).resolve().parent
        env = os.environ.copy()
        env["PYTHONPATH"] = os.pathsep.join(
            value for value in (str(python_dir), env.get("PYTHONPATH")) if value
        )
        source = textwrap.dedent(
            """
            import sys, types
            fake_js = types.ModuleType("js")
            fake_js.noonLinearTransformationPathArc = lambda *args: 0.0
            fake_js.noonResolveAnimationOptions = lambda *args: None
            sys.modules["js"] = fake_js

            import _manim_animate, _manim_arrow, _manim_compat, _manim_composition, _manim_scene
            from noon import LinearTransformationScene

            class Animation:
                def __init__(self, *args, **kwargs): self.args, self.kwargs = args, kwargs
            _manim_animate.ApplyMatrix = Animation
            _manim_animate.Transform = Animation
            class Add:
                def __init__(self, mobject, run_time=0.0):
                    self.mobject, self.run_time = mobject, run_time
            class AnimationGroup:
                def __init__(self, *animations, **kwargs):
                    self.animations, self.kwargs = animations, kwargs
            _manim_composition.Add = Add
            _manim_composition.AnimationGroup = AnimationGroup
            class Arrow: pass
            _manim_arrow.Arrow = Arrow

            edits = []
            def apply_target(target, animation):
                edits.append((target, animation))
            _manim_scene._apply_matrix_target = apply_target

            class Family:
                def __init__(self, *members): self.members = members
                def copy(self): return Ghost(tuple(GhostLeaf(member) for member in self.members))
            class GhostLeaf:
                def __init__(self, source): self.source = source
            class Ghost:
                def __init__(self, members): self.members, self.opacity = members, 1.0
                def fade(self, darkness): self.opacity *= 1.0 - darkness; return self
            _manim_compat.Group = Family
            _manim_compat._leaf_mobjects = lambda value: list(value.members)

            class Transformable:
                def _copy_for_animate_target(self): return object()
            mobject = Transformable()
            class Arrow:
                def __init__(self, *args, **kwargs): self.args, self.kwargs = args, kwargs
                _semantic_arrow_handle = type("Handle", (), {
                    "matrixTransformedEndpoints": lambda self, *args: (0.0, 0.0, 1.0, 1.0)
                })()
                def get_color(self): return "yellow"
                def get_stroke_width(self): return 2.0
            _manim_arrow.Arrow = Arrow
            vector = Arrow()
            scene = LinearTransformationScene(
                include_background_plane=False,
                include_foreground_plane=False,
                show_basis_vectors=False,
                leave_ghost_vectors=True,
            )
            scene.transformable_mobjects = [mobject]
            scene.moving_vectors = [vector]
            additions, plays = [], []
            scene.add = lambda *items: additions.extend(items)
            scene.play = lambda *animations, **kwargs: plays.append((animations, kwargs))

            scene.apply_matrix([[0, 1], [1, 0]])
            assert edits[0][1].args[1] is mobject
            assert additions == [], "ghosts must not be added before play validates the composition"
            assert len(plays) == 1 and len(plays[0][0]) == 1
            assert plays[0][1] == {"run_time": 3.0}
            composition, = plays[0][0]
            assert isinstance(composition, AnimationGroup)
            assert len(composition.animations) == 2
            ghost_add, transform = composition.animations
            assert isinstance(ghost_add, Add) and ghost_add.mobject.source is vector
            assert ghost_add.run_time == 0.0, "ghost introduction is an instantaneous boundary"
            assert isinstance(transform, Animation)
            source, target = transform.args[:2]
            assert source.members[0] is mobject and source.members[1].members == (vector,)
            assert len(target.members) == 2

            generic_only = LinearTransformationScene(
                include_background_plane=False,
                include_foreground_plane=False,
                show_basis_vectors=False,
                leave_ghost_vectors=True,
            )
            generic_only.transformable_mobjects = [mobject]
            generic_only.moving_vectors = []
            generic_only.add = lambda *items: additions.extend(items)
            generic_plays = []
            generic_only.play = lambda *animations, **kwargs: generic_plays.append((animations, kwargs))
            generic_only.apply_matrix([[0, 1], [1, 0]])
            assert additions == [], "generic ApplyMatrix objects do not use get_piece_movement ghosts"
            assert isinstance(generic_plays[0][0][0], Animation)

            def reject_target(target, animation): raise RuntimeError("unsupported target")
            _manim_scene._apply_matrix_target = reject_target
            try:
                scene.apply_matrix([[0, 1], [1, 0]])
            except RuntimeError as error:
                assert "unsupported target" in str(error)
            else:
                raise AssertionError("unsupported transformable was accepted")
            assert additions == [], "failed target preparation must not add a ghost"
            _manim_scene._apply_matrix_target = apply_target
            def reject_play(*animations, **kwargs):
                raise ValueError("unsupported play option")
            scene.play = reject_play
            try:
                scene.apply_matrix([[0, 1], [1, 0]])
            except ValueError as error:
                assert "unsupported play option" in str(error)
            else:
                raise AssertionError("unsupported play was accepted")
            assert additions == [], "rejected composition must not leave visible ghosts"
            """
        )
        completed = subprocess.run(
            [sys.executable, "-c", source], cwd=python_dir, env=env,
            capture_output=True, text=True, check=False,
        )
        self.assertEqual(
            completed.returncode, 0,
            msg=f"stdout:\n{completed.stdout}\nstderr:\n{completed.stderr}",
        )

    def test_group_fade_uses_shared_rust_family_operation(self) -> None:
        python_dir = Path(__file__).resolve().parent
        env = os.environ.copy()
        env["PYTHONPATH"] = os.pathsep.join(
            value for value in (str(python_dir), env.get("PYTHONPATH")) if value
        )
        source = textwrap.dedent(
            """
            import _manim_compat, _manim_semantic_handles
            calls = []
            class Handle:
                def fade(self, darkness): calls.append(darkness)
                def memberKeys(self): return []
            group = object.__new__(_manim_compat.Group)
            group._semantic_family_handle = Handle()
            _manim_semantic_handles.engine_call = lambda function, *args, **kwargs: function(*args)
            assert group.fade(0.7) is group
            assert calls == [0.7]
            try:
                group.fade(1.1)
            except ValueError:
                pass
            else:
                raise AssertionError("out-of-range darkness was accepted")
            assert calls == [0.7]
            """
        )
        completed = subprocess.run(
            [sys.executable, "-c", source], cwd=python_dir, env=env,
            capture_output=True, text=True, check=False,
        )
        self.assertEqual(
            completed.returncode, 0,
            msg=f"stdout:\n{completed.stdout}\nstderr:\n{completed.stderr}",
        )

    def test_lts_apply_matrix_uses_rust_path_arc_and_manim_three_second_default(self) -> None:
        python_dir = Path(__file__).resolve().parent
        env = os.environ.copy()
        env["PYTHONPATH"] = os.pathsep.join(
            value for value in (str(python_dir), env.get("PYTHONPATH")) if value
        )
        source = textwrap.dedent(
            """
            import sys
            import types

            calls = []
            fake_js = types.ModuleType("js")
            def path_arc(values, rows, columns):
                calls.append((list(values), rows, columns))
                return 0.0
            fake_js.noonLinearTransformationPathArc = path_arc
            fake_js.noonResolveAnimationOptions = lambda *args: None
            sys.modules["js"] = fake_js

            from noon import LinearTransformationScene
            import _manim_compat
            import _manim_animate
            import _manim_arrow
            import _manim_scene

            class Shaft:
                def get_stroke_width(self):
                    return 8.5

            source_arrow_type = _manim_arrow.Arrow
            source_arrow = object.__new__(source_arrow_type)
            source_arrow._shaft = Shaft()
            assert source_arrow.get_stroke_width() == 8.5

            class Aggregate:
                def matrixTransformedEndpoints(self, values, rows, columns, x, y):
                    return (0.0, 0.0, 1.0, 1.0)

            source_arrow._semantic_arrow_handle = Aggregate()
            source_arrow.get_color = lambda: "yellow"

            target_arrows = []
            class TargetArrow:
                def __init__(self, *args, **kwargs):
                    self.args = args
                    self.kwargs = kwargs
                    target_arrows.append(self)

            _manim_arrow.Arrow = TargetArrow

            class Family:
                def __init__(self, *members):
                    self.members = members
            _manim_compat.Group = Family

            class Plane:
                def _copy_for_animate_target(self):
                    return PlaneTarget()

            class PlaneTarget:
                pass

            matrix_edits = []
            _manim_scene._apply_matrix_target = lambda target, animation: matrix_edits.append(
                (target, animation)
            )

            class Animation:
                def __init__(self, *args, **kwargs):
                    self.args = args
                    self.anim_args = kwargs

            _manim_animate.ApplyMatrix = Animation
            _manim_animate.Transform = Animation

            scene = LinearTransformationScene()
            scene.foreground_plane = Plane()
            scene.moving_vectors = [source_arrow]
            captured = {}
            scene.play = lambda *animations, **kwargs: captured.update(
                animations=animations, kwargs=kwargs
            )
            scene.apply_matrix([[0.0, 1.0], [1.0, 0.0]])
            assert calls == [([0.0, 1.0, 1.0, 0.0], 2, 2)]
            assert captured["kwargs"] == {"run_time": 3.0}
            assert len(captured["animations"]) == 1
            combined_animation, = captured["animations"]
            assert combined_animation.anim_args == {"path_arc": 0.0, "run_time": 3.0}
            source_family, target_family = combined_animation.args
            assert len(source_family.members) == len(target_family.members) == 2
            assert source_family.members[0] is scene.foreground_plane
            assert isinstance(target_family.members[0], PlaneTarget)
            assert len(source_family.members[1].members) == 1
            assert len(target_family.members[1].members) == 1
            assert matrix_edits[0][0] is target_family.members[0]
            assert matrix_edits[0][1].args[0] == [[0.0, 1.0], [1.0, 0.0]]
            assert target_arrows[0].args == ((0.0, 0.0), (1.0, 1.0))
            assert target_arrows[0].kwargs["stroke_width"] == 8.5
            # The flattened length is four, but these are not two valid rows.
            # Reject before querying Rust or preparing any target objects.
            try:
                scene.apply_matrix([[1.0, 2.0, 3.0], [4.0]])
            except ValueError as error:
                assert "equal lengths" in str(error)
            else:
                raise AssertionError("ragged matrix was silently reinterpreted")
            assert calls == [([0.0, 1.0, 1.0, 0.0], 2, 2)]
            """
        )
        completed = subprocess.run(
            [sys.executable, "-c", source], cwd=python_dir, env=env,
            capture_output=True, text=True, check=False,
        )
        self.assertEqual(
            completed.returncode, 0,
            msg=f"stdout:\n{completed.stdout}\nstderr:\n{completed.stderr}",
        )


if __name__ == "__main__":
    unittest.main()
