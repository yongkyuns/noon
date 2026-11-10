# Noon

Noon is an experimental animation engine inspired by [Manim](https://www.manim.community/).
It is written in Rust, with performance in mind, and supports authoring in **Python
(native and web)** and **Rust**.

The goal is to make creating animated visuals feel frictionless. Mathematical
explanations are one use, but the scope also includes simulations, interactive
design, and other kinds of visual experiments.

[Website](https://yongkyuns.github.io/noon/) ·
[Try the playground](https://yongkyuns.github.io/noon/playground.html?catalog=showcase) ·
[Implementation reference](REFERENCE.md) · [Architecture](docs/architecture.md)

## A few examples

These GIFs are excerpts rendered by Noon. Click one to open its editable Python
source and run the complete scene in the browser. They show the output, not a
performance benchmark.

| Your first scene | Interference is addition |
| --- | --- |
| [![A blue circle and pink square being created, moved, and rotated](https://yongkyuns.github.io/noon/site/media/showcase-first-scene.gif)](https://yongkyuns.github.io/noon/playground.html?catalog=showcase&example=showcase-first-scene) | [![Two circular wave fields and their sum as the relative phase changes](https://yongkyuns.github.io/noon/site/media/showcase-wave-interference.gif)](https://yongkyuns.github.io/noon/playground.html?catalog=showcase&example=showcase-wave-interference) |
| Create objects, arrange them, and animate their properties. | Add two wave fields and see how a half-cycle phase shift changes the result. |
| **A field in motion** | **Make it move** |
| [![A field of colored shapes forming waves and a spiral](https://yongkyuns.github.io/noon/site/media/showcase-dynamic-scene.gif)](https://yongkyuns.github.io/noon/playground.html?catalog=showcase&example=showcase-dynamic-scene) | [![An editorial layout with animated gradients, grain, and a frosted caption](https://yongkyuns.github.io/noon/site/media/showcase-editorial-cover.gif)](https://yongkyuns.github.io/noon/playground.html?catalog=showcase&example=showcase-editorial-cover) |
| Coordinate a larger composition of shapes and labels. | Combine typography, procedural surfaces, and visual effects. |

The [gallery](https://yongkyuns.github.io/noon/playground.html?catalog=showcase)
also includes plots, text and equations, code blocks, 3D examples, audio, and
input-driven scenes such as
[click selection](https://yongkyuns.github.io/noon/playground.html?example=showcase-pointer-selection)
and [dragging objects](https://yongkyuns.github.io/noon/playground.html?example=showcase-translation-drag).
Each example has its source available to edit.

## Try it in the browser

Open the [playground](https://yongkyuns.github.io/noon/playground.html?example=showcase-first-scene),
choose an example, and press **Run**. Edit the Python source and run it again.
There is nothing to install; the first run downloads the WebAssembly engine and
Pyodide. Rendering uses WebGPU or WebGL2, depending on the browser and device.

A small scene looks like this:

```python
from noon import *

class Demo(Scene):
    def construct(self):
        circle = Circle(color=BLUE)
        square = Square(color=PINK)
        square.next_to(circle, RIGHT)

        self.play(Create(circle), Create(square))
        self.play(
            circle.animate.shift(UP),
            square.animate.rotate(PI / 4),
            run_time=2,
        )
        self.wait(1)
```

The [first-scene example](web/python/examples/showcase_first_scene.py) adds
captions, styling, and a few more animation steps.

## Run it locally

Start from a checkout. Native builds use the Rust toolchain pinned in
[`rust-toolchain.toml`](rust-toolchain.toml); Python also needs CPython 3.11 or newer.
The commands below use a POSIX shell. Native windows require a working graphics
adapter and the platform's usual window-system dependencies.

```bash
git clone https://github.com/yongkyuns/noon.git
cd noon
```

### Python

Build the native extension and run a scene through the shared Rust engine:

```bash
python3 scripts/build-native-python.py --profile dev
PYTHONPATH=build/python python3 -m noon_native web/python/examples/ordinary_affine_play.py
```

This is the native CPython path, not the browser exporter. See
[Native CPython](REFERENCE.md#native-cpython) for headless execution, snapshots,
scene selection, and current limitations. This command builds the extension
from the checkout; prepublication wheel candidates are described below.

### Rust

Run an existing native example:

```bash
cargo run -p noon-native --example ordinary_affine_play
```

Rust uses the same engine directly, without Python. See the
[native examples](crates/noon-native/examples) and
[Rust authoring reference](REFERENCE.md#authoring) for scene construction,
and sequential animation. Rust also runs as WebAssembly; host setup differs
from the native runner.

For a local browser build, follow the [web guide](web/README.md).

## Release candidates

Registry names are not yet finalized, and the existing `noon` entries on
crates.io and PyPI are unrelated projects. The current checkout supports
packaging and qualifying Rust crates, native Python wheels, and versioned
browser archives without publishing them. For the commands, supported
platforms, and limitations, see [initial release candidates](REFERENCE.md#initial-release-candidates).

## Current scope and Manim compatibility

Noon follows much of **Manim Community Edition's semantics and visual behavior**
for supported features, particularly common 2D authoring. Familiar operations such
as `Scene`, `play`, `wait`, `Create`, `Transform`, and `.animate` are the intended
starting point. It is not a drop-in implementation of every Manim class, option,
or renderer behavior.

Compatibility is useful, but it is **not a strict long-term goal**. As new features
are added, Noon may take a different direction. Its own runtime, rendering,
interaction, and performance constraints also differ from Manim's.

Noon remains experimental. APIs are changing, and feature coverage differs
between native Python, browser Python, and Rust. Some callback-driven and
interactive scenes have replay or seeking restrictions. Native Python does not
yet cover the complete browser gallery, including its audio bindings. Consult the
[implementation reference](REFERENCE.md) and runnable examples before relying on
a particular feature. Performance is a design concern, not a blanket claim that
Noon is faster than another engine.

## Project background

Noon started years ago as a minimal working prototype. It was recently redesigned
with AI agents, which now contribute much of the development. The project is an
ongoing exploration of what a tool for authoring animated visuals can be.

## Development

The [architecture guide](docs/architecture.md) explains the shared Rust engine and
is the authority for its design. The previous detailed README is retained as the
[implementation reference](REFERENCE.md), including host-specific boundaries and
advanced examples. [Open issues](https://github.com/yongkyuns/noon/issues) track
current work.

Run the repository's validation entrypoint when changing code:

```bash
bash scripts/check.sh fast
```

Bug reports are most useful with a small scene, the revision used, the host or
browser, and what you expected to see. Captured website media and their source
hashes are regenerated by the Pages build; see
[website maintenance](docs/project-site.md).
