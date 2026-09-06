import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimRotatingPrincipalAxisProjectionTests(unittest.TestCase):
    def test_public_demo_principal_axis_turns_follow_full_history(self) -> None:
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
            import math
            import sys
            import types

            fake_js = types.ModuleType("js")

            class Result:
                ok = True
                errorKind = ""
                message = ""

            class JsArray(list):
                @property
                def length(self):
                    return len(self)

            def resolve_animation_options(
                default_lag_ratio,
                animation_run_time,
                animation_rate_func,
                animation_lag_ratio,
                path_arc,
                reverse_rate_function,
                play_run_time,
                play_rate_func,
                play_lag_ratio,
            ):
                result = Result()
                result.runTime = (
                    play_run_time
                    if math.isfinite(play_run_time)
                    else animation_run_time
                    if math.isfinite(animation_run_time)
                    else 1.0
                )
                result.rateFunc = play_rate_func or animation_rate_func or "smooth"
                result.lagRatio = (
                    play_lag_ratio
                    if math.isfinite(play_lag_ratio)
                    else animation_lag_ratio
                    if math.isfinite(animation_lag_ratio)
                    else default_lag_ratio
                )
                result.pathArc = path_arc if math.isfinite(path_arc) else 0.0
                result.reverseRateFunction = reverse_rate_function == 1
                return result

            def resolve_uniform_schedule(child_count, lag_ratio, run_time):
                result = Result()
                intrinsic = 1.0 + max(int(child_count) - 1, 0) * float(lag_ratio)
                scale = float(run_time) / intrinsic
                result.intervals = JsArray(
                    [
                        types.SimpleNamespace(
                            startTime=index * float(lag_ratio) * scale,
                            duration=scale,
                        )
                        for index in range(int(child_count))
                    ]
                )
                return result

            fake_js.noonResolveAnimationOptions = resolve_animation_options
            fake_js.noonResolveUniformCompositionSchedule = resolve_uniform_schedule
            sys.modules["js"] = fake_js

            import _manim_compat
            _manim_compat.install()
            import _manim_rate_functions
            _manim_rate_functions.install()
            import _manim_phase_b  # noqa: F401
            import _manim_geometry  # noqa: F401
            import _manim_animate
            import _manim_rotate
            _manim_rotate.install()
            import _manim_updaters
            _manim_updaters.install()

            from noon import (
                Arrow,
                BLUE,
                Circle,
                DEGREES,
                GOLD,
                Line,
                ORIGIN,
                PI,
                RIGHT,
                Rotating,
                Scene,
                Square,
                UP,
                VGroup,
            )

            semantic_commits = []

            def record_semantic_commit(source, target):
                semantic_commits.append((id(source), target.to_ir()))

            # Native tests have no WASM semantic handle. Record the same commit hook
            # the browser uses so family Rotating cannot silently bypass ownership
            # synchronization again.
            _manim_animate._semantic_handles.commit_transform_target = record_semantic_commit

            def world_point(snapshot, x=0.0, y=0.0):
                transform = snapshot["transform"]
                scale = transform["scale"]
                rotation = float(transform["rotation"])
                translation = transform["translation"]
                local_x = x * float(scale["x"])
                local_y = y * float(scale["y"])
                cosine = math.cos(rotation)
                sine = math.sin(rotation)
                return (
                    local_x * cosine - local_y * sine + float(translation["x"]),
                    local_x * sine + local_y * cosine + float(translation["y"]),
                )

            def snapshot_center(snapshot):
                geometry = snapshot["geometry"]
                points = []
                if "circle" in geometry:
                    radius = float(geometry["circle"]["radius"])
                    points = [(-radius, -radius), (radius, radius)]
                elif "rectangle" in geometry:
                    size = geometry["rectangle"]["size"]
                    half_x = float(size["x"]) / 2.0
                    half_y = float(size["y"]) / 2.0
                    points = [(-half_x, -half_y), (half_x, half_y)]
                elif "line" in geometry:
                    line = geometry["line"]
                    points = [
                        (float(line["start"]["x"]), float(line["start"]["y"])),
                        (float(line["end"]["x"]), float(line["end"]["y"])),
                    ]
                elif "vector_path" in geometry:
                    for command in geometry["vector_path"]["commands"]:
                        if command == "close":
                            continue
                        payload = next(iter(command.values()))
                        for key in ("to", "control", "control1", "control2"):
                            if key in payload:
                                point = payload[key]
                                points.append((float(point["x"]), float(point["y"])))
                assert points, geometry
                min_x = min(point[0] for point in points)
                max_x = max(point[0] for point in points)
                min_y = min(point[1] for point in points)
                max_y = max(point[1] for point in points)
                world_corners = [
                    world_point(snapshot, x, y)
                    for x, y in (
                        (min_x, min_y),
                        (min_x, max_y),
                        (max_x, min_y),
                        (max_x, max_y),
                    )
                ]
                return (
                    (min(point[0] for point in world_corners) + max(point[0] for point in world_corners)) / 2.0,
                    (min(point[1] for point in world_corners) + max(point[1] for point in world_corners)) / 2.0,
                )

            def transform_tracks(document, start, end):
                return [
                    track
                    for track in document["tracks"]
                    if track["property"] == "transform"
                    and start <= float(track["timing"]["start_time"]) < end
                ]

            def endpoint_track(tracks, object_id, time):
                matches = []
                for track in tracks:
                    if track["object"] != object_id:
                        continue
                    timing = track["timing"]
                    track_end = float(timing["start_time"]) + float(timing["duration"])
                    if math.isclose(track_end, time, rel_tol=0.0, abs_tol=1e-12):
                        matches.append(track)
                assert len(matches) == 1, (object_id, time, len(matches))
                return matches[0]

            scene = Scene()
            circle = Circle(radius=1, color=BLUE)
            line = Line(start=ORIGIN, end=RIGHT)
            arrow = Arrow(start=ORIGIN, end=RIGHT, buff=0, color=GOLD)
            family = VGroup(circle, line, arrow)
            scene.add(family)

            circle_id = circle.id
            shaft_id = arrow._shaft.id
            tip_id = arrow._tip.id

            # Execute the literal public RotatingDemo history. The first three z-axis
            # plays are important: browser semantic handles lower their wire rotation
            # through f32, so exact multiples of pi return with ~1e-7 rad residue.
            anim_kw = {"about_point": arrow.get_start(), "run_time": 1}
            scene.play(Rotating(arrow, 180 * DEGREES, **anim_kw))
            assert len(semantic_commits) == 2
            scene.play(Rotating(arrow, PI, **anim_kw))
            assert len(semantic_commits) == 4
            scene.play(Rotating(family, PI, about_point=RIGHT))
            assert scene.time == 7.0
            assert len(semantic_commits) == 8

            scene.play(Rotating(family, PI, axis=UP, about_point=ORIGIN))
            assert scene.time == 12.0
            assert len(semantic_commits) == 12
            scene.play(Rotating(family, PI, axis=RIGHT, about_edge=UP))
            assert scene.time == 17.0
            assert len(semantic_commits) == 16

            document = scene.to_document()
            y_tracks = transform_tracks(document, 7.0, 12.0)
            x_tracks = transform_tracks(document, 12.0, 17.0)
            assert len(y_tracks) > 4
            assert len(x_tracks) > 4

            # At 90 degrees around world y every world-x coordinate projects onto the
            # pivot plane. Circle therefore becomes edge-on and Arrow's retained shaft
            # endpoint remains attached to the geometric center of its triangular tip.
            y_circle_mid = endpoint_track(y_tracks, circle_id, 9.5)["values"]["object"]["to"]
            y_scale = y_circle_mid["transform"]["scale"]
            assert abs(float(y_scale["x"])) < 1e-12, y_scale

            y_shaft_mid = endpoint_track(y_tracks, shaft_id, 9.5)["values"]["object"]["to"]
            y_tip_mid = endpoint_track(y_tracks, tip_id, 9.5)["values"]["object"]["to"]
            shaft_geometry = y_shaft_mid["geometry"]["line"]
            shaft_end = shaft_geometry["end"]
            projected_shaft_end = world_point(
                y_shaft_mid,
                float(shaft_end["x"]),
                float(shaft_end["y"]),
            )
            assert math.dist(projected_shaft_end, snapshot_center(y_tip_mid)) < 1e-9

            # The final x-axis turn is likewise a cosine compression in world y.
            x_circle_mid = endpoint_track(x_tracks, circle_id, 14.5)["values"]["object"]["to"]
            x_transform = x_circle_mid["transform"]
            assert abs(float(x_transform["scale"]["y"])) < 1e-12, x_transform
            assert math.isclose(
                float(x_transform["translation"]["y"]),
                1.0,
                rel_tol=0.0,
                abs_tol=1e-9,
            )

            x_shaft_mid = endpoint_track(x_tracks, shaft_id, 14.5)["values"]["object"]["to"]
            x_tip_mid = endpoint_track(x_tracks, tip_id, 14.5)["values"]["object"]["to"]
            shaft_geometry = x_shaft_mid["geometry"]["line"]
            shaft_end = shaft_geometry["end"]
            projected_shaft_end = world_point(
                x_shaft_mid,
                float(shaft_end["x"]),
                float(shaft_end["y"]),
            )
            assert math.dist(projected_shaft_end, snapshot_center(x_tip_mid)) < 1e-9

            # Every retained leaf must hand off at the exact authored play endpoint.
            for tracks, end_time in ((y_tracks, 12.0), (x_tracks, 17.0)):
                object_ids = {track["object"] for track in tracks}
                for object_id in object_ids:
                    final_track = max(
                        (track for track in tracks if track["object"] == object_id),
                        key=lambda track: track["timing"]["start_time"],
                    )
                    timing = final_track["timing"]
                    assert float(timing["start_time"]) + float(timing["duration"]) == end_time

            # Target copying still observes the final committed orientations. This
            # retained fixture cannot schedule the now-migrated family Transform;
            # its real shared execution is covered by the paired family example.
            builder = family.animate.move_to(ORIGIN)
            source_leaves = _manim_compat._leaf_mobjects(family)
            target_leaves = _manim_compat._leaf_mobjects(builder.target)
            for source, target in zip(source_leaves, target_leaves):
                source_transform = source._current_raw().to_ir()["transform"]
                target_transform = target._current_raw().to_ir()["transform"]
                assert source_transform["rotation"] == target_transform["rotation"]
                assert source_transform["scale"] == target_transform["scale"]
            try:
                scene.play(builder)
                raise AssertionError("legacy fixture scheduled a shared family Transform")
            except NotImplementedError as error:
                assert "shared semantic composition" in str(error)
            assert scene.time == 17.0
            assert len(semantic_commits) == 16

            # This is the actual f32 wire value obtained near 3*pi. It is a principal
            # orientation despite representational roundoff and must not be confused
            # with a genuinely arbitrary basis requiring shear.
            precision_scene = Scene()
            wire_pi3 = 9.42477798461914
            rounded = Circle(rotation=wire_pi3)
            rounded_family = VGroup(rounded)
            precision_scene.add(rounded_family)
            precision_scene.play(
                Rotating(
                    rounded_family,
                    PI,
                    axis=UP,
                    about_point=ORIGIN,
                    run_time=1.0,
                )
            )
            assert precision_scene.time == 1.0

            # A genuinely non-axis-aligned 2D basis still requires shear after
            # principal-axis projection. Reject that case transactionally rather than
            # silently broadening the f32 precision allowance into an approximation.
            unsupported_scene = Scene()
            diamond = Square().rotate(PI / 4.0)
            unsupported_family = VGroup(diamond)
            unsupported_scene.add(unsupported_family)
            before = unsupported_scene.to_document()
            try:
                unsupported_scene.play(
                    Rotating(
                        unsupported_family,
                        PI,
                        axis=UP,
                        about_point=ORIGIN,
                        run_time=1.0,
                    )
                )
            except NotImplementedError as error:
                assert "shear" in str(error).lower()
            else:
                raise AssertionError("non-axis-aligned projected Rotating must be rejected")
            assert unsupported_scene.time == 0.0
            assert unsupported_scene.to_document() == before
            """
        )

        completed = subprocess.run(
            [sys.executable, "-c", source],
            check=False,
            cwd=python_dir,
            env=env,
            capture_output=True,
            text=True,
        )
        self.assertEqual(
            completed.returncode,
            0,
            f"compatibility subprocess failed:\nstdout:\n{completed.stdout}\nstderr:\n{completed.stderr}",
        )


if __name__ == "__main__":
    unittest.main()
