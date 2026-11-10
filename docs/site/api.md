# API reference

This build contains two different source-backed references: compiler-generated
Rust documentation, and a static index of the Python facade's explicit exports.
The revision and extraction coverage are recorded with the site.

## Rust

[Open the generated Rust API](rust/noon/index.html). Rustdoc supplies the actual
public signatures, traits, types and docstrings for this checkout's default
feature profile. A back-link on each generated page returns to this documentation.
For complete applications and host usage, see [Rust library details](native-rust.html)
and [native export details](export-details.html).

## Python

The index below resolves `noon.__all__`, `_PUBLIC_EXPORTS`, and local aliases
without importing the runtime. Each callable page includes its declared signature,
parameter kinds/defaults/annotations, return annotation, docstring and source link.
Class pages list their declared methods and bases.

This is source-derived documentation, not an execution test. Dynamically attached
or inherited members are not silently treated as absent: the page identifies its
coverage, and unresolved external/runtime exports are listed explicitly.
A symbol's presence does not establish every native/browser feature combination.

<!-- noon:python-index -->
