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
}


def literal_timeline(source):
    """Read only straight-line, explicitly timed play/wait calls; reject ambiguity."""
    tree = ast.parse(source)
    scenes = [node for node in tree.body if isinstance(node, ast.ClassDef)]
    if len(scenes) != 1:
        raise ValueError("expected one lesson Scene")
    construct = next(node for node in scenes[0].body if isinstance(node, ast.FunctionDef) and node.name == "construct")

    def timed_call(node):
        return (
            isinstance(node, ast.Call) and isinstance(node.func, ast.Attribute)
            and isinstance(node.func.value, ast.Name) and node.func.value.id == "self"
            and node.func.attr in {"play", "wait"}
        )

    direct = [
        node.value for node in construct.body
        if isinstance(node, ast.Expr) and timed_call(node.value)
    ]
    if len(direct) != sum(timed_call(node) for node in ast.walk(construct)):
        raise ValueError("nested timed calls need an explicit storyboard instead")
    clock, holds, transitions = Decimal(0), [], []
    for call in direct:
        if call.func.attr == "play":
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
                self.assertGreaterEqual(len(transitions), 4)
                self.assertGreaterEqual(holds[-1][1] - holds[-1][0], Decimal("1.0"))
                self.assertTrue(any(start < Decimal(str(entry["thumbnail_time"])) <= end for start, end in holds)
                                or any(Decimal(str(entry["thumbnail_time"])) == start for start, _ in holds))
                for beat in entry["beats"]:
                    self.assertTrue(Decimal(0) < Decimal(str(beat["time"])) <= duration)

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
        self.assertTrue(targets <= faded, "pre-enrollment must retain the animated introduction")

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
