# Rust walkthrough

Rust is a first-class authoring path. You can use ordinary synchronous functions
and loops with `BlockingScene`; async syntax is not required for this lesson.
The following source is included directly from the repository, so the guide does
not keep an independent copy of its API calls.

## The complete synchronous lesson

<!-- noon:source crates/noon/src/example_scenes/first_lesson.rs -->

## What the code owns

`BlockingScene` exposes the scene-authoring operations. The `step` helper receives
that scene and a typed family handle. It returns `Result<(), ScriptError>` so
errors propagate through ordinary Rust error handling. It does not allocate or
manage a separate renderer or event loop.

`scene.family` groups the shape handles. The animation builder describes a shift;
`AnimationOptions` supplies its duration and rate function. `scene.play_with`
waits at the engine's logical completion boundary before the loop continues.
The title, formula and final caption use the same scene resource path.

## Run in a native window

```sh
cargo run -p noon-native --example first_lesson
```

The host runner accepts the scene constructor and owns the native window and
event loop. See [Rust library details](native-rust.html) for a complete application
and current checkout dependency setup. Do not copy a public registry dependency
name before the package name is finalized.

## Export the unchanged constructor

```sh
cargo run -p noon-export --example first_lesson -- lesson.mp4
cargo run -p noon-export --example first_lesson -- --png lesson-images
```

These commands run the same finite lesson through the native export host. An
installed FFmpeg is needed for MP4, and rendering needs a supported native GPU
backend. See [exporting visuals](export.html) for restrictions and output policy.

## Rust in a browser

Rust source compiled to WASM uses the same semantic, runtime, and renderer
responsibilities. Browser integration supplies the canvas and event loop; Python
is optional, not a mandatory bridge between the Rust layers. Follow
[browser integration](web-runtime.html) for the existing direct Rust/WASM examples.
Source changes require a rebuild rather than entering Rust into the Python editor.

## API and asynchronous authoring

The generated [Rust API](rust/noon/index.html) contains compiler-produced
signatures and trait relationships. The [implementation reference](reference.html#authoring)
also explains `Scene::script`, async completion boundaries, ordinary callbacks,
and the timing difference between Rust animation builders and Python `.animate`.
Both interfaces reach the same shared engine; choose according to the host and
control flow you need.
