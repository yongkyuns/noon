import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimAutomaticWaitDispatchTests(unittest.TestCase):
    def test_wait_selects_one_timing_authority_for_each_execution_mode(self) -> None:
        python_dir = Path(__file__).resolve().parent
        env = os.environ.copy()
        env["PYTHONPATH"] = os.pathsep.join(
            part for part in (str(python_dir), env.get("PYTHONPATH", "")) if part
        )
        source = textwrap.dedent(
            """
            import asyncio
            import sys
            import types

            fake_js = types.ModuleType("js")
            fake_js.__getattr__ = lambda name: (lambda *args, **kwargs: None)
            sys.modules["js"] = fake_js

            import _manim_compat as compat
            compat.install()
            import _manim_canonical_scene as canonical
            canonical.install()
            from noon import Scene

            class Context:
                def __init__(self, authored_duration=0.0):
                    self.duration = authored_duration
                    self.calls = []

                def authoredDuration(self):
                    return self.duration

                def beginOrdinaryWait(self, duration):
                    self.calls.append(("begin", duration))

                def authoredWait(self, duration):
                    self.calls.append(("authored", duration))
                    self.duration += duration

            class WaitScene(Scene):
                def __init__(self, duration):
                    super().__init__()
                    self.duration = duration

                def setup(self):
                    pass

                def tear_down(self):
                    pass

                def construct(self):
                    returned = self.wait(self.duration)
                    assert returned is self

            # A normal worker construct with an eligible shared execution enters
            # the synchronous continuation and begins the wait in Rust.
            live_context = Context()
            canonical._create_context = lambda: live_context
            canonical.execution_context = lambda scene, callbacks=None: live_context
            canonical._start_default_synchronous_continuation = lambda scene: setattr(
                scene, canonical._SYNCHRONOUS_CONTINUATION_MODE, True
            )
            canonical._require_semantic_continuation_active = lambda scene: None
            canonical._prepare_semantic_continuation_callbacks = lambda scene, context: None
            continuation_waits = []
            canonical._synchronous_continuation_wait = lambda scene: (
                continuation_waits.append(scene), scene
            )[1]
            worker_scene = WaitScene(0.25)
            asyncio.run(canonical.execute_construct(worker_scene))
            assert live_context.calls == [("begin", 0.25)], live_context.calls
            assert continuation_waits == [worker_scene], continuation_waits
            assert canonical._legacy_authored_time(worker_scene) == 0.0

            # Explicit export with an established canonical cursor stays on the
            # typed authored cursor and never requests a renderer continuation.
            export_context = Context(0.5)
            canonical._create_context = lambda: export_context
            canonical.execution_context = lambda scene, callbacks=None: export_context
            canonical._synchronous_continuation_wait = lambda scene: (_ for _ in ()).throw(
                AssertionError("explicit export must not await a live continuation")
            )
            canonical_scene = WaitScene(0.75)
            canonical_scene._canonical_authoring_context = export_context
            asyncio.run(canonical.execute_construct(canonical_scene, export_document=True))
            assert export_context.calls == [("authored", 0.75)]
            assert canonical._legacy_authored_time(canonical_scene) == 0.0

            # An export that never selected canonical timing retains the legacy
            # cursor; export mode must not create a context merely for wait.
            canonical._create_context = lambda: (_ for _ in ()).throw(
                AssertionError("legacy export wait must not create a canonical context")
            )
            legacy_export = WaitScene(0.4)
            asyncio.run(canonical.execute_construct(legacy_export, export_document=True))
            assert abs(canonical._legacy_authored_time(legacy_export) - 0.4) < 1e-12
            assert not hasattr(legacy_export, "_canonical_authoring_context")

            # Plain CPython has no browser factory. Its ordinary wait remains the
            # existing legacy clock instead of manufacturing a shared session.
            canonical._create_context = None
            canonical.execution_context = lambda scene, callbacks=None: (_ for _ in ()).throw(
                AssertionError("CPython wait without a factory must not probe execution")
            )
            native_scene = WaitScene(0.6)
            asyncio.run(canonical.execute_construct(native_scene))
            assert abs(canonical._legacy_authored_time(native_scene) - 0.6) < 1e-12
            assert not hasattr(native_scene, "_canonical_authoring_context")
            """
        )
        completed = subprocess.run(
            [sys.executable, "-c", source],
            cwd=python_dir,
            env=env,
            text=True,
            capture_output=True,
            check=False,
        )
        self.assertEqual(
            completed.returncode,
            0,
            f"automatic wait dispatch subprocess failed:\n{completed.stdout}\n{completed.stderr}",
        )


if __name__ == "__main__":
    unittest.main()
