# noon-python

Native CPython binding to the shared Noon authoring and execution API.

This package belongs to [Noon](https://github.com/yongkyuns/noon), a shared Rust
animation engine with native, Python and browser hosts. Ordinary scene authors
start with the public authoring facade and a native or browser host; supporting
crates retain their existing compilation and dependency boundaries.

## Release status

The 0.1.0 artifacts are local release candidates. Registry names remain undecided;
the existing `noon` packages on crates.io and PyPI belong to unrelated projects.
Do not install those packages expecting this engine. Follow
[release readiness #2433](https://github.com/yongkyuns/noon/issues/2433) for the
qualified artifact set and eventual installation names.

The supported build toolchain is Rust 1.98.1 or newer. Native Python and browser
support are qualified separately from the native Rust library. See the repository
README for the supported feature and host profiles.

## Native preview, capture and MP4

The installed `noon` command and `python -m noon_native` use the same native
Python host. For a source file containing an ordinary `Scene.construct`, use:

```bash
noon first_lesson.py
noon first_lesson.py --snapshot lesson.png --time 1.75 --width 1280 --height 720
noon first_lesson.py --mp4 lesson.mp4 --fps 30000/1001 --width 1280 --height 720
noon --version
```

Until registry names are selected, install the reviewed wheel directly rather
than requesting the unrelated package by name. For development from a checkout:

```bash
python scripts/build-native-python.py --profile dev
PYTHONPATH=build/python python -m noon_native web/python/examples/first_lesson.py --mp4 lesson.mp4
```

MP4 export requires an installed FFmpeg executable with the `libx264` encoder
and a native GPU adapter supported by Vulkan, Metal or DX12. It needs no window,
display server, Node, Chromium or Playwright. `--ffmpeg /path/to/ffmpeg` selects
an explicit executable. Geometry-only extension builds omit capture and export.

Python source executes once and prepares the existing bounded deterministic
Rust history. The native renderer samples that history with Noon's shared output
frame policy. Rendering or encoding speed does not change authored time, skip
frames or execute Python again. MP4 uses SDR H.264/YUV420p and requires even
dimensions. Fully predeclared scenes can export their authored soundtrack as
48 kHz stereo AAC. Export requires deterministic source timing. For native
window playback with audio, build with `python scripts/build-native-python.py
--audio`. This enables the same device backend as the Rust window.

### Native LaTeX

`Tex`, `MathTex`, and DecimalNumber labels use the system `latex` and
`kpsewhich` executables. Set `NOON_LATEX_FONT_DIR` to the BaKoMa TTF directory
and `NOON_LATEX_RESOURCE_IDENTITY` to a stable identifier for that compiler
and font installation. The optional `NOON_LATEX_EXECUTABLE` and
`NOON_KPSEWHICH_EXECUTABLE` variables override executable discovery. The
compiler is initialized on first use, including an explicit
`await prepare_latex()`.

### Output controls

- `--fps 30` or `--fps 30000/1001` selects the exact rational output frame grid.
- `--start-frame N` crops at source-grid frame N. Earlier samples are evaluated
  and consumed before output begins; the movie's presentation timestamps start
  at zero.
- `--frames N` or `--end-time SECONDS` adds an exclusive sample bound. Natural
  source completion can end the output earlier. These flags are mutually exclusive.
- `--hold SECONDS` adds an explicit frozen terminal hold without rerunning
  callbacks or extending authored animation. Without a hold, natural completion
  preserves an off-grid source endpoint in the last MP4 sample duration.
- `--max-frames N` bounds all grid samples, including a discarded crop prefix and
  holds. The default is 108000; exhausting this cap is an error, not a successful
  partial movie.
- `--crf N` selects H.264 quality from 0 to 51; the default is 18.
- Existing output is preserved unless `--overwrite` is supplied. Even then,
  replacement occurs only after successful rendering, encoder finalization and
  atomic destination publication. Cancellation and failures clean private staging.

`--headless` executes and validates without GPU output. It is mutually exclusive
with `--snapshot` and `--mp4`. Use `--scene ClassName` when a source defines more
than one scene.

### Python API

```python
import asyncio
from pathlib import Path
from noon_native import prepare_view, run_source

scene = asyncio.run(run_source(Path("first_lesson.py").read_text(), retain_history=True))
view = prepare_view(scene)
try:
    result = view.export_mp4("lesson.mp4", fps=30000, fps_denominator=1001, width=1280, height=720)
    print(result.path, result.frames)
finally:
    view.close()
```

The result contains `path`, `frames`, `fps`, `fps_denominator`, `start_time`,
`end_time`, `source_end` and `reason` (`source_end` or `requested_stop`). It is
returned only after the file is published. Once export takes ownership, the view
is consumed on success or failure; prepare a new source instance for another
export. Basic argument conversion and contradictory stop options are rejected
before ownership transfer. Calling `close()` afterward remains safe.

An optional `ExportCancellation` from `noon_native` can be passed as
`cancellation=token`; another Python thread may call `token.cancel()`. Python
signals such as Ctrl-C are checked between samples and before publication,
including after encoder finalization. Cancellation is cooperative; GPU and
encoder waits keep their configured finite deadlines.

Real output qualification is run with
`PYTHONPATH=build/python python scripts/test_native_python_export.py`. It inspects
the encoded frame count, rational timestamps, terminal duration and decoded
motion/color; it also covers cropping, holds, once-only source execution,
preserved destinations and the installed-style CLI path.

## License

MIT. See LICENSE. Third-party resources retain their original notices.
