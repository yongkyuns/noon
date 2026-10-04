"""Check simple lesson storyboards from Python syntax, not a substitute renderer."""
import ast
from decimal import Decimal
import json
from pathlib import Path
import unittest

WEB = Path(__file__).resolve().parents[1]
LESSONS = {
    "showcase-entrances-exits",
    "showcase-transform-ownership",
    "showcase-bezier-paths",
    "showcase-reactive-relationships",
    "showcase-always-redraw",
    "showcase-camera-follows-path",
}


def literal_timeline(source):
    """Read only straight-line, explicitly timed play/wait calls; reject ambiguity."""
    tree = ast.parse(source)
    scenes = [node for node in tree.body if isinstance(node, ast.ClassDef)]
    if len(scenes) != 1:
        raise ValueError("expected one lesson Scene")
    construct = next(node for node in scenes[0].body
                     if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)) and node.name == "construct")

    def timed_call(node):
        return (
            isinstance(node, ast.Call) and isinstance(node.func, ast.Attribute)
            and isinstance(node.func.value, ast.Name) and node.func.value.id == "self"
            and node.func.attr in {"play", "wait", "move_camera"}
        )

    direct = []
    for node in construct.body:
        if not isinstance(node, ast.Expr):
            continue
        value = node.value.value if isinstance(node.value, ast.Await) else node.value
        if timed_call(value):
            direct.append(value)
    if len(direct) != sum(timed_call(node) for node in ast.walk(construct)):
        raise ValueError("nested timed calls need an explicit storyboard instead")
    clock, holds, transitions = Decimal(0), [], []
    for call in direct:
        if call.func.attr in {"play", "move_camera"}:
            values = [kw.value for kw in call.keywords if kw.arg == "run_time"]
        else:
            values = call.args
        if len(values) != 1 or not isinstance(values[0], ast.Constant):
            raise ValueError("every play/wait must have one literal duration")
        value = values[0].value
        if isinstance(value, bool) or not isinstance(value, (int, float)) or value <= 0:
            raise ValueError("duration must be positive")
        end = clock + Decimal(str(value))
        (holds if call.func.attr == "wait" else transitions).append([clock, end])
        clock = end
    if not transitions:
        raise ValueError("a storyboard needs animation")
    return clock, holds, transitions


class FeatureLessonStoryboards(unittest.TestCase):
    def test_storyboards_match_source_durations_and_actual_waits(self):
        manifest = json.loads((WEB / "python/examples/noon_showcase_manifest.json").read_text())
        selected = [entry for entry in manifest["entries"] if entry["id"] in LESSONS]
        self.assertEqual({entry["id"] for entry in selected}, LESSONS)
        for entry in selected:
            with self.subTest(lesson=entry["id"]):
                source = (WEB / entry["path"]).read_text()
                compile(source, entry["path"], "exec")
                tree = ast.parse(source)
                self.assertFalse(any(isinstance(node, ast.Assert) for node in ast.walk(tree)))
                duration, holds, transitions = literal_timeline(source)
                self.assertEqual(duration, Decimal(str(entry["duration"])))
                self.assertEqual(holds, [[Decimal(str(x)) for x in interval] for interval in entry["still_intervals"]])
                required_transitions = 3 if entry["id"] == "showcase-spatial-scene" else 4
                self.assertGreaterEqual(len(transitions), required_transitions)
                self.assertGreaterEqual(holds[-1][1] - holds[-1][0], Decimal("1.0"))
                self.assertTrue(any(start < Decimal(str(entry["thumbnail_time"])) <= end for start, end in holds)
                                or any(Decimal(str(entry["thumbnail_time"])) == start for start, _ in holds))
                for beat in entry["beats"]:
                    self.assertTrue(Decimal(0) < Decimal(str(beat["time"])) <= duration)

    def test_spatial_showcase_models_camera_moves_as_timed_animation(self):
        manifest = json.loads((WEB / "python/examples/noon_showcase_manifest.json").read_text())
        entry = next(item for item in manifest["entries"] if item["id"] == "showcase-spatial-scene")
        source = (WEB / entry["path"]).read_text()
        tree = ast.parse(source, filename=entry["path"])
        scene = next(node for node in tree.body if isinstance(node, ast.ClassDef))
        self.assertIn("ThreeDScene", [base.id for base in scene.bases if isinstance(base, ast.Name)])
        duration, holds, transitions = literal_timeline(source)
        self.assertEqual(duration, Decimal("8.0"))
        requested_stills = [[Decimal(str(value)) for value in interval]
                            for interval in entry["still_intervals"]]
        self.assertEqual(requested_stills, [
            [Decimal("1.85"), Decimal("2.2")],
            [Decimal("3.8"), Decimal("4.1")],
            [Decimal("6.0"), Decimal("8.0")],
        ])
        self.assertTrue(all(any(start <= still[0] <= still[1] <= end for start, end in holds)
                            for still in requested_stills))
        self.assertEqual(transitions, [
            [Decimal("0"), Decimal("1.8")],
            [Decimal("2.25"), Decimal("3.75")],
            [Decimal("4.15"), Decimal("5.95")],
        ])
        camera_moves = [node for node in ast.walk(scene) if isinstance(node, ast.Call)
                        and isinstance(node.func, ast.Attribute) and node.func.attr == "move_camera"]
        self.assertEqual(len(camera_moves), 2)
        self.assertTrue(all(any(keyword.arg == "run_time" for keyword in call.keywords)
                            for call in camera_moves))
        self.assertTrue(any(isinstance(node, ast.Call) and isinstance(node.func, ast.Name)
                            and node.func.id == "WorldTransformTo" for node in ast.walk(scene)))
        self.assertEqual(entry["thumbnail"], "thumbnails/showcase/showcase-spatial-scene.png")

    def test_camera_path_lesson_uses_a_finite_updater_and_retained_path_motion(self):
        manifest = json.loads((WEB / "python/examples/noon_showcase_manifest.json").read_text())
        entry = next(item for item in manifest["entries"] if item["id"] == "showcase-camera-follows-path")
        self.assertEqual(entry["duration"], 9.8)
        self.assertEqual(entry["primary_feature"], "camera-following-path-motion")
        self.assertEqual(entry["playback_capability"], "nonreplayable-host-callbacks")
        source = (WEB / entry["path"]).read_text()
        tree = ast.parse(source, filename=entry["path"])
        scene = next(node for node in tree.body if isinstance(node, ast.ClassDef))
        construct = next(node for node in scene.body if isinstance(node, ast.AsyncFunctionDef))
        self.assertEqual([base.id for base in scene.bases if isinstance(base, ast.Name)], ["MovingCameraScene"])
        self.assertTrue(any(isinstance(node, ast.Call) and isinstance(node.func, ast.Name)
                            and node.func.id == "MoveAlongPath" for node in ast.walk(construct)))
        updater_calls = [node for node in ast.walk(construct) if isinstance(node, ast.Call)
                         and isinstance(node.func, ast.Attribute)
                         and node.func.attr in {"add_updater", "remove_updater"}]
        self.assertEqual(len(updater_calls), 2)
        self.assertCountEqual([node.func.attr for node in updater_calls], ["add_updater", "remove_updater"])
        for call in updater_calls:
            self.assertEqual(ast.unparse(call.func.value), "camera_frame")
            self.assertEqual(ast.unparse(call.args[0]), "follow_point")
        self.assertTrue(any(isinstance(node, ast.Call) and isinstance(node.func, ast.Name)
                            and node.func.id == "Restore"
                            and ast.unparse(node.args[0]) == "camera_frame"
                            for node in ast.walk(construct)))
        self.assertIn("fixed axes and path", entry["features"])

    def test_reactive_lesson_removes_exact_registered_callbacks(self):
        source = (WEB / "python/examples/showcase_reactive_relationships.py").read_text()
        tree = ast.parse(source)
        pairs = {"add_updater": [], "remove_updater": []}
        for node in ast.walk(tree):
            if isinstance(node, ast.Call) and isinstance(node.func, ast.Attribute) and node.func.attr in pairs:
                self.assertIsInstance(node.func.value, ast.Name)
                self.assertEqual(len(node.args), 1)
                self.assertIsInstance(node.args[0], ast.Name)
                pairs[node.func.attr].append((node.func.value.id, node.args[0].id))
        self.assertEqual(len(pairs["add_updater"]), 3)
        self.assertCountEqual(pairs["add_updater"], pairs["remove_updater"])

    def test_always_redraw_lesson_uses_two_bounded_producers_and_declares_callbacks(self):
        manifest = json.loads((WEB / "python/examples/noon_showcase_manifest.json").read_text())
        entry = next(item for item in manifest["entries"] if item["id"] == "showcase-always-redraw")
        self.assertEqual(entry["playback_capability"], "nonreplayable-host-callbacks")
        source = (WEB / entry["path"]).read_text()
        tree = ast.parse(source)
        scene = next(node for node in tree.body if isinstance(node, ast.ClassDef))
        construct = next(node for node in scene.body
                         if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)) and node.name == "construct")
        self.assertIsInstance(construct, ast.AsyncFunctionDef)
        redraws = [node for node in ast.walk(tree) if isinstance(node, ast.Call)
                   and isinstance(node.func, ast.Name) and node.func.id == "always_redraw"]
        self.assertEqual(len(redraws), 2)
        produced_shapes = []
        for redraw in redraws:
            self.assertIsInstance(redraw.args[0], ast.Lambda)
            produced_shapes.extend(node.func.id for node in ast.walk(redraw.args[0])
                                   if isinstance(node, ast.Call) and isinstance(node.func, ast.Name)
                                   and node.func.id in {"Circle", "Rectangle", "Line", "Path"})
        self.assertCountEqual(produced_shapes, ["Circle", "Rectangle"])
        self.assertFalse(any(isinstance(node, ast.Call) and isinstance(node.func, ast.Attribute)
                             and node.func.attr in {"add_updater", "remove_updater"}
                             for node in ast.walk(tree)))

    def test_reactive_targets_are_bound_and_registered_before_first_play(self):
        source = (WEB / "python/examples/showcase_reactive_relationships.py").read_text()
        tree = ast.parse(source)
        calls = [node for node in ast.walk(tree) if isinstance(node, ast.Call)]
        plays = sorted((node for node in calls if isinstance(node.func, ast.Attribute)
                        and isinstance(node.func.value, ast.Name) and node.func.value.id == "self"
                        and node.func.attr in {"play", "wait"}), key=lambda node: node.lineno)
        first = plays[0]
        registrations = [node for node in calls if isinstance(node.func, ast.Attribute)
                         and node.func.attr == "add_updater"]
        targets = {node.func.value.id for node in registrations}
        self.assertEqual(targets, {"left", "right", "connector"})
        self.assertTrue(all(node.lineno < first.lineno for node in registrations))
        bound = {arg.id for node in calls if isinstance(node.func, ast.Attribute)
                 and isinstance(node.func.value, ast.Name) and node.func.value.id == "self"
                 and node.func.attr == "add" and node.lineno < first.lineno
                 for arg in node.args if isinstance(arg, ast.Name)}
        self.assertTrue(targets <= bound, "detached callbacks alone do not enroll a running scene")
        faded = {node.args[0].id for node in ast.walk(first) if isinstance(node, ast.Call)
                 and isinstance(node.func, ast.Name) and node.func.id == "FadeIn"
                 and isinstance(node.args[0], ast.Name)}
        self.assertFalse(targets & faded, "FadeIn cannot introduce an already attached callback target")
        hidden = {node.func.value.id for node in calls if isinstance(node.func, ast.Attribute)
                  and isinstance(node.func.value, ast.Name) and node.func.attr == "set_opacity"
                  and node.lineno < first.lineno and len(node.args) == 1
                  and isinstance(node.args[0], ast.Constant) and node.args[0].value == 0}
        revealed = {node.func.value.value.id for node in ast.walk(first) if isinstance(node, ast.Call)
                    and isinstance(node.func, ast.Attribute) and node.func.attr == "set_opacity"
                    and isinstance(node.func.value, ast.Attribute) and node.func.value.attr == "animate"
                    and isinstance(node.func.value.value, ast.Name) and len(node.args) == 1
                    and isinstance(node.args[0], ast.Constant) and node.args[0].value == 1}
        self.assertTrue(targets <= hidden, "pre-enrolled targets must start invisible")
        self.assertTrue(targets <= revealed, "each callback target needs an animated opacity reveal")
        self.assertTrue(any(kw.arg == "rate_func" and isinstance(kw.value, ast.Name)
                            and kw.value.id == "smooth" for kw in first.keywords))

    def test_transform_lesson_teaches_explicit_copy_not_unimplemented_animations(self):
        source = (WEB / "python/examples/showcase_transform_ownership.py").read_text()
        tree = ast.parse(source)
        calls = [node for node in ast.walk(tree) if isinstance(node, ast.Call)]
        names = {node.func.id for node in calls if isinstance(node.func, ast.Name)}
        unsupported = {"ReplacementTransform", "TransformFromCopy"}
        self.assertFalse(names & unsupported)
        copies = [node for node in calls if isinstance(node.func, ast.Attribute) and node.func.attr == "copy"]
        self.assertEqual(len(copies), 1)
        self.assertEqual(copies[0].func.value.id, "original")
        morphs = [node for node in calls if isinstance(node.func, ast.Name) and node.func.id == "Transform"]
        self.assertEqual({node.args[0].id for node in morphs}, {"source", "copied"})
        manifest = json.loads((WEB / "python/examples/noon_showcase_manifest.json").read_text())
        entry = next(item for item in manifest["entries"] if item["id"] == "showcase-transform-ownership")
        self.assertFalse(set(entry["features"]) & unsupported)
        self.assertIn("copy", entry["features"])

    def test_timeline_uses_decimal_authored_durations(self):
        source = "class Example(Scene):\n def construct(self):\n  self.play(Create(x), run_time=0.1)\n  self.wait(0.2)\n"
        self.assertEqual(literal_timeline(source)[:2], (Decimal("0.3"), [[Decimal("0.1"), Decimal("0.3")]]))

    def test_ambiguous_storyboards_are_rejected_not_estimated(self):
        for body in [
            "self.play(Create(x))",
            "self.play(Create(x), run_time=duration)",
            "self.play(Create(x), run_time=True)",
            "self.wait(-1)",
            "for x in objects:\n   self.play(Create(x), run_time=1)",
        ]:
            with self.subTest(body=body):
                source = "class Example(Scene):\n def construct(self):\n  " + body + "\n"
                with self.assertRaises(ValueError):
                    literal_timeline(source)


if __name__ == "__main__":
    unittest.main()
