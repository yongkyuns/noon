# noon-native

Native window host for typed Noon execution sessions.

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

## Run a scene

The native host owns the desktop window, event loop and presentation while the
shared Rust engine owns the scene and execution state:

```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run_scene(noon::example_scenes::first_lesson::construct)?;
    Ok(())
}
```

From a checkout, run `cargo run -p noon-native --example first_lesson`. The
complete authoring source is in the public `noon` facade README. The ordinary
source plays once and the window remains open until closed. The default host is
visual-only; `audio` is an optional feature and requires a native audio device
and its platform build dependencies. Offscreen PNG/MP4 output is provided by
`noon-export` and does not require the window host.

## License

MIT. See LICENSE. Third-party resources retain their original notices.
