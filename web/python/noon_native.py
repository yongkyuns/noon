"""Run Noon Python scenes with the shared Rust engine, without a browser.

Preparation executes source once; preview and MP4 export replay bounded
deterministic Rust history. MP4 export uses the native renderer and an installed
FFmpeg executable. Arbitrary callbacks can be sampled with --headless; realtime
callback preview/export remain unsupported. Predeclared audio is exported to
MP4. Native window audio is available in builds with the audio feature.
"""
from __future__ import annotations

from _noon_native_host import run_scene, run_source, close_scene

# Geometry-only extension builds intentionally omit the native output surface.
import _noon_native as _native
if hasattr(_native, "ExportCancellation"):
    ExportCancellation = _native.ExportCancellation


def prepare_view(scene):
    """Consume a completed scene into the existing native renderer/window host."""
    from _manim_scene import _context
    try:
        return _context(scene).prepare_view()
    finally:
        close_scene(scene)


def main():
    import argparse
    import asyncio
    from pathlib import Path
    from fractions import Fraction
    from math import isfinite
    from noon import __version__

    def frame_rate(value):
        try:
            rate = Fraction(value)
        except (ValueError, ZeroDivisionError) as error:
            raise argparse.ArgumentTypeError("fps must be a positive number or fraction such as 30000/1001") from error
        if rate <= 0 or max(rate.numerator, rate.denominator) > 2**31 - 1:
            raise argparse.ArgumentTypeError("fps must be a positive fraction with 32-bit terms")
        return rate.numerator, rate.denominator

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--version", action="version", version=f"Noon {__version__}")
    parser.add_argument("source", type=Path)
    parser.add_argument("--scene", dest="scene_name", help="select a Scene class from a source with multiple scenes")
    parser.add_argument("--sample-hz", type=float, default=60.0)
    parser.add_argument("--portable-source", action="store_true")
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--headless", action="store_true", help="execute and validate without a GPU or window")
    mode.add_argument("--snapshot", type=Path, help="write one native-rendered PNG instead of opening a window")
    mode.add_argument("--mp4", type=Path, help="export deterministic native-rendered MP4 without opening a window (requires FFmpeg)")
    parser.add_argument("--time", type=float, default=0.0, help="snapshot time in seconds")
    parser.add_argument("--width", type=int, default=960)
    parser.add_argument("--height", type=int, default=540)
    export = parser.add_argument_group("MP4 export")
    export.add_argument("--fps", type=frame_rate, default="30", help="exact output rate, e.g. 30 or 30000/1001 (default: 30)")
    export.add_argument("--start-frame", type=int, default=0, help="crop on the source frame grid; preceding samples are still evaluated")
    stop = export.add_mutually_exclusive_group()
    stop.add_argument("--frames", type=int, help="maximum output frame count; source completion can stop earlier")
    stop.add_argument("--end-time", type=float, help="exclusive authored-time bound in seconds")
    export.add_argument("--hold", type=float, default=0.0, help="explicit frozen hold after the source endpoint, in seconds")
    export.add_argument("--max-frames", type=int, default=108_000, help="safety limit including discarded prefix and holds (default: 108000)")
    export.add_argument("--overwrite", action="store_true", help="atomically replace an existing MP4 only after successful encoding")
    export.add_argument("--ffmpeg", default="ffmpeg", help="trusted FFmpeg executable path (default: ffmpeg on PATH)")
    export.add_argument("--crf", type=int, default=18, help="H.264 quality from 0 to 51 (default: 18)")
    args = parser.parse_args()
    if args.width <= 0 or args.height <= 0:
        parser.error("--width and --height must be positive")
    if args.mp4:
        if args.width % 2 or args.height % 2:
            parser.error("MP4 requires even --width and --height")
        if args.start_frame < 0 or args.max_frames <= args.start_frame:
            parser.error("--start-frame must be nonnegative and smaller than --max-frames")
        if args.frames is not None and args.frames <= 0:
            parser.error("--frames must be positive")
        if args.end_time is not None and (not isfinite(args.end_time) or args.end_time <= 0):
            parser.error("--end-time must be finite and positive")
        if not isfinite(args.hold) or args.hold < 0:
            parser.error("--hold must be finite and nonnegative")
        if not 0 <= args.crf <= 51:
            parser.error("--crf must be between 0 and 51")
    async def run():
        scene = await run_source(args.source.read_text(encoding="utf8"),
                                 sample_hz=args.sample_hz, portable=args.portable_source,
                                 filename=str(args.source), retain_history=not args.headless,
                                 prepare_shaders=not args.headless, scene_name=args.scene_name)
        if args.headless:
            close_scene(scene)
            return None
        return prepare_view(scene)
    # Match script import lookup at the CLI boundary. Library invocations retain
    # their caller's import path, including concurrently executing sources.
    import sys
    previous_path = sys.path[:]
    try:
        sys.path.insert(0, str(args.source.resolve().parent))
        view = asyncio.run(run())
    finally:
        sys.path[:] = previous_path
    if view is not None:
        try:
            if args.snapshot:
                view.snapshot(args.snapshot, time=args.time, width=args.width, height=args.height)
            elif args.mp4:
                result = view.export_mp4(args.mp4, fps=args.fps[0], fps_denominator=args.fps[1],
                                         width=args.width, height=args.height, start_frame=args.start_frame,
                                         frames=args.frames, end_time=args.end_time, hold=args.hold,
                                         max_frames=args.max_frames, overwrite=args.overwrite,
                                         ffmpeg=args.ffmpeg, crf=args.crf)
                print(f"Wrote {result.path}: {result.frames} frames at {result.fps}/{result.fps_denominator} fps")
            else:
                view.show(title=args.source.stem, width=args.width, height=args.height)
        finally:
            view.close()


if __name__ == "__main__":
    main()
