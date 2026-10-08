"""Native CPython entry point for Noon's finite, sampled authoring profile.

The scene source is shared with Pyodide. Execution-only builds are renderer-free;
--output uses the optional native export feature, never a realtime window. Optional text/resources,
spatial/family construction and callback structural/content producers are outside
this binding's initial profile; they are not silently replaced or emulated.
"""
from __future__ import annotations

from _noon_native_host import run_scene, run_source, close_scene, export_scene, export_source


def main():
    import argparse
    import asyncio
    from pathlib import Path
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    parser.add_argument("--sample-hz", type=float, default=60.0)
    parser.add_argument("--portable-source", action="store_true")
    parser.add_argument("--output", type=Path, help="Native video or PNG bundle destination")
    parser.add_argument("--fps", default="30/1", help="Exact numerator/denominator for export")
    parser.add_argument("--width", type=int, default=1280)
    parser.add_argument("--height", type=int, default=720)
    parser.add_argument("--max-frames", type=int, default=108000)
    parser.add_argument("--png", action="store_true")
    args = parser.parse_args()
    async def run():
        if args.output is not None:
            try:
                rate = tuple(int(v) for v in args.fps.split("/"))
                if len(rate) != 2 or min(rate) <= 0:
                    raise ValueError
            except ValueError:
                parser.error("--fps must be a positive integer numerator/denominator")
            result = await export_source(args.source.read_text(encoding="utf8"),
                                         args.output, portable=args.portable_source,
                                         filename=str(args.source), fps=rate,
                                         width=args.width, height=args.height,
                                         max_frames=args.max_frames, png=args.png)
            print(f"Finalized {result['frames']} frames: {result['path']}")
            return result
        scene = await run_source(args.source.read_text(encoding="utf8"),
                                 sample_hz=args.sample_hz, portable=args.portable_source,
                                 filename=str(args.source))
        try:
            return scene.time
        finally:
            close_scene(scene)
    asyncio.run(run())


if __name__ == "__main__":
    main()
