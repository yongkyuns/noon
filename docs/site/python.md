# Python walkthrough

This complete lesson is the checked-in source used by the native package example.
It is paired with the Rust lesson, rather than being a separate demonstration of
an imagined API.

## The complete scene

<!-- noon:source web/python/examples/first_lesson.py -->

## Objects, families, and membership

`Text`, `MathTypst`, `Square` and `Circle` construct visual objects. `shift` places
them in scene coordinates, not CSS pixels. `VGroup` makes the two shapes a family,
so a single animation can move both. `self.add` makes those objects visible
without introducing a `Create` animation.

The small `step` helper receives the scene and the family. It calls the same
`play` method you would use inside `construct`; a helper function and an ordinary
Python loop need no separate scheduling mechanism.

## Authored time

The first wait contributes half a second. Each of the two calls to `step`
contributes half a second, and the final wait contributes another half second.
The lesson therefore authors two seconds of content. Creating the last caption
happens after the earlier plays; it is not necessary to create every object at
time zero.

The Python host executes authoring code. Rust owns the shared scene semantics
and runtime. Replaying a qualified finite scene should not be confused with
rerunning its source, especially when source has side effects.

## Run the same file natively

```sh
PYTHONPATH=build/python python3 -m noon_native web/python/examples/first_lesson.py
```

For an installed wheel, `python -m noon_native first_lesson.py` or
`noon first_lesson.py` uses the installed facade without `PYTHONPATH`.
Follow [native Python details](native-python.html) for the wheel-build procedure;
only install a candidate that matches your interpreter and platform.

## Extend the lesson

Change the shift amount or the loop count and run again. Changing a Python
numeric parameter is ordinary source authoring. For interactive changes driven
by input during playback, use the appropriate input/reactive APIs and consult
[playback and interaction](playback.html).

Use the generated [Python API index](api.html#python) to find the actual signatures
and declared methods of `Scene`, `Mobject`, `VGroup` and `Text` in this revision.
