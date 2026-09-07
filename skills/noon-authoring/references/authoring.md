# Authoring constraints

## Geometry, layout, and timing

Prefer the qualified relative-layout operations from a working example over
hand-guessed coordinates. Plan contrast, label clearance, the viewing frame, and
holds at explanatory endpoints. Do not claim every Group/VGroup method works just
because the class is exported.

Check whether an operation mutates the source object, replaces its membership,
creates a copy, or removes it. Preserve object identity in subsequent statements.
Distinguish `.animate` endpoint interpolation from a dedicated animation with a
particular motion path. Obtain current restrictions through capability discovery.

## Text and mathematical meaning

Query Text, Tex, MathTex, Typst, or MathTypst before using them. A source-compatible
facade is not evidence of exact LaTeX glyph, layout, timing, or term-matching parity.
Inspect the rendered mathematical expression, including signs, fractions, subscripts,
labels, and missing glyphs. Do not automatically substitute plain text or translate
LaTeX syntax into Typst. A change of representation must preserve meaning and be
explicit in the result.

Plotting and 3D recipes from external Manim skills are not Noon capability evidence.
Report a missing feature rather than installing Manim and calling its output Noon.

## Continuations, callbacks, and state

Python can issue semantic operations, wait for a segment-completion barrier, observe
coherent effective state, and continue. Static playback need not involve Python;
arbitrary callbacks have explicit host-execution requirements. Do not infer that
arbitrary Python expressions become native reactive dependencies.

Prefer a known supported deterministic pattern. Use callbacks only with a matching
example and restrictions. Wall-clock reads, unseeded randomness, I/O, and hidden
mutable callback state undermine repeatability. Never assume a fresh replay safely
repeats external side effects.

Seek/rewind is a capability of the actual session, not of every Noon scene. The
current external-sampling raster harness is forward-only and fixes its sample
schedule. Do not change a running schedule or promise reverse playback for opaque
callbacks. An interactive continuation may have no known final duration.

## Rust

The shared Rust API authors directly into shared semantics. Rust source is compiled;
mutating a live scene through compiled methods is not interpreting arbitrary new
Rust code. Use the current README's public-API examples, not migration-era
`noon::legacy` examples. Paired Python/Rust examples establish the common behavior
while allowing different host control flow.

Ordinary authored/base state and current effective runtime state are different
queries. Use the documented live query/transaction surface when working with a live
session. Do not implement a second scene store or serialized patch engine in an
agent helper. Repository architecture and current owning issues remain authoritative.
