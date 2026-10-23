# Native scene export

`noon-export` is native GPU/output integration over the shared Rust engine and
retained WGPU renderer. It has no dependency on the window/event-loop host,
Python, JavaScript, or a browser. Native Python integration remains a separate
language binding task, not another exporter.

## Video and PNG output

A trusted installed FFmpeg build with `libx264`, PNG, MP4 and image2 support is
required for these file adapters. MP4 soundtrack export also requires its AAC
encoder and raw `f32le` input support. Capture-only use does not invoke FFmpeg.

```sh
cargo run --release -p noon-export --example render_video -- scene.mp4
cargo run --release -p noon-export --example render_video -- scene-images --png
```

The example exports the existing Rust `FollowingGraphCamera` program at 1280x720,
30 FPS. The library entry point `output::export_file` accepts a normal fresh
`LiveProgram`, callback table, `ExportFrameOptions`, `CaptureOptions` and
`OutputOptions`. Use `OutputOptions::mp4(path)` or `::png_sequence(path)`.

Frame indices and rational FPS drive the existing runtime, never wall time.
`30000/1001` and `60000/1001` are preserved. The MP4 track uses numerator ticks
per second and denominator ticks per frame. There is no realtime recording,
frame-dropping mode, output-rate conversion, or extra endpoint frame.

MP4 uses H.264, YUV420p, `veryfast`, CRF 18 by default and no B-frames. CRF is
configurable from 0 through 51. Even dimensions are required; odd dimensions are
rejected instead of being resized. PNG preserves every captured RGBA byte and
supports odd dimensions. The file adapters currently accept opaque input only.

## Finite scenes with a soundtrack

The native `output::export_session_file` entry point exports an existing finite,
immutable `ExecutionSession`, including its authored audio, to one MP4. It uses
the same `ExportFrameOptions`, `CaptureOptions`, `OutputOptions` and
`FileExportSummary` as the live-program entry point:

```rust
export_session_file(&mut session, frame_options, capture_options, output_options)
```

Its error type is `SessionFileExportError`. Admission requires a session at time
zero with a known finite immutable timeline and no required callbacks, pending
segment completion or retained source history. Lower an authored scene with `Scene::execution_session()`
or `Scene::execution_session_with_animation_root(...)` before calling it. The
exporter borrows that session; it advances the existing runtime, consumes its
publications through `SessionCapture`, and follows `ExportFramePolicy` for the
same prefix, output and completion samples used by other export hosts. A finite
range preflight determines staging size without running the scene a second time;
the completed sampling summary must match that plan before publication.

The executable background-music example renders the shared Rust scene at
1280x720, 30 FPS, for 30 seconds:

```sh
cargo run --release -p noon-export --example render_audio_video -- scene-with-music.mp4
```

It uses *A Small Discovery*, the project's quiet original score. The composition
and generated audio carry the scoped CC0 dedication in
[`render_background_music/README.md`](../noon/examples/render_background_music/README.md).
Its synthesis uses no external recording, sample, melody, model or service.
The visible playback buttons are authored scene objects and appear in the
rendered movie; an MP4 itself does not provide their interactive scene behavior.

Authored immutable PCM is mixed by the shared Rust runtime into 48 kHz stereo,
then encoded as AAC at 192 kbit/s alongside H.264 in one FFmpeg encode. There is
no separate manual audio postprocessing step. Mixing uses each clip's authored
start, trim, loop, gain and fades at absolute scene time. Source rates use linear
interpolation; mono is duplicated into the two output channels and stereo is
preserved. Overlapping clips sum together. A nonfinite mixed sample or a final
mixed value outside `[-1, 1]` fails the export so the author can lower gains;
the mixer does not silently clip. Viewer volume, mute and realtime output
transition ramps do not change the soundtrack.

Audio starts at the same source-grid crop origin as the video. Gaps, exhausted
clips and the part of a final frame after an explicit requested end are silent.
An explicit frozen terminal hold adds no new authored sound after clips end.
The shared finite endpoint already includes authored audio: a clip that outlasts
an animation keeps the final picture visible until that clip ends. Audio staging
and muxing do not add frames to or shorten this planned output range.
Natural, unheld completion preserves the supported exact video endpoint by
finalizing the last sample duration. Explicit stops and final holds keep the
existing full video-grid extent. Soundtrack finalization handles the qualified
one-video/one-audio MP4 layout, preserving AAC priming on a common movie clock.
Audio presentation ends on the last complete 48 kHz sample at or before the exact
video end. Unsupported timing layouts fail explicitly. Known clock/duration
overflows in the qualified version-zero MP4 fields are rejected before staging.

PCM is staged privately before encoding, in blocks of at most 16,384 audio
frames. Audio memory stays bounded by those blocks, and the video pipe retains
its existing two-frame buffer pool. The temporary stereo float file uses
384,000 bytes per second of scheduled video, in addition to the encoded output.
Cancellation is checked between blocks and before publication. This uses a file
input for audio rather than another live encoder pipe, and successful or failed
runs clean their own staging through the existing destination owner.

AAC decoders may return complete 1,024-sample packets beyond the presentation
endpoint. Raw decoded byte count is therefore not the movie-duration oracle:
container/edit-list and packet presentation times establish the end, while
decoded signal checks establish alignment. Audio presentation may end less than
one 48 kHz sample before video; it cannot extend the reported movie duration.
An interval shorter than one audio sample can contain no presented audio frames.

PNG sequences are explicitly visual-only and omit authored audio. The existing
`export_file` live-program MP4 path still rejects audio until live execution is
qualified. Finite Python scenes can use the browser capture and native media
import described below. In-process native CPython bindings, live/history audio
export and narration remain separate work tracked by #1874 and #2263.

## Finite Python scenes and prepared media import

The complete finite Python profile runs the normal Pyodide authoring worker,
shared Rust execution session and browser retained renderer. The worker owns
the only runtime. Shared `ExportFramePolicy` selects each source sample; the host
waits for that publication's renderer completion before copying its canvas
pixels and acknowledging it. The shared Rust mixer supplies the soundtrack on
the same absolute cropped grid. JavaScript copies media and transports receipts;
it does not plan animation frames or reconstruct a scene.

Install the browser-test Playwright dependency and Chromium. Build the browser
package and export from the repository root:

```sh
bash scripts/build-web-demo.sh
node scripts/export-python-scene.mjs \
  web/python/examples/background_music_lesson.py /tmp/noon-python-capture \
  --mp4 scene-with-music.mp4
```

The default is 640x360 at 30 FPS, WebGPU and transferable worker transport. The
30-second example is paired with the first-class Rust scene exported above.
`--fps 30000/1001`, `--start-frame N`, `--frames N`, `--end-time T` and `--hold T`
map to the existing shared frame options. `--frames` and `--end-time` are mutually
exclusive; omitting both uses the finite source end. `--max-frames` defaults to
18,000 and bounds the full source grid, including discarded prefix work.
`--backend webgl` and `--transport shared` select the existing browser paths.
Run `--help` for all options.

The source must produce a complete finite immutable `result = Scene()` at time
zero. Callbacks, live source continuations and retained source history are
rejected before export. Completion must be retained by a visual/scalar track or
audio clip. A trailing authored wait beyond those retained endpoints is rejected
explicitly instead of silently shortening the movie; a general completed
authoring horizon remains outside this finite profile. The script compares the
canonical authoring duration with the shared runtime's admitted endpoint without
overriding either. Scene playback controls cannot mutate an active export.
The browser is the pixel producer; this path makes no claim that native WGPU
rendered Python's frames or that a native CPython binding exists.

Omit `--mp4` to retain only the prepared capture, or supply `--encoder PATH` to use
an already built `encode_prepared_capture` executable. Without that option, the
script invokes the native example through Cargo. The same completed media can
be encoded separately, without Python, WASM or a browser in the native process:

```sh
cargo run --release --locked -p noon-export --example encode_prepared_capture -- \
  /tmp/noon-python-capture scene-with-music.mp4
```

The library entry point is `output::export_prepared_capture(directory,
output_options, limits, cancellation)`. It returns `PreparedCaptureSummary`,
which certifies native encoding and destination publication. It accepts MP4 only
and uses the existing `FileSink`, encoder process and exact terminal-duration
finalization. It does not instantiate an execution session or remix received
PCM. Both commands require a new MP4 destination unless `--overwrite` is
explicit. The capture directory must always be new.

### Capture format and timing

A completed `noon-prepared-capture-v1` directory contains:

| File | Contents |
| --- | --- |
| `capture.json` | Dimensions, exact rational frame options, admitted finite source end, audio presence and every policy observation. |
| `frames.rgba` | Tightly packed, top-down, opaque RGBA8 frames in rebased output PTS order. No header or per-row padding. |
| `audio.f32le` | When audio is present, interleaved 48 kHz stereo little-endian float32 samples from the shared mixer. |

The JSON shape is explicit and rejects unknown fields:

```json
{
  "format": "noon-prepared-capture-v1",
  "width": 640,
  "height": 360,
  "source_end": 30.0,
  "frame_options": {
    "frame_rate": { "numerator": 30, "denominator": 1 },
    "start_frame": 0,
    "stop": { "kind": "frame_count", "frames": 2 },
    "max_frames": 18000,
    "max_transitions_per_sample": 64,
    "final_hold_seconds": 0.0
  },
  "audio": true,
  "samples": [
    {
      "requested_time": 0.0,
      "published_time": 0.0,
      "publication": {
        "scene_revision": "1", "execution_revision": "1", "frame_epoch": "0"
      }
    },
    {
      "requested_time": 0.03333333333333333,
      "published_time": 0.03333333333333333,
      "publication": {
        "scene_revision": "1", "execution_revision": "1", "frame_epoch": "1"
      }
    },
    {
      "requested_time": 0.06666666666666667,
      "published_time": 0.06666666666666667,
      "publication": {
        "scene_revision": "1", "execution_revision": "1", "frame_epoch": "2"
      }
    }
  ]
}
```

`stop` can instead be `{ "kind": "source_end" }` or
`{ "kind": "end_time", "time": T }`. Revision and epoch values are canonical
unsigned decimal strings so all 64 bits survive JavaScript transport. The
values above illustrate the format; producers record the runtime's actual
publication identities.

Every prefix, output and completion observation is required in policy order.
Only output observations have RGBA bytes. Thus this two-frame example contains
three observations, two images and 3,200 stereo audio frames. The importer
replays the shared policy, requires the exact requested and published times,
rejects missing or extra receipts, and freezes publication identity during
terminal holds. Scene and execution revisions stay fixed; a changed published
time requires a newer frame epoch. Receipt validation checks the media contract,
but cannot prove that an arbitrary producer actually rendered those pixels.

For `F` output video frames at rational rate `N/D`, the PCM file contains exactly
`ceil(F * D * 48000 / N)` audio frames, or eight times that many bytes. Shared
`ExportAudioGrid` computes this with integer arithmetic. Audio frame `k` samples
absolute scene time `start_time + k / 48000`; cropping does not restart clips or
fades. Samples at or after the policy's exclusive end are zero. The native sink
applies the same natural-end versus explicit-stop/hold timing rules described
above, including exact final partial video duration and AAC presentation bounds.
The importer requires finite PCM in `[-1, 1]`, correct silent padding, exact file
lengths and opaque frames. With `audio: false`, an `audio.f32le` file is rejected.

### Bounds, completion and cleanup

Default `PreparedCaptureLimits` admit at most a 16 MiB manifest, 1,000,000
observations, 256 MiB per RGBA frame and 64 GiB of total raw media. The browser
script also limits Python input to 8 MiB. The native importer retains one input
frame, the existing two encoder-pipe buffers and one block of at most 16,384 PCM
frames; it does not load the whole movie into memory. Raw RGBA capture still
requires `4 * width * height * F` bytes on disk, plus the PCM file.

The producer closes its media files and writes `capture.json` last, then leaves
the directory immutable while importing. It removes its own incomplete capture
on ordinary failure or cancellation. A completed capture remains available if
native encoding fails. Native encoding stages PCM and the movie under the
existing destination transaction, preserving an existing output on failure and
checking cancellation between media blocks and frames. No success is reported
until encoder flush, timing finalization and destination publication complete.
These files are an explicit export/import boundary containing media and sample
metadata; they are not a persisted scene or an alternate runtime authority.

## Publication and failures

MP4 is written inside a private same-filesystem staging directory and published
only after successful input EOF, encoder flush/exit and file synchronization.
The default no-clobber hard-link operation also protects a path created after
preflight. Filesystems without hard-link support fail safely. Explicit
`overwrite = true` uses rename, never remove-then-rename. Existing files survive
capture, audio-mixing, timing-finalization or encoder failures. Symlinks and
directories are not overwrite targets.

A PNG sequence exclusively reserves a **new bundle directory**. During capture,
only `.incomplete/` exists inside it. After encoder success, `frames/` appears
with numbered PNGs and `timing.tsv` in one directory rename. The TSV records
rational FPS, dimensions, PTS, source index, requested/published time and holds.
Its final line certifies the completed frame count. Existing directories are
never overwritten. Failed runs clean their own staging/reservation; process
crashes may leave an explicitly incomplete bundle. Cleanup is best effort and
neither profile promises crash durability of directory entries or protection
against external mutation of its private working directories.

A real one-frame codec/muxer probe runs before user source execution. Its image
is discarded, not inserted into authored output. Missing/unsupported FFmpeg
builds fail explicitly. Stdout is unused, stderr is continuously drained with a
64 KiB retention limit, and writes handle partial pipe writes. Two preallocated
owned input buffers overlap encoder pipe writes with subsequent capture. A full
pool applies backpressure instead of growing or dropping frames. Source and
callbacks stay on the caller thread and need not implement `Send`.

Queue acceptance is distinct from I/O completion. Finalization collects a
successful, ordered write receipt for every submitted frame before sending EOF.
A writer failure cannot be hidden by a successful encoder exit. The one-frame
capability probe uses the synchronous path through the same transport.

The transport stores at most two frames (8 x width x height bytes), in addition
to the existing capture buffers and encoder-process memory. It adds no source
history or GPU submissions in flight. A failed encoder can be discovered after
later source work has already run; the whole export is then abandoned, never
retried against the consumed runtime state.

Cancellation and per-write/finalization deadlines kill the encoder to unblock
stalled pipes. OS process termination/reaping, blocking filesystems, GPU driver
calls and synchronous user callbacks are not hard-preemptible. Supply a trusted
FFmpeg executable, not an arbitrary process spawning descendants that retain
its pipe handles. A failed or cancelled run cannot be resumed as a fresh source.

A successful `FileExportSummary` means capture, encoder finalization and output
publication succeeded. Full decoding is performed by qualification, not repeated
inside every production export. The lower-level `capture_frames` summary alone
still does not certify encoder finalization.

## SDR interpretation

The file adapter interprets the current renderer's encoded UNORM RGB values as
sRGB-transfer, BT.709-primary RGB. MP4 explicitly converts full-range RGB to
limited-range BT.709 YCbCr and tags the preserved **sRGB** transfer; it does not
merely relabel those samples as the different BT.709 transfer. PNG preserves
capture bytes without transfer or alpha conversion. No HDR, alpha-video or
wide-gamut profile is claimed. Broader color-management/player qualification
remains part of #1896; lossy video is compared with explicit pixel tolerances,
not bit-identical expectations.

## Capture-only use

`capture_frames` gives a fallible consumer borrowed tightly packed RGBA pixels
and coherent sample/publication metadata. One renderer, target, staging buffer
and CPU capture buffer persist per run. The initial GPU capture is serial;
required endpoint/prefix publications are rendered but not output. It uses the
production camera, indexed visibility, text/image/transient/inset/spatial path,
without viewer inspection or selection overlays.

```sh
cargo run --release -p noon-export --example capture_frames > scene.rgba
```

This raw example is 320x180 at 30 FPS. File export overlaps encoding with the
next serial capture; GPU readback itself still waits for each submission.
GPU readback pipelining and matched Manim measurements remain later slices.
No numerical throughput improvement is claimed without measurement.

## Qualification

```sh
cargo test -p noon-export --all-features --lib --tests
mkdir -p output-proof/media
NOON_OUTPUT_PROOF_DIR="$PWD/output-proof/media" \
  env -u DISPLAY -u WAYLAND_DISPLAY cargo test -p noon-export --all-features \
    --lib --tests -- --include-ignored --nocapture --test-threads=1
python3 scripts/verify-native-export.py output-proof/media
```

Selected CI runs real/software Vulkan without a display server. It checks actual
encoded frame counts/PTS/duration/rational rate, independently decodes all proof
videos, compares the PNG sequence byte-for-byte against a fresh native capture,
and retains media, logs, tool versions and tested revision. Process and file
failure tests are separate from visual/timing checks. Existing architecture,
workspace, native viewer, browser and product gates are not waived.

## Session-owned native hosts

`SessionCapture` is the capture-only entry point for a native host that already
owns an `ExecutionSession`, rather than a Rust `LiveProgram`. It uses the same
native GPU capture, retained composition and readback code as `capture_frames`.
It does not run Python, drive a timeline, invoke callbacks or encode a file.

Construct it with `SessionCapture::new(&session, capture_options)`, then call
`capture(&mut session)` only after the shared runtime has settled the requested
sample. The result borrows tightly packed RGBA pixels and reports the actual
publication context/time. It deliberately does not invent video timestamps.
`render(&mut session)` consumes a publication without reading pixels; it can
satisfy an off-grid render barrier without creating an extra output frame. The
source owner still performs the existing endpoint-admission/continuation steps.

A capture instance is bound to one runtime identity. A different runtime, an
unsettled callback, cancellation or a render error fails explicitly and poisons
that instance. Capturing cannot change scene time, complete a callback or admit a
source continuation. Repeated captures of a settled state are just repeated
images, not animation playback. A consumer must copy borrowed pixels to retain
them beyond the next capture.

This typed API remains a prerequisite for in-process native language-binding
integration. The finite Pyodide path described above captures through the normal
browser renderer and imports finished media into the native sink; it does not
use `SessionCapture` or replace a native CPython binding.
