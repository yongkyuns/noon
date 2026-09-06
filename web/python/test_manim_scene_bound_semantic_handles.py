import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimSceneBoundSemanticHandleTests(unittest.TestCase):
    def test_bound_deterministic_state_stays_in_shared_handle(self) -> None:
        python_dir = Path(__file__).resolve().parent
        env = os.environ.copy()
        existing = env.get("PYTHONPATH")
        env["PYTHONPATH"] = str(python_dir) if not existing else os.pathsep.join((str(python_dir), existing))
        source = textwrap.dedent(
            r'''
            import copy
            import json
            import math
            import sys
            import types

            fake_js = types.ModuleType("js")

            class Result:
                ok = True
                runTime = 1.0
                rateFunc = "linear"
                lagRatio = 0.0
                pathArc = 0.0
                reverseRateFunction = False
                errorKind = ""
                message = ""

            fake_js.noonResolveAnimationOptions = lambda *args: Result()
            fake_js.noonResolveUniformCompositionSchedule = lambda *args: None

            class FakeHandle:
                def __init__(self, snapshot_json):
                    self.snapshot = json.loads(snapshot_json)
                    self.calls = []
                    self.snapshot_requests = 0

                def snapshotJson(self):
                    self.snapshot_requests += 1
                    return json.dumps(self.snapshot, separators=(",", ":"))

                def replaceSnapshotJson(self, snapshot_json):
                    self.calls.append("replaceSnapshotJson")
                    self.snapshot = json.loads(snapshot_json)

                def cloneHandle(self):
                    self.calls.append("cloneHandle")
                    clone = FakeHandle(json.dumps(self.snapshot))
                    clone.calls.append("cloneHandle")
                    return clone

                def targetEditor(self):
                    self.calls.append("targetEditor")
                    target = FakeHandle(json.dumps(self.snapshot))
                    target.calls.append("targetEditor")
                    return target

                def becomeHandle(self, other):
                    self.calls.append("becomeHandle")
                    self.snapshot = copy.deepcopy(other.snapshot)

                @property
                def centerX(self):
                    return float(self.snapshot["transform"]["translation"]["x"])

                @property
                def centerY(self):
                    return float(self.snapshot["transform"]["translation"]["y"])

                @property
                def width(self):
                    geometry = self.snapshot["geometry"]
                    if "rectangle" in geometry:
                        base = float(geometry["rectangle"]["size"]["x"])
                    else:
                        base = 2.0 * float(geometry["circle"]["radius"])
                    return abs(base * float(self.snapshot["transform"]["scale"]["x"]))

                @property
                def height(self):
                    geometry = self.snapshot["geometry"]
                    if "rectangle" in geometry:
                        base = float(geometry["rectangle"]["size"]["y"])
                    else:
                        base = 2.0 * float(geometry["circle"]["radius"])
                    return abs(base * float(self.snapshot["transform"]["scale"]["y"]))

                def criticalX(self, direction_x, direction_y):
                    if direction_x < 0:
                        return self.centerX - self.width / 2
                    if direction_x > 0:
                        return self.centerX + self.width / 2
                    return self.centerX

                def criticalY(self, direction_x, direction_y):
                    if direction_y < 0:
                        return self.centerY - self.height / 2
                    if direction_y > 0:
                        return self.centerY + self.height / 2
                    return self.centerY

                def shift(self, x, y):
                    self.calls.append(("shift", float(x), float(y)))
                    t = self.snapshot["transform"]["translation"]
                    t["x"] += float(x)
                    t["y"] += float(y)

                def scale(self, x, y):
                    self.calls.append(("scale", float(x), float(y)))
                    scale = self.snapshot["transform"]["scale"]
                    scale["x"] *= float(x)
                    scale["y"] *= float(y)

                def rotateAboutPoint(self, angle, point_x, point_y):
                    self.calls.append(("rotateAboutPoint", float(angle)))
                    t = self.snapshot["transform"]["translation"]
                    dx = t["x"] - float(point_x)
                    dy = t["y"] - float(point_y)
                    c = math.cos(float(angle))
                    s = math.sin(float(angle))
                    t["x"] = float(point_x) + dx * c - dy * s
                    t["y"] = float(point_y) + dx * s + dy * c
                    self.snapshot["transform"]["rotation"] += float(angle)

                def setFillOpacity(self, opacity):
                    fill = self.snapshot["style"]["fill"]
                    if fill is None:
                        fill = {"red": 1.0, "green": 1.0, "blue": 1.0, "alpha": 1.0}
                        self.snapshot["style"]["fill"] = fill
                    fill["alpha"] = float(opacity)

                def setStrokeOpacity(self, opacity):
                    stroke = self.snapshot["style"]["stroke"]
                    if stroke is None:
                        stroke = {"red": 1.0, "green": 1.0, "blue": 1.0, "alpha": 1.0}
                        self.snapshot["style"]["stroke"] = stroke
                    stroke["alpha"] = float(opacity)

                def setFillColor(self, red, green, blue, alpha):
                    fill = self.snapshot["style"]["fill"]
                    current_alpha = float(alpha) if fill is None else float(fill["alpha"])
                    self.snapshot["style"]["fill"] = {
                        "red": float(red), "green": float(green), "blue": float(blue), "alpha": current_alpha
                    }

                def setStrokeColor(self, red, green, blue, alpha):
                    stroke = self.snapshot["style"]["stroke"]
                    current_alpha = float(alpha) if stroke is None else float(stroke["alpha"])
                    self.snapshot["style"]["stroke"] = {
                        "red": float(red), "green": float(green), "blue": float(blue), "alpha": current_alpha
                    }

                def setFill(self, red, green, blue, opacity):
                    self.snapshot["style"]["fill"] = {
                        "red": float(red), "green": float(green), "blue": float(blue), "alpha": float(opacity)
                    }

                def setStrokeWidth(self, width):
                    self.snapshot["style"]["stroke_width"] = float(width)

                def setOpacity(self, opacity):
                    for name in ("fill", "stroke"):
                        if self.snapshot["style"][name] is not None:
                            self.snapshot["style"][name]["alpha"] = float(opacity)

                def setObjectOpacity(self, opacity):
                    self.calls.append(("setObjectOpacity", float(opacity)))
                    self.snapshot["style"]["opacity"] = float(opacity)

                def disableFill(self):
                    self.snapshot["style"]["fill"] = None

                def disableStroke(self):
                    self.snapshot["style"]["stroke"] = None

                @property
                def fillOpacity(self):
                    fill = self.snapshot["style"]["fill"]
                    return 0.0 if fill is None else float(fill["alpha"])

                @property
                def strokeOpacity(self):
                    stroke = self.snapshot["style"]["stroke"]
                    return 0.0 if stroke is None else float(stroke["alpha"])

                @property
                def wireTranslationX(self): return float(self.snapshot["transform"]["translation"]["x"])
                @property
                def wireTranslationY(self): return float(self.snapshot["transform"]["translation"]["y"])
                @property
                def wireScaleX(self): return float(self.snapshot["transform"]["scale"]["x"])
                @property
                def wireScaleY(self): return float(self.snapshot["transform"]["scale"]["y"])
                @property
                def wireRotation(self): return float(self.snapshot["transform"]["rotation"])
                @property
                def wireHasFill(self): return self.snapshot["style"]["fill"] is not None
                @property
                def wireFillRed(self): return 0.0 if not self.wireHasFill else float(self.snapshot["style"]["fill"]["red"])
                @property
                def wireFillGreen(self): return 0.0 if not self.wireHasFill else float(self.snapshot["style"]["fill"]["green"])
                @property
                def wireFillBlue(self): return 0.0 if not self.wireHasFill else float(self.snapshot["style"]["fill"]["blue"])
                @property
                def wireFillAlpha(self): return 0.0 if not self.wireHasFill else float(self.snapshot["style"]["fill"]["alpha"])
                @property
                def wireHasStroke(self): return self.snapshot["style"]["stroke"] is not None
                @property
                def wireStrokeRed(self): return 0.0 if not self.wireHasStroke else float(self.snapshot["style"]["stroke"]["red"])
                @property
                def wireStrokeGreen(self): return 0.0 if not self.wireHasStroke else float(self.snapshot["style"]["stroke"]["green"])
                @property
                def wireStrokeBlue(self): return 0.0 if not self.wireHasStroke else float(self.snapshot["style"]["stroke"]["blue"])
                @property
                def wireStrokeAlpha(self): return 0.0 if not self.wireHasStroke else float(self.snapshot["style"]["stroke"]["alpha"])
                @property
                def wireStrokeWidth(self): return float(self.snapshot["style"]["stroke_width"])
                @property
                def wireObjectOpacity(self): return float(self.snapshot["style"]["opacity"])

            fake_js.noonCreateAuthoringMobjectHandle = FakeHandle
            sys.modules["js"] = fake_js

            import _manim_compat
            _manim_compat.install()
            import _manim_rate_functions
            _manim_rate_functions.install()
            import _manim_phase_b  # noqa: F401
            import _manim_semantic_handles as handles
            handles._create_handle = FakeHandle
            handles.install()
            import _manim_animate as animate

            from noon import BLUE, GREEN, RIGHT, Scene, Square

            square = Square(color=BLUE, fill_opacity=0.8)
            handle = square._semantic_handle
            scene = Scene()
            scene.add(square)
            handle.snapshot_requests = 0
            handle.calls.clear()

            # Binding does not discard the stable semantic identity. A direct static
            # mutation executes in the shared handle and mirrors only typed wire fields.
            square.shift(RIGHT)
            assert handle.calls == [("shift", 1.0, 0.0)], handle.calls
            assert handle.snapshot_requests == 0
            stored = scene._objects[square.id]
            assert stored["transform"]["translation"] == {"x": 1.0, "y": 0.0}
            assert square.get_center().x == 1.0

            class EffectiveLayout:
                centerX = 2.5
                centerY = -1.5
                width = 6.0
                height = 4.0

            class CanonicalContext:
                def __init__(self):
                    self.transferred = False
                    self.queries = []

                def queryMobjectLayout(self, queried):
                    self.queries.append(queried)
                    if self.transferred:
                        raise RuntimeError("live execution session is running in the semantic engine")
                    return EffectiveLayout()

            context = CanonicalContext()
            scene._canonical_authoring_context = context
            assert square.get_center() == (2.5, -1.5)
            assert square.width == 6.0
            assert square.height == 4.0
            assert context.queries == [handle, handle, handle]

            # Merely registering an updater does not make its last callback frame
            # authoritative outside an active phase. Ordinary reads still query
            # the canonical runtime through the fresh raw semantic handle.
            square._noon_updaters = [lambda mobject: mobject]
            context.queries.clear()
            assert handles._handle_for(square) is None
            assert square.get_center() == (2.5, -1.5)
            assert square.width == 6.0
            assert square.height == 4.0
            assert context.queries == [handle, handle, handle]

            # The explicit legacy materialization boundary keeps its existing raw
            # fallback rather than consulting an unrelated canonical runtime.
            scene._legacy_geometry_materialized = True
            assert square.get_center() == (1.0, 0.0)
            del scene._legacy_geometry_materialized
            del square._noon_updaters

            context.transferred = True
            try:
                square.get_center()
            except RuntimeError as error:
                assert "running in the semantic engine" in str(error)
            else:
                raise AssertionError("transferred live layout read returned stale authored state")
            # A returned player remains the shared mutation authority between
            # continuation awaits. Neither the source nor a new target may edit
            # its handle behind the runtime's publication revision.
            context.transferred = False
            context.liveExecutionOwnership = lambda: "returned"
            live_calls = []
            context.liveShift = lambda target, x, y: live_calls.append((target, x, y))
            context.liveTargetEditor = lambda target: FakeHandle(json.dumps(target.snapshot))
            square.shift(RIGHT)
            target = square.copy()
            target.shift(RIGHT)
            assert live_calls == [(handle, 1.0, 0.0), (target._semantic_handle, 1.0, 0.0)]
            assert target._canonical_live_target_context is context
            assert handle.centerX == 1.0, "returned edits must not bypass the live session"
            del scene._canonical_authoring_context

            square.set_fill(GREEN, opacity=0.25)
            assert handle.snapshot_requests == 0
            assert abs(stored["style"]["fill"]["alpha"] - 0.25) < 1e-12
            square.set_object_opacity(0.4)
            assert handle.calls[-1] == ("setObjectOpacity", 0.4)
            assert handle.snapshot_requests == 0
            assert abs(stored["style"]["opacity"] - 0.4) < 1e-12

            first = animate._AlignedAnimationBuilder(square)
            first_target = first.target
            assert handle.calls[-1] == "targetEditor"
            assert handle.snapshot_requests == 0
            first.shift(RIGHT)
            scene.play(first, run_time=1.0)
            assert handle.calls[-1] == "becomeHandle", handle.calls
            assert handle.snapshot_requests == 0
            assert square.get_center().x == 2.0

            # The second builder starts directly from the committed shared final state;
            # no evaluated Python snapshot or source JSON seed is needed.
            before = handle.snapshot_requests
            second = animate._AlignedAnimationBuilder(square)
            second_target = second.target
            assert handle.calls[-1] == "targetEditor"
            assert handle.snapshot_requests == before == 0
            assert second_target.get_center().x == 2.0
            second.scale(0.5)
            assert second_target.get_center().x == 2.0

            # Updater metadata can be attached before Scene.add. While detached there is
            # no runtime callback snapshot yet, so the shared handle must remain available
            # to materialize the initial low-level Mobject for binding.
            detached_updater = Square()
            detached_updater._noon_updaters = [lambda mob: mob]
            detached_handle = detached_updater._semantic_handle
            assert handles._handle_for(detached_updater) is detached_handle
            assert isinstance(detached_updater._current_raw(), handles._ir.Mobject)
            updater_scene = Scene()
            updater_scene.add(detached_updater)
            assert detached_updater._scene is updater_scene

            # Once bound, host-dynamic state deliberately opts out until runtime evaluated
            # handles are shared. This is a correctness fallback, not a second deterministic path.
            assert handles._handle_for(detached_updater) is None
            square._noon_updaters = []
            assert handles._handle_for(square) is None
            '''
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
            "scene-bound semantic-handle subprocess failed:\n"
            f"stdout:\n{completed.stdout}\nstderr:\n{completed.stderr}",
        )


if __name__ == "__main__":
    unittest.main()
