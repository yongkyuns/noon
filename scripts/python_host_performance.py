#!/usr/bin/env python3
"""Measure real native binding work; assert interpreter locality, not guessed FPS.

Finite sampled execution is compared at equal logical work. CPU and wall times
are observations, not a universal zero-overhead or native/window rendering claim.
The existing product gate owns paired baseline/candidate browser performance.
"""
from __future__ import annotations
import argparse
import asyncio
from collections import Counter
import gc
import json
from pathlib import Path
import statistics
import sys
import time
import tracemalloc

ROOT = Path(__file__).resolve().parents[1]


def sample(object_count: int, segments: int, sample_hz: float, duration: float,
           *, instrument=False) -> dict:
    from noon import Scene, Circle, RIGHT, linear
    from noon_native import run_scene, close_scene
    calls = Counter()
    def profile(frame, event, _arg):
        if event == "call":
            filename = Path(frame.f_code.co_filename).name
            if filename in {"_noon_native_host.py", "_manim_scene.py", "_manim_updaters.py"}:
                calls[(filename, frame.f_code.co_name)] += 1
    class Measured(Scene):
        async def construct(self):
            begun, cpu = time.perf_counter(), time.process_time()
            self.objects = [Circle(0.2).shift((i % 25) * 0.1 * RIGHT) for i in range(object_count)]
            self.add(*self.objects)
            self.creation = [time.perf_counter() - begun, time.process_time() - cpu]
            begun, cpu = time.perf_counter(), time.process_time()
            for _ in range(segments):
                await self.play(self.objects[0].animate.shift(0.01 * RIGHT),
                                run_time=duration, rate_func=linear)
            self.playback = [time.perf_counter() - begun, time.process_time() - cpu]
            before = self._canonical_authoring_context.metrics()
            # A long inactive wait must not create per-frame evaluations or
            # interpreter turns. Rust's wake state selects its one deadline.
            await self.wait(100)
            after = self._canonical_authoring_context.metrics()
            self.idle_frames = after[0] - before[0]
            assert self.idle_frames <= 1, ("idle polling", self.idle_frames)
            before = time.process_time()
            self.objects[0].shift(RIGHT)
            self.edit_cpu = time.process_time() - before
    async def run():
        scene = await run_scene(Measured, sample_hz=sample_hz)
        try:
            metrics = scene._canonical_authoring_context.metrics()
            assert metrics[2] == 0, "deterministic playback requested Python callbacks"
            return {"objects": object_count, "segments": segments, "sample_hz": sample_hz,
                    "segment_duration": duration, "creation_seconds": scene.creation,
                    "playback_seconds": scene.playback, "local_edit_cpu_seconds": scene.edit_cpu,
                    "runtime_samples": metrics[0], "runtime_segments": metrics[1],
                    "callback_regions": metrics[2], "idle_evaluations": scene.idle_frames}
        finally:
            close_scene(scene)
    try:
        if instrument:
            sys.setprofile(profile)
        result = asyncio.run(run())
    finally:
        if instrument:
            sys.setprofile(None)
    if instrument:
        result["host_python_calls"] = {f"{module}:{name}": count for (module, name), count in sorted(calls.items())}
    gc.collect()
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--python-path", type=Path, default=ROOT / "build/python")
    parser.add_argument("--output", type=Path, default=ROOT / "artifacts/python-host/native-performance.json")
    args = parser.parse_args()
    sys.path.insert(0, str(args.python_path.resolve()))
    # Warm the interpreter, shared geometry and binding code before timing.
    sample(2, 1, 60, 0.25)
    rows = []
    for count, segments, hz, duration in [(1, 1, 60, 5), (600, 1, 60, 5),
                                          (600, 120, 60, 1 / 60)]:
        repeats = [sample(count, segments, hz, duration) for _ in range(5)]
        rows.append({"samples": repeats, "median_playback_cpu_seconds":
                     statistics.median(r["playback_seconds"][1] for r in repeats)})
    # Profiling is kept OUT of the timing rows. Increasing evaluated frames by
    # tenfold must not increase Python continuation or callback invocations.
    low = sample(32, 1, 6, 5, instrument=True)
    high = sample(32, 1, 60, 5, instrument=True)
    assert high["runtime_samples"] > 5 * low["runtime_samples"]
    assert high["host_python_calls"] == low["host_python_calls"], "Python work scaled with rendered samples"
    # A Python heap retention trend is not total RSS; name it precisely.
    tracemalloc.start()
    for _ in range(4):
        sample(2, 1, 4, 0.25)
    gc.collect()
    before = tracemalloc.get_traced_memory()[0]
    for _ in range(32):
        sample(2, 1, 4, 0.25)
    gc.collect()
    after, peak = tracemalloc.get_traced_memory()
    tracemalloc.stop()
    assert after - before < 256 * 1024, ("unbounded Python retention", after - before)
    report = {"schema": 1, "host": "native-cpython", "python": sys.version,
              "clock": "process_time + perf_counter", "renderer": "none / explicit sampled runtime",
              "timings": rows, "frame_locality": {"low": low, "high": high},
              "python_retention": {"iterations": 32, "before_bytes": before,
                                    "after_bytes": after, "peak_bytes": peak},
              "limits": ["Not a GPU or native-window benchmark", "Not an isolated allocator/RSS measurement",
                         "Browser regression evidence is produced by the existing paired Product Gate"]}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2, allow_nan=False) + "\n")
    print("Native timings recorded; sample-independent Python calls and idle/retirement bounds passed")


if __name__ == "__main__":
    main()
