# Native frame capture

`noon-export` is the native offscreen GPU/output integration boundary. It depends
on the shared Noon engine and retained WGPU renderer, not on `noon-native`'s
window/event loop or any Python/browser host. The separate package keeps native
capture usable without a window-system dependency and keeps GPU/process output
out of the language-neutral authoring/runtime crates.

`capture_frames` accepts a fresh `LiveProgram`, its Rust callback table, the shared
`ExportFrameOptions`, native `CaptureOptions`, and a fallible frame consumer.
The consumer receives requested/published time, source index, rebased PTS and
borrowed tightly packed RGBA bytes. Callback and consumer closures need not be
`Send`. The source remains sequential; consumer latency cannot change scene time.

One renderer, target, staging buffer and CPU output buffer are reused during a
run. The initial implementation waits for each GPU submission before continuing.
Required semantic-endpoint and prefix publications are rendered but not output.
An explicit final hold freezes the terminal image without executing updaters.
The shared range policy currently supports frame-aligned crop starts only.

Capture uses the full production retained composition path, including effective
authored camera, indexed visibility, text/images, transient presentations,
secondary views and spatial passes. Viewer inspection and selection are excluded.
The initial output profile is opaque, top-down `Rgba8Unorm` renderer bytes. This
is not an implicit linear/sRGB, video range/matrix, or alpha-video conversion.

## Raw output example

The example streams the existing Rust `FollowingGraphCamera` program at 320x180,
30 FPS to stdout. Diagnostics go to stderr. No window or browser is started:

```sh
cargo run --release -p noon-export --example capture_frames > scene.rgba
```

For a diagnostic video, use an FFmpeg build with libx264:

```sh
set -o pipefail
cargo run --quiet --release -p noon-export --example capture_frames | \
  ffmpeg -nostdin -n -f rawvideo -pixel_format rgba -video_size 320x180 \
    -framerate 30 -i pipe:0 -an -c:v libx264 -pix_fmt yuv420p scene.mp4
```

This pipeline is an explicit external consumer, not the planned production
encoder adapter. Encoder capability checks, color qualification, independent
video validation and atomic file finalization remain #1896 P3/P6. In particular,
a nonzero pipeline exit must not be treated as a successful exported file.

`CaptureCancellation` is cooperative. Each GPU wait has a finite timeout, but
GPU initialization, synchronous source callbacks and consumer calls cannot be
preempted by this API. Errors abandon the run rather than retrying consumed
publications. Start a fresh source/run after failure. A successful capture summary
does not certify that an external encoder has flushed or finalized its file.

## Qualification

```sh
cargo test -p noon-export --all-features --lib --tests
# Requires a real/software Vulkan device; no virtual display is used:
env -u DISPLAY -u WAYLAND_DISPLAY cargo test -p noon-export --all-features \
  --lib --tests -- --include-ignored --nocapture --test-threads=1
```

GPU assertions cover odd dimensions, first/last frames, stateful crops, delayed
consumers, terminal holds, camera changes, the shared text/image/zoomed-view
example, spatial depth, cancellation, consumer errors and device loss. The Native
Host Smoke workflow selects these tests and retains pixel evidence. Test results
and remaining qualification are tracked in #1904 and #1896, not inferred from the
presence of test code. No export speedup over Manim is claimed here.
