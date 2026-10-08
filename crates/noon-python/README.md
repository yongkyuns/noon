# Native CPython execution and optional video export

The native binding is an optional wrapper over the shared Rust engine. It does
not broaden the existing native analytic authoring capability profile. Unsupported
text, image, spatial and structural/content-callback operations still fail rather
than being replaced with a browser or a second renderer.

## Build and use

```sh
python3 scripts/build-native-python.py --profile release --export-video
PYTHONPATH=build/python python3 -m noon_native scene.py --output scene.mp4 --fps 60000/1001
```

FFmpeg must be installed with the required H.264/MP4 or PNG/image2 support. The
build flag is optional; the default native binding still excludes GPU/encoder
integration. The ordinary execution-only CLI remains available without that flag.

Programmatic use:

```python
import asyncio
from noon import Scene, Square
from noon_native import export_scene

class Demo(Scene):
    async def construct(self):
        marker = Square()
        self.add(marker)
        await self.wait(0.105)
        marker.shift([1, 0, 0])
        await self.wait(0.207)

summary = asyncio.run(export_scene(Demo, "demo.mp4", fps=(30, 1)))
```

`export_source(source, destination, ...)` uses the existing native source loader;
its original synchronous and supported portable/async source modes are unchanged.
The source must create exactly one exported scene context. Nested exports are
rejected. Failed source/module/constructor/callback execution abandons the output.
All acquired source/callback handles are retired on exit, including after success.

## Contract

Output frame selection, rational timestamps, frame-aligned crop replay and final
holds use `ExportFramePolicy`, also used by compiled Rust. Python executes only
its source and genuinely required user callbacks; it does not iterate video
frames. The existing callback transaction protocol remains unchanged. A segment
endpoint is consumed before resuming source; same-time source edits settle before
an output frame is captured. Static waits produce all scheduled video frames.

This first binding exports to natural source completion, with an explicit
`max_frames` safety cap (including the discarded crop prefix). Exceeding the cap
fails; it is not silent truncation. `start_frame` crops on the zero-origin grid.
`final_hold` freezes the terminal publication without running more callbacks.
The broader Rust API's explicit non-grid end-time/intentional-count stop options
are not exposed by this initial Python entry point.

A finite per-request transition budget bounds repeated segment/callback handoffs.
Python signal checks permit cooperative interruption between native operations;
this is not preemption of arbitrary Python code, driver calls or filesystem work.
GPU readback and encoder finalization retain the shared native failure and file
publication contracts. Output is opaque SDR; MP4 requires even dimensions, while
PNG accepts odd dimensions. Audio, browser/Pyodide encoding, zero-copy, arbitrary
host seeking and universal player/backend color equivalence are not included.

The native video conformance workflow runs real CPython plus the same retained
renderer and FFmpeg, compares PNG pixels with a compiled Rust counterpart, checks
callback/crop/hold equivalence, decodes every MP4 frame and verifies rational PTS,
and tests source failure and handle cleanup. A published entry point is not a
passing qualification result; consult its exact source revision's CI.
