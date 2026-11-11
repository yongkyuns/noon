# Your first animation

Run a scene, change its color and timing, and understand the few ideas behind it.
This first lesson runs in your browser; there is nothing to install.

## Browser

The live scene below starts as the page loads. It creates a circle and a square,
moves them, then brings them back. The first load downloads the engine and Python
runtime; a loading message disappears when the scene is ready.

<!-- noon:demo showcase-first-scene -->

Use the small **View / Code** switch on the scene. Code is the actual Python
source, not a screenshot. Edits run automatically after a typing pause. Switching
back to View preserves your changes and does not submit another run.

## 1. Change a color

Switch to **Code**. Find the line that creates the circle:

```python
circle = Circle(radius=0.75, color=BLUE).set_fill(BLUE, opacity=0.8)
```

Change both occurrences of `BLUE` to `GREEN`, then switch back to **View**.
The circle should now have a green outline and green fill. The square should
remain pink. No Run button or separate editor is needed.

## 2. Change the motion

Find `circle.animate.shift(0.7 * UP)`. Change `0.7` to `1.2`. After the automatic
reload, the circle should move farther upward. Change the later downward shift
to `1.2` too, so it returns to the same starting position.

The `run_time=1.6` argument controls that motion's duration in authored seconds.
Changing it to `3.2` makes the same motion take twice as long. It is not a frame
count: you describe what should happen over time, not how many frames to draw.

## 3. Understand the structure

The core pattern is small. The live example adds captions and a few more steps:

```python
from noon import *

class Demo(Scene):
    def construct(self):
        circle = Circle(color=BLUE)
        square = Square(color=PINK)
        square.next_to(circle, RIGHT)

        self.play(Create(circle), Create(square))
        self.play(circle.animate.shift(UP), run_time=2)
        self.wait(1)
```

`Scene` is the program you author. `construct` describes its work. `Circle` and
`Square` create visual objects; `next_to` arranges them. `Create` introduces the
objects, `.animate` describes a change, and `wait` adds time without a new action.

Two animations inside one `play` call run together. Separate `play` calls happen
in sequence. These ideas are enough to begin; the [authoring guide](guide.html)
goes into scene membership, families, coordinates, and animation composition.

## When an edit does not work

Incomplete Python is normal while typing. Switch to **Code** to read the error,
correct the source, and let it reload. Changing View/Code does not discard the
draft. Use your editor's Undo command to reverse an accidental change.

For a blank canvas after loading, check [troubleshooting](troubleshooting.html).
For full playback controls, open the [playground](../../playground.html?catalog=showcase&example=showcase-first-scene).

## Continue from here

[Learn scene authoring](guide.html), follow the [Python walkthrough](python.html),
or [choose a local installation](installation.html). Prefer Rust? The
[Rust walkthrough](rust.html) starts with a complete synchronous lesson.

The website runs Python through Pyodide and the Rust engine through WASM.
Changing Rust source is a build step, not an edit in this Python panel.
