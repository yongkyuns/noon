# Noon documentation

These docs cover writing and running scenes in Python and Rust. The browser
tutorial shows how to edit an example and how Noon handles source errors.

<!-- noon:paths start -->

## Tutorials and guides

1. **[Getting started](getting-started.html).** Run a scene and change its
   appearance and timing.
2. **[Scenes and animation](guide.html).** Arrange and group objects, then animate
   them in parallel or in sequence.
3. **[Installation](installation.html).** Use the browser playground or build
   native Python and Rust from a source checkout.
4. **[Export](export.html).** Save a finite scene as frames or video.

## Reference

<!-- noon:paths reference -->

[Playback and interaction](playback.html) explains the capabilities of live and
replayable scenes. [Troubleshooting](troubleshooting.html) helps with blank output,
source errors, native builds, and export failures.

## Current status

Noon is experimental. Python runs in the browser and natively. Rust has native
and WebAssembly paths. Feature coverage differs between hosts. The API reference
lists the declarations in this source revision.

ManimCE is the reference for supported Python authoring behavior and visuals.
See the [implementation reference](reference.html) for host coverage. Each page
links to its source and records the revision it was built from.
