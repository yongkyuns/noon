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
    "showcase-gpu-glow",
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

    def test_gpu_glow_has_real_absent_to_neutral_and_replacement_beats(self):
        manifest = json.loads((WEB / "python/examples/noon_showcase_manifest.json").read_text())
        entry = next(item for item in manifest["entries"] if item["id"] == "showcase-gpu-glow")
        self.assertEqual(entry["primary_feature"], "gpu-glow-parameter-animation")
        self.assertEqual(entry["duration"], 12.0)
        self.assertEqual(entry["thumbnail_time"], 11.4)
        self.assertEqual(entry["still_intervals"], [
            [2.9, 3.4], [4.8, 5.8], [7.1, 7.45], [8.7, 9.5], [11.0, 12.0],
        ])
        source = (WEB / entry["path"]).read_text()
        tree = ast.parse(source)
        calls = [node for node in ast.walk(tree) if isinstance(node, ast.Call)]
        effects = [node for node in calls if isinstance(node.func, ast.Attribute)
                   and node.func.attr == "set_glow"]
        self.assertEqual(len(effects), 14, "parameter beats plus path/text attachment and animation")
        animated = [node for node in effects if isinstance(node.func.value, ast.Attribute) and node.func.value.attr == "animate"]
        self.assertEqual(len(animated), 12, "glow changes use the shared semantic animation lane")
        self.assertTrue(any(isinstance(node.func, ast.Name) and node.func.id == "Create" for node in calls))
        self.assertTrue(any(isinstance(node.func, ast.Name) and node.func.id == "Write" for node in calls))
        updates = {kw.arg for node in effects for kw in node.keywords}
        self.assertTrue({"intensity", "radius", "color"}.issubset(updates))
        self.assertTrue(any(isinstance(node, ast.Call)
                            and isinstance(node.func, ast.Attribute)
                            and node.func.attr == "remove_glow" for node in calls))
        self.assertFalse(any(isinstance(node.func, ast.Attribute)
                             and node.func.attr in {"add_updater", "remove_updater"}
                             for node in calls))
        duration, holds, transitions = literal_timeline(source)
        self.assertEqual(duration, Decimal("12.0"))
        self.assertEqual(len(transitions), 7)
        self.assertEqual(len(holds), 5)
        self.assertTrue(any(start < Decimal("5.3") <= end for start, end in holds))

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

    def test_camera_and_dot_follow_the_same_path_in_one_segment(self):
        manifest = json.loads((WEB / "python/examples/noon_showcase_manifest.json").read_text())
        entry = next(item for item in manifest["entries"] if item["id"] == "showcase-camera-follows-path")
        self.assertEqual(entry["playback_capability"], "deterministic-retained-replay")
        tree = ast.parse((WEB / entry["path"]).read_text())
        calls = [node for node in ast.walk(tree) if isinstance(node, ast.Call)]
        plays = [node for node in calls if isinstance(node.func, ast.Attribute) and node.func.attr == "play"]
        follow = [
            [tuple(ast.unparse(value) for value in arg.args) for arg in play.args
             if isinstance(arg, ast.Call) and isinstance(arg.func, ast.Name) and arg.func.id == "MoveAlongPath"]
            for play in plays
        ]
        self.assertEqual([args for args in follow if args], [[("moving_dot", "path"), ("camera_frame", "path")]])
        self.assertTrue(any(isinstance(node.func, ast.Name) and node.func.id == "Restore"
                            and ast.unparse(node.args[0]) == "camera_frame" for node in calls))

    def test_reactive_lesson_animates_both_endpoint_positions_and_connector(self):
        tree = ast.parse((WEB / "python/examples/showcase_reactive_relationships.py").read_text())
        moves = [node for node in ast.walk(tree) if isinstance(node, ast.Call)
                 and isinstance(node.func, ast.Name) and node.func.id == "move_readings"]
        self.assertEqual(len(moves), 2)
        self.assertTrue(all([ast.unparse(arg) for arg in move.args[:3]] == ["left", "right", "connector"]
                            for move in moves))
        morphs = [node for node in ast.walk(tree) if isinstance(node, ast.Call)
                  and isinstance(node.func, ast.Name) and node.func.id == "Transform"]
        self.assertEqual(len(morphs), 1)
        self.assertEqual(ast.unparse(morphs[0].args[0]), "connector")
        self.assertEqual([ast.unparse(arg) for arg in morphs[0].args[1].args], ["left_end", "right_end"])

    def test_animated_values_lesson_retains_shared_tracker_bindings(self):
        manifest = json.loads((WEB / "python/examples/noon_showcase_manifest.json").read_text())
        entry = next(item for item in manifest["entries"] if item["id"] == "showcase-always-redraw")
        self.assertEqual(entry["playback_capability"], "deterministic-retained-replay")
        tree = ast.parse((WEB / entry["path"]).read_text())
        bindings = [node for node in ast.walk(tree) if isinstance(node, ast.Call)
                    and isinstance(node.func, ast.Attribute) and node.func.attr == "bind_sampled"]
        self.assertCountEqual([tuple(ast.unparse(arg) for arg in call.args[:2]) + (call.args[2].value,)
                               for call in bindings], [
            ("circle", "clock", "position"), ("circle", "clock", "scale"),
            ("rectangle", "clock", "position"), ("rectangle", "clock", "rotation"),
        ])

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
