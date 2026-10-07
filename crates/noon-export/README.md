# Native capture and video output

`noon-export` is the native offscreen GPU/output integration boundary. It uses
Noon's shared runtime and retained WGPU renderer, not the native window host,
Python, JavaScript, a browser, or an alternate scene representation.

## Frame capture

`capture_frames` accepts a fresh Rust `LiveProgram`, its callback table, shared
`ExportFrameOptions`, native `CaptureOptions`, and a fallible frame consumer.
The consumer receives source-frame identity, rebased presentation timestamps,
requested and published authored time, and borrowed tightly packed RGBA pixels.

One renderer, target, staging buffer and CPU pixel buffer are reused per run.
Execution stays sequential; consumer latency never changes authored sample times.
Required endpoint/prefix publications are consumed but are not extra output frames.
Frame-aligned crops replay their stateful prefix. Explicit terminal holds freeze
final source edits without continuing updaters. Viewer inspection/selection is
excluded; authored camera, text/images, transients, insets and spatial composition
use the production renderer.

```sh
cargo run --release -p noon-export --example capture_frames > scene.rgba
```

This example streams the three-second Rust `FollowingGraphCamera` scene at
320x180 and 30 FPS. A successful capture summary alone does not finalize an encoder.

## H.264/MP4 export

Enable the optional `ffmpeg` feature. It adds no Rust package dependencies and
requires a trusted FFmpeg executable with libx264, rawvideo, scale, setparams and
MP4 support only when used. The codec/filter/muxer profile is exercised by a tiny
preflight encode before authoring callbacks start. Raw capture needs no FFmpeg.

```sh
cargo run --release -p noon-export --features ffmpeg \
  --example export_video -- scene.mp4
```

This uses the same Rust camera scene at 1280x720 and 30 FPS. The output must not
already exist. For other scenes/resolutions/rational frame rates, use
`noon_export::video::export_video(program, callbacks, frame_options,
capture_options, VideoOptions::mp4(path))`. The lower-level `Mp4Encoder` is a
consumer of that same `CapturedFrame` interface, not another scene executor.

Output indices and time bases are checked before each frame write. The raw input
rate and encoder/MP4 time bases preserve the rational schedule without an output
FPS conversion, display pacing, dropped samples or an implicit endpoint frame.
The finish operation checks the shared capture summary's count, closes input,
waits for delayed encoder packets and muxer finalization, and only then publishes
the final file. It is not a claim that every production export runs a decoder.

The default rejects existing destinations, including ones created concurrently
while encoding. `overwrite = true` explicitly permits replacing a regular file
only after success. Temporary output lives in a private sibling directory;
failed/abandoned exports never intentionally remove an existing destination.
Publication uses a same-filesystem hard link (no overwrite) or rename (explicit
overwrite), without a non-atomic copy fallback. Unsupported filesystems return an
error. Cleanup is best effort after filesystem failure; crash-durable directory
publication and hostile concurrent filesystem mutation are not promised.

The profile is opaque H.264/yuv420p. Dimensions must be positive and even; the
adapter does not silently resize or crop. Captured renderer RGB bytes are treated
as sRGB-coded, converted from full-range RGB to limited-range BT.709 YCbCr, and
marked with sRGB transfer / BT.709 primaries and matrix. This does not convert the
transfer curve to BT.709. Non-opaque input is rejected. HDR, wide-gamut output,
alpha video, audio and PNG sequence output are not implemented by this adapter.

The encoder's stderr is continuously drained with a bounded tail. Cancellation
and finite per-write/finalization deadlines terminate the owned child to release
pipe backpressure. Source/callbacks never move onto these supervision threads.
Only a trusted direct FFmpeg child is managed: this is not a process-tree sandbox,
and synchronous source code, GPU initialization and OS/filesystem calls are not
universally preemptible. Configure timeouts for the output size and chosen codec.

## Qualification

```sh
cargo test -p noon-export --all-features --lib --tests
# FFmpeg/libx264 and real/software Vulkan; no display server:
env -u DISPLAY -u WAYLAND_DISPLAY cargo test -p noon-export --all-features \
  --lib --test video -- --include-ignored --nocapture --test-threads=1
python3 scripts/qualify-video-export.py /path/to/NOON_VIDEO_PROOF_DIR
```

The native video CI job records the exact tested source, runs Rust process/file
and capture tests, then independently decodes videos with FFmpeg/ffprobe. It checks
integer and fractional rates, all presentation timestamps/durations, actual frame
count, color metadata and per-frame RGB error against fresh raw captures or fixed
color patches. Python is qualification tooling, not a dependency of native export.
The existing capture and workspace/native/browser/product gates remain required.
See #1896 and the owning PRs for actual results and outstanding qualification.
No physical-GPU or Manim export speedup is implied by these correctness tests.
