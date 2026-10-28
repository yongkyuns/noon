"""Editorial/source contracts live outside the examples and do not substitute for rendering."""
import ast
import json
from pathlib import Path
import unittest

WEB = Path(__file__).resolve().parents[1]
MANIFEST = json.loads((WEB / "python/examples/noon_showcase_manifest.json").read_text())
# Qualified by the actual Pyodide showcase captures on both renderer backends.
# These are bundled standard-library modules; optional packages stay excluded.
QUALIFIED_BROWSER_MODULES = {"noon", "math", "colorsys", "struct", "zlib", "array", "re"}


class ShowcaseSourceContract(unittest.TestCase):
    def test_sources_are_self_contained_animated_lessons(self):
        for entry in MANIFEST["entries"]:
            with self.subTest(example=entry["id"]):
                source = (WEB / entry["path"]).read_text()
                tree = ast.parse(source, filename=entry["path"])
                compile(tree, entry["path"], "exec")
                self.assertFalse(any(isinstance(node, ast.Assert) for node in ast.walk(tree)))
                for node in ast.walk(tree):
                    if isinstance(node, ast.ImportFrom):
                        self.assertEqual(node.level, 0, "examples must not depend on a surrounding package")
                        modules = {node.module}
                    elif isinstance(node, ast.Import):
                        modules = {alias.name for alias in node.names}
                    else:
                        continue
                    self.assertLessEqual(modules, QUALIFIED_BROWSER_MODULES,
                                         "new package dependencies require explicit browser qualification")
                if entry["id"] == "showcase-background-music":
                    self._assert_finite_audio_lesson(entry, tree)
                    continue
                scenes = [node for node in tree.body if isinstance(node, ast.ClassDef)]
                self.assertEqual(len(scenes), 1)
                is_surface = entry["id"] == "showcase-spatial-scene"
                is_axes = entry["id"] == "showcase-three-d-axes"
                is_linear = entry["id"] == "showcase-linear-algebra"
                is_camera = entry["id"] == "showcase-camera-follows-path"
                scene_bases = {base.id for base in scenes[0].bases if isinstance(base, ast.Name)}
                expected_base = "ThreeDScene" if is_surface or is_axes else "LinearTransformationScene" if is_linear else "MovingCameraScene" if is_camera else "Scene"
                self.assertIn(expected_base, scene_bases)
                calls = [node for node in ast.walk(scenes[0]) if isinstance(node, ast.Call)]
                is_cuda = entry["id"] == "showcase-cuda-transpose"
                if is_cuda:
                    # This lesson uses awaited play_for() calls to preserve source-owned
                    # Python continuation. Admit those calls only if the one helper
                    # awaits the real Scene.play; a no-op wrapper must not pass.
                    self.assertEqual(entry["path"], "python/examples/cuda_transpose_banks.py")
                    helpers = [method for method in scenes[0].body
                               if isinstance(method, ast.AsyncFunctionDef) and method.name == "play_for"]
                    self.assertEqual(len(helpers), 1, "CUDA lesson must have one async play helper")
                    delegates = [node for node in ast.walk(helpers[0])
                                 if isinstance(node, ast.Await) and isinstance(node.value, ast.Call)
                                 and isinstance(node.value.func, ast.Attribute)
                                 and isinstance(node.value.func.value, ast.Name)
                                 and node.value.func.value.id == "self" and node.value.func.attr == "play"]
                    self.assertEqual(len(delegates), 1, "CUDA helper must await the real Scene.play")
                    parents = {child: parent for parent in ast.walk(scenes[0])
                               for child in ast.iter_child_nodes(parent)}
                    progression = {"play_for"}
                else:
                    progression = {"play", "move_camera"} if is_surface or is_axes else {"apply_matrix"} if is_linear else {"play"}
                plays = [node for node in calls if isinstance(node.func, ast.Attribute)
                         and isinstance(node.func.value, ast.Name) and node.func.value.id == "self"
                         and node.func.attr in progression]
                self.assertGreaterEqual(len(plays), 2, "a lesson needs a progression, not just a static API probe")
                if is_cuda:
                    self.assertTrue(all(isinstance(parents[call], ast.Await) for call in plays),
                                    "CUDA play_for calls must all be awaited")
                if is_surface:
                    self.assertTrue(any(isinstance(node.func, ast.Attribute)
                                        and isinstance(node.func.value, ast.Name)
                                        and node.func.value.id == "Mesh3D"
                                        and node.func.attr == "parametric" for node in calls))
                    self.assertTrue(any(isinstance(node.func, ast.Name)
                                        and node.func.id == "WorldTransformTo" for node in calls))
                elif is_axes:
                    self.assertTrue(any(isinstance(node.func, ast.Name)
                                        and node.func.id == "ThreeDAxes" for node in calls))
                    self.assertTrue(any(isinstance(node.func, ast.Attribute)
                                        and node.func.attr == "c2p" for node in calls))
                elif is_linear:
                    self.assertTrue(any(isinstance(node.func, ast.Attribute)
                                        and node.func.attr == "add_vector" for node in calls))
                else:
                    self.assertTrue(
                        any(isinstance(node.func, ast.Name)
                            and node.func.id in {"FadeIn", "Write", "Create"} for node in calls)
                        or any(isinstance(node, ast.Attribute) and node.attr == "animate"
                               for node in ast.walk(scenes[0])),
                        "teach an entrance or ordinary target animation")
                self.assertTrue(any(isinstance(node.func, ast.Attribute) and node.func.attr == "wait" for node in calls), "hold a readable state")
    def _assert_finite_audio_lesson(self, entry, tree):
        """The explicit result profile is separate from ordinary continuation lessons."""
        self.assertEqual(entry["path"], "python/examples/background_music_lesson.py")
        self.assertFalse(any(isinstance(node, ast.ClassDef) for node in ast.walk(tree)))
        results = [node for node in tree.body if isinstance(node, ast.Assign)
                   and any(isinstance(target, ast.Name) and target.id == "result" for target in node.targets)]
        self.assertEqual(len(results), 1)
        self.assertIsInstance(results[0].value, ast.Call)
        self.assertEqual(results[0].value.func.id, "Scene")
        calls = [node for node in ast.walk(tree) if isinstance(node, ast.Call)
                 and isinstance(node.func, ast.Attribute) and isinstance(node.func.value, ast.Name)
                 and node.func.value.id == "result"]
        named = lambda name: [call for call in calls if call.func.attr == name]
        self.assertFalse(any(named(name) for name in ("play", "wait", "add_updater", "on_click")))
        self.assertEqual(len(named("bind_position")), 2)
        self.assertEqual(len(named("value_tracker")), 1)
        self.assertEqual([ast.literal_eval(call.args[0]) for call in named("next_section")],
                         ["Double a positive input", "Keep the sign", "Return to zero"])
        declarations = named("declare_value")
        self.assertEqual([ast.literal_eval(call.args[1]) for call in declarations], [2, -2, 0])
        durations = [ast.literal_eval(next(keyword.value for keyword in call.keywords if keyword.arg == "run_time"))
                     for call in declarations]
        holds = [ast.literal_eval(call.args[0]) for call in named("declare_wait")]
        self.assertEqual(durations, [8, 8, 8])
        self.assertEqual(holds, [1, 1, 1, 1, 1, 1])
        self.assertEqual(sum(durations) + sum(holds), entry["duration"])
        self.assertEqual(len(named("audio")), 1)
        audio = named("audio")[0]
        self.assertEqual(ast.unparse(audio.args[0]), "AudioSource.small_discovery()")
        self.assertEqual({keyword.arg: ast.literal_eval(keyword.value) for keyword in audio.keywords},
                         {"duration": 30, "gain": 0.5, "fade_in": 1.5, "fade_out": 2.5})

    def test_dynamic_scene_is_not_the_unchanged_regression_workload(self):
        entry = next(item for item in MANIFEST["entries"] if item.get("performance"))
        self.assertEqual(len(entry["beats"]), 6)
        self.assertGreater(entry["duration"], 15, "allow viewers to perceive the six animation sequences")
        self.assertLessEqual(entry["duration"], 20, "keep the featured composition brisk")
        source = (WEB / entry["path"]).read_text()
        self.assertIn("ROWS = 20", source)
        self.assertIn("COLS = 30", source)
        self.assertIn("TRACKS = 24", source)
        self.assertIn("shapes[::3]", source)
        self.assertNotEqual(entry["path"], "python/examples/manim_parity_stress_grid.py")

    def test_showcase_destinations_do_not_reuse_illustrated_reference_posters(self):
        posters = [entry["thumbnail"] for entry in MANIFEST["entries"]]
        self.assertEqual(len(posters), len(set(posters)))
        self.assertTrue(all(path.startswith("thumbnails/showcase/") and path.endswith(".png") for path in posters))
        self.assertEqual(MANIFEST["publication"], "curated")


if __name__ == "__main__":
    unittest.main()
