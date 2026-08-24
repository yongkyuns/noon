# ManimCE differential testing

Noon's internal cross-language parity tests answer a necessary but insufficient
question: do the Rust and Python frontends agree with each other? They do not
prove that either frontend agrees with ManimCE.

`scripts/manim-differential.py` adds a second, independent compatibility gate.
It executes equivalent renderer-independent operations through Noon and a
pinned ManimCE installation and compares normalized semantic observations.
The CI reference is intentionally pinned to **ManimCE 0.21.0**; changing that
version is a compatibility-target decision, not a routine dependency upgrade.

## What belongs in the differential suite

Prefer small semantic probes with useful structural diffs:

- object center, width, and height;
- layout operations such as `next_to`, `align_to`, `to_edge`, and `arrange`;
- family membership/order and z ordering once the authoritative semantic store
  exposes them;
- fill/stroke/style observables;
- animation timing, lifecycle, and state sampled at selected times;
- updater/tracker state when the corresponding execution semantics are stable.

Raster comparisons remain valuable, but only for behavior whose correctness
cannot be expressed structurally. The existing browser rendering tests remain
responsible for renderer output.

## Supported versus unsupported

A missing feature is not the same thing as a wrong implementation. The harness
therefore has an explicit `UNSUPPORTED` registry. A semantic area moves from
that registry into `FIXTURES` when Noon claims to support it. From that point,
a mismatch is a CI failure.

This rule is particularly important during the architecture reset: it lets the
suite document gaps without normalizing Noon's current behavior as the Manim
specification.

## Running locally

Install the same Manim version used by CI and run:

```sh
python scripts/manim-differential.py
```

For machine-readable output:

```sh
python scripts/manim-differential.py --json
```

Linux systems need Manim's Cairo/Pango/FFmpeg dependencies; the dedicated
GitHub Actions workflow documents the exact CI setup.

## Expansion policy

Every architecture or compatibility PR should add a focused differential
fixture when it claims new Manim-compatible semantics. Keep probes small enough
that a failure identifies the semantic difference directly rather than merely
reporting that a large demo scene changed.
