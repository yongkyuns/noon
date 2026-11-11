# Learn Noon

Make animated visuals with Python or Rust. Start with a live scene in the browser,
then learn how objects, animation, and authored time fit together.

<!-- noon:paths start -->

## A clear path from your first scene

1. **[Run and change a scene](getting-started.html).** See the result first. Change
   its appearance and timing, and learn how to recover from a source error.
2. **[Learn the building blocks](guide.html).** Understand objects, arrangement,
   grouping, and the difference between simultaneous and sequential animation.
3. **[Choose a host](installation.html).** Stay in the browser or use a source
   checkout for native Python or Rust.
4. **[Save the result](export.html).** Export a finite scene as frames or video,
   with the requirements and current limitations made explicit.

You do not need to read the engine architecture before making your first scene.

## Find the details when you need them

<!-- noon:paths reference -->

[Playback and interaction](playback.html) explains the capabilities of live and
replayable scenes. [Troubleshooting](troubleshooting.html) helps with blank output,
source errors, native builds, and export failures.

## What to expect

Noon is experimental. Python runs in the browser and natively; Rust has native
and WebAssembly paths. Feature coverage differs between hosts. The API reference
shows what is declared in this source revision, not a promise that every feature
combination works on every host.

ManimCE is a useful reference for supported authoring semantics and visuals, not
an exhaustive compatibility promise. For the precise host boundaries, read the
[implementation reference](reference.html). Every page links to its source and
records the revision it was built from.
