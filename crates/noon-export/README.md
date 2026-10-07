# Native frame and video export

`noon-export` is the native offscreen GPU/output boundary. It uses Noon's shared
semantic/runtime path and retained WGPU renderer without depending on
`noon-native`'s window event loop, Python, WASM, or browser infrastructure.

## Built-in output paths

`capture_frames` is the low-level frame consumer API.

`capture_png_sequence` writes numbered PNGs into a sibling scratch directory and
publishes the directory only after every frame succeeds.

`capture_mp4_ffmpeg` streams exactly one raw RGBA frame per shared output PTS to
an external FFmpeg process using H.264/libx264 and yuv420p. The shared rational
frame rate is passed directly to the rawvideo input. FFmpeg stderr is drained
continuously with bounded retention; stdin is closed and the process/muxer must
finish successfully before the temporary MP4 is published.

Existing output is preserved unless overwrite is explicitly enabled. Failed
capture, encoding, or muxing removes scratch output instead of publishing a
partial file.

The current video profile requires positive even dimensions and an FFmpeg build
that advertises `libx264`. There is no realtime pacing or screen recording.

### MP4

```sh
cargo run --release -p noon-export --example export_mp4 -- scene.mp4
```

### PNG sequence

```sh
cargo run --release -p noon-export --example export_png -- frames
```

### Raw RGBA

```sh
cargo run --release -p noon-export --example capture_frames > scene.rgba
```

The output clock is independent of rendering speed. Semantic endpoint
publications and cropped-prefix samples are consumed by the retained renderer but
are not emitted as video frames. Explicit terminal holds freeze the final image
without invoking updaters again.

## Color and alpha

The capture profile is opaque, top-down `Rgba8Unorm` renderer bytes over the
configured background. PNG stores those bytes directly. FFmpeg performs the
selected H.264/yuv420p conversion. Broader HDR, alpha-video, and cross-platform
color-management profiles remain future work.

## Qualification

```sh
cargo test -p noon-export --all-features --lib --tests
env -u DISPLAY -u WAYLAND_DISPLAY cargo test -p noon-export --all-features \
  --lib --tests -- --include-ignored --nocapture --test-threads=1
```

Native CI additionally installs FFmpeg, decodes the generated MP4, checks exact
decoded frame count, rational timestamps, dimensions/rate, validates PNG output,
and retains output evidence. Performance versus Manim remains P6 work.
