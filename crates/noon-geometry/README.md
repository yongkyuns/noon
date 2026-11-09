# noon-geometry

Vector geometry compilation and tessellation for Noon.

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

## License

MIT. See LICENSE. Third-party resources retain their original notices.
