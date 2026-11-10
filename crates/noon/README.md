# noon

Ergonomic language-neutral authoring facade for Noon.

This package belongs to [Noon](https://github.com/yongkyuns/noon), a shared Rust
animation engine with native, Python and browser hosts. Ordinary scene authors
start with the public authoring facade and a native or browser host; supporting
crates retain their existing compilation and dependency boundaries.

## Release status

The 0.1.0 artifacts are local release candidates. Registry names remain undecided;
the existing `noon` packages on crates.io and PyPI belong to unrelated projects.
Do not install those packages expecting this engine. Follow
[release readiness #2433](https://github.com/yongkyuns/noon/issues/2433) for the
qualified artifact set and eventual installation names.

The supported build toolchain is Rust 1.98.1 or newer. Native Python and browser
support are qualified separately from the native Rust library. See the repository
README for the supported feature and host profiles.

## First native application

Until registry names are settled, create an application alongside a checkout of
this repository and use its local packages:

```toml
[dependencies]
noon = { path = "../noon/crates/noon" }
noon-native = { path = "../noon/crates/noon-native" }
```

Save this as `src/main.rs`, then run `cargo run`. A native graphics driver and a
desktop session are required for the window. The source authors exactly two
seconds; the window remains open until you close it.

```rust
use noon::{
    AnimationOptions, BlockingScene, Color, MathTypst, MobjectFamily, RateFunction, ScriptError,
    Text, Vec2,
};

fn step(scene: &BlockingScene, shapes: &MobjectFamily) -> Result<(), ScriptError> {
    scene.play_with(
        shapes.animate().shift(Vec2::new(0.75, 0.0)),
        AnimationOptions::new()
            .run_time(0.5)
            .rate_func(RateFunction::Linear),
    )
}

/// Construct two seconds of text, math and grouped motion without async syntax.
fn construct(scene: &BlockingScene) -> Result<(), ScriptError> {
    let title = scene.text(
        Text::new("One scene, ordinary functions")
            .with_font_size(36.0)
            .shift(Vec2::new(0.0, 2.7)),
    )?;
    let equation = scene.math_typst(
        MathTypst::new("x(t) = x_0 + v t")
            .with_font_size(48.0)
            .shift(Vec2::new(0.0, 1.6)),
    )?;
    scene.add(&title)?;
    scene.add(&equation)?;

    let square = scene.square(0.8)?;
    let circle = scene.circle(0.4)?;
    scene.set_fill(&square, Color::BLUE, 1.0)?;
    scene.set_fill(&circle, Color::ORANGE, 1.0)?;
    scene.shift(&square, Vec2::new(-0.8, -0.4))?;
    scene.shift(&circle, Vec2::new(0.8, -0.4))?;
    let shapes = scene.family([&square, &circle])?;
    scene.add(&shapes)?;
    scene.wait(0.5)?;
    for _ in 0..2 {
        step(scene, &shapes)?;
    }

    // This resource is constructed after execution has started. The same Scene
    // admits it atomically; the source never needs raw storage or host plumbing.
    let caption = scene.text(
        Text::new("Two steps, one shared scene")
            .with_font_size(24.0)
            .shift(Vec2::new(0.0, -2.0)),
    )?;
    scene.add(&caption)?;
    scene.wait(0.5)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run_scene(construct)?;
    Ok(())
}
```

`BlockingScene` also exposes `typst`, `image`, `image_rgba8`, `code`, `axes`,
`number_line` and `number_plane` through the same scoped Scene admission API.
Objects are detached until `add`. Resource errors are typed `ScriptError`s;
failed preparation preserves the previously published scene. Calls after a
completed `play`/`wait` continue through the same Scene and runtime.

The async `Scene::script` capability exposes the same constructors; await its
`play` and `wait` methods. Geometry-only library users can set
`default-features = false`; text, Typst, code highlighting and image decoding are
explicit provider features. See the repository README for their combinations.

The paired Python lesson is
[`first_lesson.py`](https://github.com/yongkyuns/noon/blob/master/web/python/examples/first_lesson.py).

## Export the same ordinary source

Add the checkout dependency `noon-export = { path = "../noon/crates/noon-export" }`
and pass the unchanged `construct` function above to
`noon_export::output::export_blocking_scene`, together with explicit frame,
capture and output options. The [complete export example](../noon-export/examples/first_lesson.rs)
and [export API guide](../noon-export/README.md) use that same two-second lesson:

```sh
cargo run -p noon-export --example first_lesson -- lesson.mp4
cargo run -p noon-export --example first_lesson -- --png lesson-images
```

These commands run from the repository checkout. They produce 60 frames at
320×180 and 30 FPS through the native renderer and installed FFmpeg, without a
window. Source executes once, each ordinary boundary completes on the shared
runtime, and the exact completed Scene/runtime pair transfers to sealed-history
sampling. Source operations, boundaries and retained history have explicit
budgets in addition to the output frame cap. Required callbacks, unrecorded input
and retained-history audio remain outside this finite visual export profile.
The ordinary native viewer continues to service observed callback/input sources.

## License

MIT. See LICENSE. Third-party resources retain their original notices.
