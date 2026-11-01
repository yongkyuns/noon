# Noon WebGPU demo

The direct Rust/WASM examples author, lower, execute, and render through typed Rust boundaries in one WASM context. The optional Pyodide authoring worker uses the same shared semantic operations; its separate execution worker is an explicit cross-context transport boundary.

From the repository root:

```bash
bash scripts/build-web-demo.sh
python3 -m http.server --directory web 8080
```

Then open <http://localhost:8080> in a WebGPU-capable browser. The JavaScript `requestAnimationFrame` timestamp is converted to deterministic scene time in Rust; JavaScript only owns browser scheduling and canvas sizing.

## Visual design and animated explanations

Two showcase scenes put the existing effects to work in complete compositions:

- **Interference is addition** (`/?catalog=showcase&example=showcase-wave-interference`)
  ([source](python/examples/showcase_wave_interference.py)) draws two analytic
  wave fields and their signed sum on the same color scale. One shared shader
  material shades three persistent rectangles; ordinary typed parameter tracks
  animate phase without a Python updater or a grid of animated cells. This is
  an analytic illustration, not a numerical wave simulation.
- **Make it move** (`/?catalog=showcase&example=showcase-editorial-cover`)
  ([source](python/examples/showcase_editorial_cover.py)) combines editable type,
  animated radial color, seeded procedural grain and a frosted caption into an
  editorial cover. The backdrop blurs the artwork beneath the caption while
  keeping its text sharp.

Both use the gallery's existing playback, seek and restart controls. They show
procedural surfaces and ordinary animation working together in browser-ready
lessons and motion design; they do not establish a speed advantage over Manim.

## Lesson key points

Use `self.next_section("Name")` between ordinary `play` and `wait` calls to
annotate the current authored time. After a replayable lesson finishes, the
viewer shows cue ticks, the active cue name/index, Previous/Next, and a named
jump menu. Click a timeline marker to jump to its exact cue; hover for its name
and time. The markers also support native button keyboard activation. These
controls work with or without audio; the wave-interference and editorial-cover
demos include examples.

Tab to the playback panel itself for keyboard controls: **Space** plays/pauses,
**Left/Right** navigate cues, and **Home** restarts. Focused buttons, menus,
sliders and the Python editor retain their normal keys.

Previous/Next use the nearest boundary strictly before/after the playhead,
without wrapping or an epsilon. Same-time cues use the first declaration;
duplicate names and skip sections remain available in the jump menu. Explicit
jumps preserve each cue's exact timestamp. Rust owns the navigation queries
and playback clock; the viewer displays their observations. Live source,
nonreplayable and open prepared-history profiles retain their existing seek
gates. These navigation cues are separate from interpolation keyframes.

## Background audio and scene navigation

Open <http://localhost:8080/background-audio.html> for the direct Rust/WASM audio
example. **Play with sound** unlocks browser audio after a user gesture. Pause,
restart, the time slider, previous/next controls, and the three named key points
seek picture and music together. Viewer mute and volume do not change the
authored scene or its clip gains. Preparation is silent; closing or leaving the
page retires the output.

The shared builder is `noon::example_scenes::background_audio::scene()`. It
declares immutable PCM with `Scene::audio(resource, AudioClipOptions { ... })`
and section markers at 0, 10, and 20 seconds. Clip start time is absolute scene
time; declaring audio does not advance the authoring cursor. Lower the returned
animation with `scene.execution_session_with_animation_root(&animation)`.
`scene.section_navigation()` provides immutable marker metadata for a host's
controls. The existing runtime and direct host clock own playback time; the
Web Audio adapter only consumes shared Rust playback descriptors.

The 30-second example uses an original synthesized score, **A Small Discovery**,
whose composition and generated audio are dedicated under **CC0-1.0**. It uses
no external recordings or samples. The generator remains MIT-licensed source;
see [the music example](../crates/noon/examples/render_background_music/README.md)
for provenance and offline rendering instructions.

The audible profiles accept finite, predeclared direct Rust/WASM scenes and
complete Python scenes returned as `result = Scene()`. The Python gallery lesson
is [Double the input](python/examples/background_music_lesson.py), paired with
[`background_music_lesson.rs`](../crates/noon/src/example_scenes/background_music_lesson.rs).
Open `/?catalog=showcase&example=showcase-background-music`, then choose **Play
with sound**. Its three sections, point motion and soundtrack share Rust time.
The viewer's Previous/Next, seek, mute and volume controls operate on this same
execution owner. Leaving the page pauses playback; returning does not restart it.

`AudioSource.small_discovery()` prepares the bundled music. Use
`AudioSource.from_interleaved(samples, sample_rate=48000, channels=2)` for decoded
float PCM. `Scene.audio(source, start_time=..., source_start_frame=...,
source_end_frame=..., duration=..., loop=True, gain=..., fade_in=..., fade_out=...)`
forwards clip options to Rust. Source trim bounds count PCM frames, times use
seconds, and declarations do not advance the authoring cursor. Finite motion can
use `declare_value(tracker, target, run_time=..., rate_func=linear)` alongside
`declare_wait(...)` and native `bind_position(...)`, as the lesson demonstrates.

Export that same Python source through the existing native MP4 sink:

```bash
node scripts/export-python-scene.mjs \
  web/python/examples/background_music_lesson.py /tmp/noon-music-capture \
  --mp4 scene-with-music.mp4 --width 1280 --height 720
```

Run this command from the repository root after the browser package is built.
It requires Playwright Chromium, Rust/Cargo and FFmpeg with H.264/AAC support.
The source executes once in the normal Pyodide authoring worker. Rust selects
every sample and mixes the soundtrack; the browser renderer supplies pixels and
the native encoder produces one MP4 with sound. `--fps 30000/1001`,
`--start-frame`, `--frames` or `--end-time`, and `--hold` use the same shared frame
policy as native Rust export. The capture directory must be new; an existing
movie is replaced only with explicit `--overwrite` after successful encoding.

Live authored mutation, source continuations, retained source history, native
device playback and scene-authored click controls in the Python worker remain
gated under [#2263](https://github.com/yongkyuns/noon/issues/2263). Ordinary
`Scene.play`/`construct` execution retains its existing continuation behavior.
The headless export command uses browser rendering and native encoding; it is
not an in-process native Python runtime. Text-to-speech is a later milestone.

Run `node scripts/background-audio-smoke.mjs` after building the browser package
to qualify actual device output, playback controls, key-point picture/audio
sync, and retirement. It writes its report and screenshots under
`browser-smoke-artifacts/background-audio/`; it does not establish subjective
music quality or production audio latency budgets.

Run `node scripts/python-background-audio-smoke.mjs` for the actual Python
gallery path, shared Rust animation, gesture activation, fades, sections,
viewer controls and output retirement. Its reports include the source and
runtime build identities plus a real captured lesson frame.

| Direct Rust/WASM qualification | Shared Rust builder | Browser-owned input |
| --- | --- | --- |
| Sequential ordinary affine play | `noon::example_scenes::ordinary_affine_play()` | None; the typed Rust session owns both plays, the wait, and the authored edit |
| Flat ordinary composition | `noon::example_scenes::ordinary_composition_play()` | None; the typed Rust session owns Parallel/Sequence scheduling and completion |
| Default Succession | `noon::example_scenes::ordinary_succession_program()` | None; matches the four-dot Python tutorial with Smooth child curves under its linear sequence root |
| Flat composition continuation | `noon::example_scenes::ordinary_composition_continuation_program()` | None; the typed Rust program owns both composition segments and their renderer admission barriers |
| Ordinary FadeIn/FadeOut | `noon::example_scenes::ordinary_fade_continuation_program()` | None; the typed Rust program owns appearance, membership, detached wait, and same-handle re-entry |
| Parallel Create | `noon::example_scenes::ordinary_square_and_circle_create_continuation_program()` | None; matches Python `manim_parity_square_and_circle.py` with one atomic admission and reveal segment |
| MovingCameraCenter | `noon::example_scenes::ordinary_moving_camera_center_program()` | None; shared camera transforms and live admission after a wait match the literal Python scene |
| Ordinary Uncreate | `noon::example_scenes::ordinary_uncreate_continuation_program()` | None; detached admission, reverse reveal, and endpoint removal share the same runtime |
| Uncreate options | `noon::example_scenes::ordinary_uncreate_options::program()` / `python/examples/ordinary_uncreate_options.py` | None; shared Rust resolves easing, reveal direction and optional endpoint removal |
| Static Typst / MathTypst | `noon::example_scenes::typst_text_reference()` / `math_typst_text_reference()` | None; shared semantic text resources supply glyphs and vector geometry |
| Ordinary Create | `noon::example_scenes::ordinary_create_continuation_program()` | None; the typed Rust program owns introduction, reveal, endpoint reconciliation, and continuation admission |
| Create → SquareToCircle → FadeOut | `noon::example_scenes::ordinary_create_then_content_morph_program()` | None; native and direct-WASM hosts run the same typed content and lifecycle continuation |
| Ordinary style play | `noon::example_scenes::ordinary_style_play()` | None; the typed Rust session owns fill/object-opacity interpolation, completion, and the following authored style edit |
| Ordinary paint play | `noon::example_scenes::ordinary_paint_play()` | None; the typed Rust session owns fill/stroke color and paint-opacity interpolation, completion, and the following authored paint edit |
| Scene membership | `noon::example_scenes::ordinary_membership::program()` | None; shared Rust owns batch validation, ordered roots, family promotion, replacement and clear |
| TransformMatchingShapes | `noon::example_scenes::family_transform_indicate::matching_shapes::program()` | None; the original reordered triangle/kite replacement, source re-add, wait, and following Indicate share the typed Rust lifecycle |
| TransformMatchingShapes breadth | `noon::example_scenes::family_transform_indicate::matching_shapes::breadth_program()` | None; candidate duplicate-key padding, unmatched fades, authored target checks, and following Indicate share the typed Rust lifecycle |
| Ordered subset display | `noon::example_scenes::ordinary_subset_display::program()` | None; shared Rust owns family preparation, detached admission, exact increasing/one-by-one thresholds, and paint endpoints |
| Mixed Text Write | `noon::example_scenes::text_write::program()` | None; shared Rust composes typed Text movement with ordered glyph Write and atomic admission |
| Native signals | `noon::example_scenes::live_native_signals()` | Typed pointer, Space-key, opacity-control, and ordered pointer-down occurrences; no scene JSON or semantic IDs |

## Curated examples

The **Example** picker is intentionally a teaching sequence rather than a feature dump. Each scene has one primary purpose and one unique source file:

1. **Getting started** — primitives plus semantic movement/rotation/opacity authoring.
2. **Analytic Transform** — circle radius, rectangle size, and line endpoint interpolation without path conversion.
3. **Lifecycle handoffs** — `ReplacementTransform` versus `TransformFromCopy` presence semantics.
4. **Fade & appearance** — `FadeOut`/`FadeIn` while preserving authored semantic opacity.
5. **Matching shapes** — deterministic `TransformMatchingShapes` pairing by shape signature. The separate breadth gallery candidate covers duplicate keys and unmatched source/target members.
6. **Path reveal** — one multi-contour path over the ordered reveal domain.
7. **Filled path Transform** — validated fixed-topology interpolation from a rounded loop to a star.
8. **Staggered timing** — identical motion with only timing varied.
9. **Instanced field · 180** — analytic batching and dirty instance uploads on a semantic grid.
10. **Morph stress · 1,000** — one deliberately dense profiling scene with twelve reusable morph targets.

The separate matching-shapes breadth candidate covers explicit 2D vector paths with duplicate-key growth
and unmatched source/target fades while preserving the original reordered triangle/kite gallery lifecycle.
The pinned Manim differential still owns the qualification decision. `key_map`, `transform_mismatches`,
and `fade_transform_mismatches` remain explicitly unsupported in the public wrapper.

Gallery scenes execute through shared Rust semantic sessions. Source-owned `play()`/`wait()` continuations determine their duration; they are not constrained to a fixed four-second loop. Browser qualification exercises this path in CI.

## Semantic Python authoring

Layout vocabulary is part of `noon` itself; there is no separate browser-only layout scene model:

```python
from noon import *

scene = Scene()

left = Circle(0.45, color=BLUE)
right = Square(0.9, color=PINK).next_to(left, RIGHT)
scene.add(left, right)

scene.play(
    left.animate.shift(UP),
    right.animate.rotate(45 * DEGREES),
    run_time=1.5,
)
scene.play(Transform(left, Square(1.0, color=PURPLE)), run_time=1.0)

result = scene
```

`Vec2`, `ORIGIN`, direction/corner constants, named colors, object-aware layout, `Group`/`VGroup`, `run_time`, `wait`, and `.animate` use shared Rust semantic operations. Python owns authoring ergonomics and arbitrary Python callback invocation; Rust owns semantic and execution state.

Sequential `.animate` operations are evaluated at semantic scene time. A later animation therefore starts from the exact endpoint authored by the previous animation rather than the object's original base snapshot.

Transform semantics are authored in Rust and lowered into renderer-independent execution data. Analytic primitives and fixed path geometry remain distinct execution representations.

`node scripts/manim-typst-authoring-smoke.mjs` qualifies native Text, multiline text, Rust-owned layout, Typst/MathTypst font scaling, and mixed painter order through shared execution and actual glyph rendering. Its optional `NOON_TEXT_AUTHORING_REPORT` contains read-only runtime observations; the harness does not export an authored scene document.

## Live authoring

Edit **Python scene source** and click **Run** to author and attach a shared semantic execution session. A rerun replaces the session; it does not currently qualify incremental hot reload or identity preservation between independently authored scenes. Within a running session, shared Rust semantics own object identity and mutations.

Python loads lazily in a separate Pyodide worker. The normal playground does not serialize scene documents or allocate frontend semantic identities. Source continuations attach early so Rust execution can release each `play()`/`wait()` barrier. Direct Rust/WASM examples do not require Python.

The worker loads the pinned Pyodide distribution from jsDelivr, so first-time Python authoring requires network access. The Rust/WASM package remains local under `web/pkg/`.

`node scripts/authoring-perf.mjs` measures cold authoring, unchanged source reruns, one-object source edits, and static seek control round trips through shared sessions. Schema 2 explicitly reports session replacements and unavailable isolated CPU/GPU, incremental-mutation, and camera-uniform timings. It cannot be compared with the removed scene-document profiler. `node scripts/perf-corpus.mjs` measures representative authored scenes, including moving cameras, through shared execution.

## Analytic performance workloads

`node scripts/perf-profile.mjs` runs the target-neutral Rust analytic builder directly in WASM. The `fit`, `fixed`, and `overdraw` layouts author their camera and a single moving circle through the normal shared scene/track APIs. Frames advance in-process through Runtime and Renderer; JavaScript measures browser cadence and host-call durations. GPU timestamp availability is reported separately.

Run the same builder in a native window with `cargo run --example analytic_profile -- 1000 fit`. The equivalent Python scene is `python/examples/analytic_profile.py`, with `object_count`, `layout`, `aspect`, and `duration` context parameters. These examples demonstrate the shared path; they do not establish 100k performance thresholds.

The frame profiler emits schema 2. Its measurements must not be compared with the removed JSON scene/delta profiler; the aggregate benchmark name also distinguishes the direct Rust path.

`node scripts/retained-dynamic-stress-perf.mjs` runs the unchanged five-second Dynamic Load Stress gallery source through shared execution. Each loop authors a fresh source session and reports all ten authored phases, with 626 final visible objects. Defaults are 60 Hz and two loops for each worker transport (`transferable` and `shared`). Set `NOON_RETAINED_STRESS_BACKEND=webgl` for WebGL2; `NOON_RETAINED_STRESS_SAMPLE_HZ`, `NOON_RETAINED_STRESS_WORKER_LOOPS`, and `NOON_RETAINED_STRESS_TRANSPORTS` select explicitly labeled qualification subsets.

This stress report measures the source/worker/runtime/render round trip. Its schema 2 does not establish isolated CPU/GPU budgets, warm replay, or the removed exported-scene profiler's morph activation thresholds. A lower sampling rate qualifies execution coverage, not 60 Hz performance.

## Vector paths

Generic paths are semantic command streams and remain distinct from analytic circle/rectangle/line fast paths. The Rust and Python APIs support move, line, quadratic, cubic, and close commands:

```python
from noon import BLUE, WHITE, Path, Scene, VectorPath

curve = (
    VectorPath()
    .move_to((-1.0, 0.0))
    .quadratic_to((0.0, 1.5), (1.0, 0.0))
    .line_to((0.0, -1.0))
    .close()
)

scene = Scene()
scene.add(Path(curve, fill=BLUE, stroke=WHITE, stroke_width=0.06))
```

Static path meshes are tessellated once per semantic path/topology cache key and reused across instances. Geometry-changing path Transforms and supported cross-kind analytic Transforms prepare a fixed source/target pair before playback; steady frames update only compact instance state rather than retessellating geometry.
