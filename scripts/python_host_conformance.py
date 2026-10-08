#!/usr/bin/env python3
"""Real native CPython half of the unchanged-source host conformance gate."""
from __future__ import annotations
import argparse
import asyncio
from contextlib import redirect_stdout
import gc
import hashlib
import io
import json
from pathlib import Path
import platform
import sys
import time
import weakref

ROOT = Path(__file__).resolve().parents[1]
CASES = ("sequential", "lifecycle", "callbacks", "atomicity", "cancellation",
         "callback_failure", "synchronous", "portable", "render_options")
EXPECTED_TERMINAL = {"cancellation": "CancelledError", "callback_failure": "NoonCallbackError"}
PREFIX = "NOON_HOST_REPORT "


def run_case(name: str, *, sample_hz=4.0) -> dict:
    from noon_native import run_source, close_scene
    path = ROOT / "parity/python-host" / f"{name}.py"
    source = path.read_text(encoding="utf8")
    output = io.StringIO()
    metrics = None
    refs = []
    async def execute():
        nonlocal metrics
        scene = await run_source(source, sample_hz=sample_hz,
                                 portable=name == "portable", filename=str(path))
        try:
            metrics = scene._canonical_authoring_context.metrics()
            refs.append(weakref.ref(scene))
        finally:
            close_scene(scene)
    started = time.perf_counter()
    cpu = time.process_time()
    failure = None
    with redirect_stdout(output):
        try:
            asyncio.run(execute())
        except BaseException as error:
            if isinstance(error, (KeyboardInterrupt, SystemExit)):
                raise
            failure = type(error).__name__
            if failure != EXPECTED_TERMINAL.get(name):
                raise
    assert failure == EXPECTED_TERMINAL.get(name), (name, failure)
    elapsed = time.perf_counter() - started
    cpu = time.process_time() - cpu
    reports = [json.loads(line[len(PREFIX):]) for line in output.getvalue().splitlines()
               if line.startswith(PREFIX)]
    assert len(reports) == 1 and reports[0]["case"] == name, (name, output.getvalue())
    gc.collect()
    assert not any(ref() is not None for ref in refs), f"native scene leaked after {name}"
    return {"case": name, "source_sha256": hashlib.sha256(source.encode()).hexdigest(),
            "terminal": failure, "report": reports[0], "native_metrics": metrics,
            "elapsed_seconds": elapsed, "cpu_seconds": cpu}


def lifecycle_guards() -> dict:
    """Real foreign-handle/retirement checks and repeated callback-table cleanup."""
    from _noon_native import Store, GeometryOptions
    import _manim_updaters
    from noon import Circle, Scene, RIGHT, NoonOwnershipError
    from noon_native import run_scene, close_scene
    from _noon_errors import engine_call, NoonForeignHandleError
    a, b = Store(), Store()
    # A same numeric slot/generation in another arena is still foreign.
    handle = a.geometry(GeometryOptions.circle(0.5))
    foreign_context = b.context()
    assert handle.centerCoordinates() == (0.0, 0.0)
    for query in (foreign_context.containsMobject, foreign_context.queryMobjectCenter):
        try:
            engine_call(query, handle)
        except NoonForeignHandleError:
            pass
        else:
            raise AssertionError("foreign store provenance was not checked")
    refs = []
    before = len(_manim_updaters._CANONICAL_SESSIONS)
    before_tracked = len(_manim_updaters._TRACKED_MOBJECTS)
    class Repeated(Scene):
        async def construct(self):
            self.obj = Circle(0.5)
            self.add(self.obj)
            self.obj.add_updater(lambda m, dt: m.shift(dt * RIGHT))
            await self.wait(0.25)
    async def loop():
        for _ in range(32):
            scene = await run_scene(Repeated, sample_hz=4)
            obj = scene.obj
            assert obj.get_center() == (0.25, 0.0)
            refs.append(weakref.ref(scene))
            close_scene(scene)
            close_scene(scene)  # idempotent
            for operation in (lambda: obj.shift(RIGHT), obj.get_center):
                try:
                    operation()
                except NoonOwnershipError:
                    pass
                else:
                    raise AssertionError("retired scene accepted a persistent read or write")
    asyncio.run(loop())
    gc.collect()
    assert all(ref() is None for ref in refs), "retired native scene retained by Python"
    assert len(_manim_updaters._CANONICAL_SESSIONS) == before
    assert len(_manim_updaters._TRACKED_MOBJECTS) == before_tracked
    return {"iterations": len(refs), "retained_scenes": 0, "foreign_handle_rejected": True,
            "retired_write_rejected": True, "retired_read_rejected": True, "callback_tables_restored": True}


def module_failure_guards() -> dict:
    """A module abort must retire contexts acquired before source selection."""
    import _manim_updaters
    from noon_native import run_source
    before = (len(_manim_updaters._CANONICAL_SESSIONS),
              len(_manim_updaters._TRACKED_MOBJECTS))
    source = """from noon import *
result = Scene()
alias = result
marker = Circle(0.5)
result.add(marker)
marker.add_updater(lambda m, dt: m.shift(dt * RIGHT))
result.wait(0.25)
"""
    failures = (
        ('raise ValueError("intentional module abort")', ValueError, "intentional module abort"),
        ("result = None", TypeError, "Python authoring result must be a noon.Scene"),
    )
    for portable in (False, True):
        for suffix, kind, message in failures:
            for _ in range(4):
                try:
                    asyncio.run(run_source(source + suffix, sample_hz=4, portable=portable))
                except kind as error:
                    assert str(error) == message, error
                else:
                    raise AssertionError("module exception was swallowed")
                gc.collect()
                actual = (len(_manim_updaters._CANONICAL_SESSIONS),
                          len(_manim_updaters._TRACKED_MOBJECTS))
                assert actual == before, ("module failure leaked callbacks", actual, before)
    return {"iterations": 16, "original_and_portable_source": True,
            "module_exception_preserved": True, "invalid_result_cleanup": True,
            "callback_tables_restored": True}


def hidden_failure_guards() -> dict:
    """Unreturned constructors and helper locals must not escape host cleanup."""
    import _manim_updaters
    from noon import Scene, Circle, RIGHT
    from noon_native import run_source, run_scene, close_scene
    class Survivor(Scene):
        async def construct(self):
            self.marker = Circle(0.5)
            self.add(self.marker)
            self.marker.add_updater(lambda m, dt: None)
            await self.wait(0.25)
    survivor = asyncio.run(run_scene(Survivor, sample_hz=4))
    before = (len(_manim_updaters._CANONICAL_SESSIONS),
              len(_manim_updaters._TRACKED_MOBJECTS))
    constructor = """from noon import Scene, Circle
import weakref
class Broken(Scene):
    def __init__(self):
        super().__init__()
        context["refs"].append(weakref.ref(self))
        marker = Circle(0.5)
        self.add(marker)
        marker.add_updater(lambda m, dt: None)
        self.wait(0.25)
        raise ValueError("unreturned constructor")
result = Broken()
"""
    helper = """from noon import Scene, Circle
import weakref
import asyncio
def hidden_scene():
    scene = Scene()
    context["refs"].append(weakref.ref(scene))
    marker = Circle(0.5)
    scene.add(marker)
    marker.add_updater(lambda m, dt: None)
    scene.wait(0.25)
hidden_scene()
raise asyncio.CancelledError("lost helper scene")
"""
    cases = ((constructor, ValueError, "unreturned constructor"),
             (constructor.replace("        self.wait(0.25)\n", ""), ValueError,
              "unreturned constructor"),
             (helper, asyncio.CancelledError, "lost helper scene"))
    try:
        for portable in (False, True):
            for source, kind, message in cases:
                for _ in range(4):
                    refs = []
                    try:
                        asyncio.run(run_source(source, {"refs": refs}, sample_hz=4,
                                               portable=portable))
                    except kind as error:
                        assert str(error) == message, error
                    else:
                        raise AssertionError("hidden source failure was swallowed")
                    gc.collect()
                    assert refs and all(ref() is None for ref in refs), "hidden Scene retained"
                    actual = (len(_manim_updaters._CANONICAL_SESSIONS),
                              len(_manim_updaters._TRACKED_MOBJECTS))
                    assert actual == before, ("hidden failure leaked callbacks", actual, before)
                    # A failed new source cannot retire an existing caller-owned host.
                    survivor.marker.shift(0.01 * RIGHT)
    finally:
        close_scene(survivor)
    return {"iterations": 24, "constructor_exception_preserved": True,
            "pre_barrier_registration_cleanup": True,
            "helper_cancellation_preserved": True, "retained_hidden_scenes": 0,
            "unrelated_owner_preserved": True, "callback_tables_restored": True}


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--python-path", type=Path, default=ROOT / "build/python")
    parser.add_argument("--output", type=Path, default=ROOT / "artifacts/python-host/native.json")
    args = parser.parse_args()
    sys.path.insert(0, str(args.python_path.resolve()))
    import _noon_native  # This must be a real extension, not an import-only substitute.
    assert Path(_noon_native.__file__).suffix in {".so", ".pyd"}
    report = {"schema": 1, "host": "native-cpython", "python": sys.version,
              "platform": platform.platform(), "sample_hz": 4,
              "cases": [run_case(name) for name in CASES], "lifecycle": lifecycle_guards(),
              "module_failure": module_failure_guards(),
              "hidden_failure": hidden_failure_guards()}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2, allow_nan=False) + "\n")
    print(f"Native CPython: {len(report['cases'])} unchanged-source cases; lifecycle guards passed")


if __name__ == "__main__":
    main()
