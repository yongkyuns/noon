"""Native CPython entry point for Noon's finite, sampled authoring profile.

The scene source is shared with Pyodide. The default host uses explicit logical
samples. Optional export uses native windowless capture and the shared output
policy. Optional text/resources,
spatial construction, full family animation/style and callback structural/content
producers remain outside this binding's qualified profile; they are not silently replaced or emulated.
"""
from __future__ import annotations

from _noon_native_host import run_scene, run_source, close_scene, export_scene, export_source


def main():
    import argparse
    import asyncio
    import json
    from pathlib import Path
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    parser.add_argument("--sample-hz", type=float, default=60.0,
                        help="Logical sample rate when no output is requested")
    parser.add_argument("--portable-source", action="store_true")
    parser.add_argument("--output", type=Path, help="Write native MP4 or a PNG sequence")
    parser.add_argument("--quality", help="Output quality: l, m, h, p or k (default h)")
    parser.add_argument("--resolution", help="Output width,height")
    parser.add_argument("--fps", help="Output FPS number or integer P/Q ratio")
    parser.add_argument("--format", help="Output format: mp4 or png")
    parser.add_argument("--width", type=int)
    parser.add_argument("--height", type=int)
    parser.add_argument("--start-frame", type=int, default=0)
    parser.add_argument("--max-frames", type=int, default=108000)
    parser.add_argument("--final-hold", type=float, default=0.0)
    parser.add_argument("--overwrite", action="store_true")
    parser.add_argument("--fallback", action="store_true", help="Require a fallback GPU adapter")
    parser.add_argument("--ffmpeg", type=Path)
    args = parser.parse_args()
    async def run():
        source = args.source.read_text(encoding="utf8")
        if args.output is not None:
            return await export_source(
                source, args.output, portable=args.portable_source, filename=str(args.source),
                quality=args.quality, resolution=args.resolution, frame_rate=args.fps,
                format=args.format, width=args.width, height=args.height,
                start_frame=args.start_frame, max_frames=args.max_frames,
                final_hold=args.final_hold, overwrite=args.overwrite,
                fallback=args.fallback, ffmpeg=args.ffmpeg,
            )
        scene = await run_source(source,
                                 sample_hz=args.sample_hz, portable=args.portable_source,
                                 filename=str(args.source))
        try:
            return scene.time
        finally:
            close_scene(scene)
    result = asyncio.run(run())
    if args.output is not None:
        print(json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    main()
