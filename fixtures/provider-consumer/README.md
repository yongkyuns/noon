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

Use a **new evidence output directory** for every invocation. Ordinary correctness
runs preserve compiler wrappers and reuse `CARGO_TARGET_DIR` (defaulting to
`target/provider-consumer`). They do not label cached builds as cold measurements.
Only explicit `--measure` runs create a fresh target under the evidence directory
and disable compiler wrappers/incremental compilation. Native provider
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

Cold/warm measurement is opt-in with `--measure`; it is not part of routine PR
correctness. The measured command is a dev-profile build with debug info and incremental
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
python3 scripts/provider-features.py --measure --config minimal \
  --baseline /path/to/base-checkout --target x86_64-unknown-linux-gnu \
  --output /tmp/noon-baseline-native
```

Relevant PRs run five small consumer configurations on two compilation targets
using the repository's pinned, read-only shared `sccache`, without forcing fresh
compiler output. The full product suite is **not** multiplied by those combinations.
Manual `workflow_dispatch` with `measure=true` runs the cold/warm matrix plus two
baseline builds against `baseline_ref`; measurement reporting is coordinated with
#1265 rather than charged to every edit. The extra maintenance is the feature
table, graph/cache-mode expectations, provider-input tests and ten isolated compile
cells. Existing all-feature Rust and normal browser/native checks remain in place.
Artifacts distinguish `qualification.json` from opt-in `measurements.json`; no
fixed speed/size promise is encoded here.

## Public authoring error contract

The provider-free `authoring_errors` integration tests exercise typed handle
validation and authored/live Scene membership operations. `Scene::add`,
`remove`, `add_many`, `remove_many`, `clear`, `replace` and `edit_membership`
return `AuthoringError`. `Mobject::validate` and `MobjectFamily::validate` retain
semantic node or resource identities. Existing `LiveSessionError` categories
continue to distinguish foreign stores, publication/segment barriers and stale
publications; its `Authoring` variant retains the membership or handle cause.
`std::error::Error::source` preserves nested typed diagnostics.

This is a bounded R2 slice, not a claim that every authoring API is typed yet.
Other geometry/animation APIs and the current language bridge may still expose
strings, and public export/raw-store narrowing remains under #958. Ordinary
post-bootstrap edits should use `scene.live(&mut session)`. Direct authored/raw
store edits invalidate an existing execution revision; rejection is intentional,
not a signal to bypass revision checks. Tests use raw access only to construct
stale-generation and callback-precondition fixtures.
