# Native scene export

`noon-export` is native GPU/output integration over the shared Rust engine and
retained WGPU renderer. It has no dependency on the window/event-loop host,
Python, JavaScript, or a browser. Native Python integration remains a separate
language binding task, not another exporter.

## Video and PNG output

A trusted installed FFmpeg build with `libx264`, PNG, MP4 and image2 support is
required for these file adapters. Capture-only use does not invoke FFmpeg.

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

## Publication and failures

MP4 is written inside a private same-filesystem staging directory and published
only after successful input EOF, encoder flush/exit and file synchronization.
The default no-clobber hard-link operation also protects a path created after
preflight. Filesystems without hard-link support fail safely. Explicit
`overwrite = true` uses rename, never remove-then-rename. Existing files survive
capture/encoder failures. Symlinks and directories are not overwrite targets.

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
64 KiB retention limit, and writes handle partial pipe writes. One reusable
owned input buffer crosses the encoder writer thread. The source/callbacks stay
on the caller thread and need not implement `Send`. No frame history is stored.

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
capture bytes without transfer or alpha conversion. No HDR, audio, alpha-video
or wide-gamut profile is claimed. Broader color-management/player qualification
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

This raw example is 320x180 at 30 FPS. Pipelined GPU readback/encoding overlap and
matched Manim performance measurements remain later slices; no speedup is claimed.

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

This API is a prerequisite for native language-binding integration, not a claim
that native Python or Pyodide can already export video. Those bindings still need
the shared rational sample/completion contract and file-sink integration; a
Python-side sampling loop or browser fallback is not supplied here.
