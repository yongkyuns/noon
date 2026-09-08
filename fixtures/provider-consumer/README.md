# Isolated provider consumer

This is a real external Cargo consumer with its own workspace. It proves optional
provider isolation without unrelated Noon workspace members unifying features.
The `main` binary always builds the **same geometry scene**, including a coherent
live edit and authored/effective queries. Shared `TextResource` types remain
available in the geometry-only build. Tests exercise explicit host font input,
provider normalization into the same resource store, resource reuse across live
edits, error categories and failure/recovery with missing or invalid fonts.

| Configuration | Facade features | Active providers and assets |
| --- | --- | --- |
| `minimal` | none | Neither compiler, shaper nor font bundle |
| `native-text` | `native-text` | Native shaping; explicit font input, no Typst or font bundle |
| `native-bundled` | `native-text,bundled-fonts` | Native shaping and font bundle; no Typst compiler |
| `typst` | `typst` | Typst layout and base compiler assets; explicit fonts, no native provider or typography font bundle |
| `product` | default features | Both providers and bundled fonts, as before |

Run from the repository root (Python 3, the pinned Rust toolchain, Clippy, and the
WASM target are required):

```sh
python3 scripts/provider_features_test.py
export NOON_PROVIDER_TEST_FONT=/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf
python3 scripts/provider-features.py --config minimal \
  --target x86_64-unknown-linux-gnu --output /tmp/noon-minimal-native
python3 scripts/provider-features.py --config native-text \
  --target wasm32-unknown-unknown --output /tmp/noon-native-text-wasm
```

Use a **new output directory** for every invocation. The runner refuses an
existing directory instead of quietly reusing compiler output. Native provider
tests use a host-supplied DejaVu Sans font; install `fonts-dejavu-core` on Debian
or Ubuntu, or supply the corresponding file explicitly. Fonts are test input,
not dependencies of the external consumer. WASM tests are compiled with
`--no-run`; this gate does not claim browser execution. Normal product browser
and renderer workflows remain responsible for that qualification.

The runner retains the active normal/build dependency tree, feature tree, exact
resolved lockfile, Rust version and measurement JSON. It fails on malformed or
empty graph output and rejects forbidden provider/asset packages and activated
font features. Typst itself requires the base `typst-assets` package (ICC, ICU,
HTML and PDF resources, including PDF standard-font data). The Typst-only cell
therefore asserts that `typst-assets/fonts` is **disabled**, rather than falsely
claiming that all upstream assets disappear. Minimal and native-text-only cells
exclude the entire assets package; native-bundled must enable its `fonts` feature
without selecting the Typst compiler. A lockfile can
mention inactive optional packages; the **active tree** is the isolation proof,
not text search over the lockfile or `cargo metadata`'s package inventory.

## Measurement interpretation

The measured command is a dev-profile build with debug info and incremental
compilation disabled, a fresh target directory, and no compiler-cache wrapper.
Registry downloads are prefetched and excluded from build timing. Cold means no
compiled output; warm means the identical no-edit build. Artifact bytes and gzip
bytes describe that same geometry executable, not a renderer, application
bundle, peak memory, or optimized release binary. Unused provider code can be
removed by the linker even when Cargo must compile it, so dependency count and
linked size need not move together. A single run on shared CI infrastructure is
observational evidence, not a statistically established speedup.

To compare against a checkout before optional features existed, the runner
creates a separate baseline manifest with defaults disabled and copies only the
unchanged geometry program:

```sh
python3 scripts/provider-features.py --config minimal \
  --baseline /path/to/base-checkout --target x86_64-unknown-linux-gnu \
  --output /tmp/noon-baseline-native
```

CI runs five small consumer configurations on two compilation targets plus two
base measurements for relevant PRs. It does **not** multiply the full product
suite by those combinations. The extra maintenance is the feature table, graph
expectations, provider-input tests and these ten isolated compile cells. Existing
all-feature Rust and normal browser/native product checks remain in place. The
PR's artifacts contain measurements; no fixed speed/size promise is encoded here.
