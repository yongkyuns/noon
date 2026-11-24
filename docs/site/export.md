# Exporting visuals

Choose the export path that matches your authoring host and the scene's admitted
profile. A finite deterministic lesson is a better first export than a scene
which waits for input or needs unrecorded callbacks.

## Rust: PNG frames or MP4

The same ordinary first-lesson constructor used by the viewer can be exported:

```sh
cargo run -p noon-export --example first_lesson -- lesson.mp4
cargo run -p noon-export --example first_lesson -- --png lesson-images
```

The example uses the shared frame policy and native renderer. MP4 needs FFmpeg;
GPU capture needs a supported native driver, but no desktop window is required.
The current example dimensions, sampling policy and source budgets are documented
in [native export details](export-details.html). Do not treat the example's
chosen resolution as a general export limit.

## Installed native Python

With the matching native wheel installed, run the checked-in Python lesson:

```sh
noon first_lesson.py --snapshot lesson.png --time 1.75
noon first_lesson.py --mp4 lesson.mp4 --fps 30000/1001 --width 1280 --height 720
```

`python -m noon_native` is equivalent to the installed console command. Native
Python MP4 export uses native rendering and FFmpeg, not a browser or Node.
Use [native Python details](native-python.html) for output options and the exact
finite-scene restrictions. `--headless` source execution alone is not a test that
GPU rendering works.

## Browser Python capture

The browser workflow also has a source-export utility and an audio-capable finite
profile. Its requirements and complete commands are in
[browser integration](web-runtime.html#background-audio-and-scene-navigation).
Do not interchange native and browser capture commands: their host preparation
and supported audio/callback profiles differ.

## Verify an export

Check the requested dimensions, frame rate, duration and the decoded output,
not merely the existence of a file. Required callbacks/input and retained-history
audio are outside some finite export profiles; a rejection should remain visible
rather than being replaced with a stale frame or silent fallback.
