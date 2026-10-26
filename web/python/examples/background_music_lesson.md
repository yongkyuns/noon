# Double the input

This 30-second lesson follows the rule `y = 2x` on two number lines. A blue point
shows the input and a gold point shows the output. Positive values, negative
values, and zero each have their own named section. The original instrumental
score **A Small Discovery** accompanies all three sections.

Open **Double the input** in the Noon showcase, then press **Play** to hear the
music. The prepared scene starts paused. Seeking and section navigation use the
same authored times for the picture and music. Viewer volume and mute change the
device output without changing the authored gain or fades.

## The paired authoring paths

The Python source is [background_music_lesson.py](background_music_lesson.py).
Its direct Rust counterpart is
[`noon::example_scenes::background_music_lesson::scene`](../../../crates/noon/src/example_scenes/background_music_lesson.rs).
The Rust function returns an ordinary `Scene` for `Scene::execution_session`
and native MP4 export. Native window audio output has not yet been qualified.

Python uses an explicit finite `result = Scene()`. This complete scene is declared
before playback. Normal `Scene.construct`, `Scene.play`, and `Scene.wait` retain
their source continuation behavior.

```python
result = Scene()
x = result.value_tracker(0)
# Add input_point and output_point before binding them.
result.bind_position(input_point, x, direction=RIGHT, offset=(1, 0.75, 0))
result.bind_position(output_point, x, direction=2 * RIGHT, offset=(1, -1.1, 0))

result.next_section("Double a positive input")
result.declare_wait(1)
result.declare_value(x, 2, run_time=8, rate_func=linear)
result.declare_wait(1)

result.audio(
    AudioSource.small_discovery(),
    duration=30, gain=0.5, fade_in=1.5, fade_out=2.5,
)
```

`declare_value` calls the shared Rust scalar timeline operation. `declare_wait`
advances the Rust authoring cursor. `next_section` records a boundary at that
cursor. `bind_position` keeps the relationship in Rust; no Python callback runs
on animation frames.

## Finite clip options

`AudioSource.small_discovery()` prepares the bundled 80-second stereo composition
at 24 kHz. `AudioSource.from_interleaved(samples, sample_rate=..., channels=...)`
accepts already-decoded mono or stereo float PCM. Stereo samples alternate left
and right. A source owns immutable Rust PCM; attaching it to clips reuses that
payload. Rust validates its format, finite sample values, and resource budget.

`Scene.audio(source, ...)` returns an opaque `AudioClip` with read-only
`start_time`, `end_time`, and `duration` observations. The clip options are:

| Argument | Meaning |
| --- | --- |
| `start_time` | Absolute authored start time in seconds; defaults to zero. |
| `source_start_frame`, `source_end_frame` | Half-open trim range in PCM frames. The default selects the complete source. |
| `duration` | Finite clip duration in seconds. The default is the selected source duration. |
| `loop` | Repeat the selected source region for the declared duration. |
| `gain` | Authored linear gain; defaults to one. |
| `fade_in`, `fade_out` | Clip-wide fades in seconds. A source loop does not restart a fade. |

Clips may overlap; the shared Rust audio plan owns their timing and mixing.
Declarations must precede execution. Live audio mutations, callback-driven
audio, and audio history changes remain outside this finite profile. The source
does not perform file decoding or network loading. The Python worker currently
uses the viewer's transport controls; scene-authored `on_click_playback` actions
remain explicitly gated until that input path is qualified.

The original composition and its rendered audio are CC0-1.0; the software remains
MIT. The scoped notice is in
[`render_background_music/README.md`](../../../crates/noon/examples/render_background_music/README.md).

## Qualification

The Rust lesson test checks section times, native point bindings, direct-seek and
forward-frame agreement, music offsets, clip-wide gain, and the exact finite
endpoint. A separate Rust test constructs the same lesson through the canonical
binding entrypoints and compares the actual frames and audio instructions. Python
unit tests check wrapper routing and argument conversion with mocked host objects;
they do not establish runtime or device behavior.

After building the web runtime, run the real browser profile with:

```bash
node scripts/python-background-audio-smoke.mjs
```

The script loads the unchanged Python source through the normal showcase and
Pyodide worker, checks actual Web Audio output using an analyser, verifies seek
and section behavior, checks viewer controls and lifecycle retirement, and
captures real rendered checkpoints. It exercises the product visibility handler
with a controlled document visibility event. A separate editor/Run fixture passes
decoded Python PCM through the real Pyodide boundary, then verifies immutable
buffer reuse, overlapping trimmed loops, and their seek phase on actual output.
Its report records the browser,
renderer, source hash, and served build identity. Chromium/WebGL is the default;
`NOON_PYTHON_AUDIO_BROWSER` and `NOON_PYTHON_AUDIO_BACKEND` select another explicit
profile. A passing run qualifies only that reported browser and renderer.

Set `NOON_PYTHON_AUDIO_RETAIN_POSTER=1` to retain the actual eight-second capture
and its source-bound evidence for the gallery after the complete profile passes.
The prior showcase capture batch keeps its own build identity.
