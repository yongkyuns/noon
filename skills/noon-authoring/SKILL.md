---
name: noon-authoring
description: Author and review Noon animation scenes using capability-qualified ManimCE-style Python or the shared Rust API. Use when a user asks to create, explain, repair, or verify a Noon animation, or when scene source imports noon. Not a generic ManimGL skill or a contract for modifying Noon internals.
compatibility: Requires a trusted Noon repository checkout, Python 3.12+, and Node 22+ for local capability/MCP tooling. Isolated preview additionally requires Docker, a built current browser package, and the repository-pinned preview runtime; capability discovery alone does not render or install dependencies.
metadata:
  version: "0.2.0"
---

# Noon authoring

Create understandable animations using the supported Noon surface, and distinguish
source that looks plausible from output that has actually been rendered and checked.
For engine changes, follow the repository's `AGENTS.md`; this skill is for authoring
scenes and does not replace `docs/architecture.md`.

## Discover before writing

Locate the user's trusted Noon checkout. Run commands below from its root, or use
an absolute path to its scripts. A copy of this skill alone is not an installed Noon
runtime. Never install another package named `noon` as a substitute.

```bash
python3 -B scripts/noon-capabilities.py --symbol Circle --symbol Transform
python3 -B scripts/noon-capabilities.py --symbol Text --symbol MathTex --symbol Axes
```

The command returns JSON from the checkout's existing inventories without importing
Manim, importing the Python scene facade, executing scene code, or starting a renderer.
Read `policy.reason`, `policy.dependency`, `policy.evidence`, `ready_examples`, and
the examples' separate `parity_status` and `qualification_mode` fields. Unknown queries
or invalid inventories fail with nonzero status; do not ignore that failure.

`exported` is static discovery, not complete behavioral support. A `ready` example
can still be a parity **candidate**. The report is a source inventory, not a guarantee
about an installed binary, the current GPU, or a live session. See
[capabilities](references/capabilities.md).

## Author and inspect

1. State the important visual states and transitions: what appears, what moves,
   what transforms, and what disappears. Include labels, layout, and holds.
2. Choose a relevant [existing example](references/examples.md), query its current
   classification, and read its actual source. Retain its provenance when reusing it.
   Prefer a narrow proven pattern to an unsupported general recipe.
3. Write ordinary `from noon import *` Python for the supported CE-like surface, or
   use the current shared Rust API. Keep the editable source as the deliverable.
   Do not import `manimlib`, invoke `manim`/`manimgl`, or silently switch renderers.
4. Prefer the qualified local preview CLI or configured Noon MCP tools below when
   available. Inspect the intended initial state, transition interiors, completion
   boundaries, and final membership. A final blank frame after `FadeOut` can be
   correct; a successful run alone is not visual verification. Apply the
   [verification checks](references/verification.md).
5. Revise source based on actual diagnostics and frames. Preserve the original
   mathematical and animation semantics; never conceal an unsupported operation by
   replacing `MathTex` with `Text` or removing an updater.

## Qualified local preview route

The checkout-bound tooling package is private and is not published to npm. Install
only its committed lockfile, with lifecycle hooks disabled:

```bash
cd /path/to/noon/tools/noon-mcp
npm ci --ignore-scripts --no-audit --no-fund
```

The preview path also needs the current browser package and the pinned local Docker
runtime. From the trusted checkout:

```bash
cd /path/to/noon
bash scripts/build-web-demo.sh

cd tools/noon-mcp
NOON_PREVIEW_CACHE=/absolute/path/to/private-preview-cache \
  node scripts/setup-preview-runtime.mjs
```

The setup helper prints the absolute `runtimeConfig` path. It pins the Playwright
container/runtime and Pyodide payload, verifies their identities, and does not make
the model choose Docker images, executables, checkout paths, seccomp profiles, or
commands. Preview execution remains local, bounded, and `--network=none` inside the
qualified Docker boundary.

For a one-shot still-frame run, use the shared CLI over `AgentPreviewService`:

```bash
NOON_PREVIEW_RUNTIME_CONFIG=/absolute/path/to/runtime.json \
  node /path/to/noon/tools/noon-mcp/bin/noon-preview.mjs \
  --source /absolute/path/to/scene.py \
  --output /absolute/path/to/preview-output \
  --loop-duration 4 \
  --time 1 --time 1.5 --time 3
```

The manifest and PNGs report observed source/build/requested-time/published-time/
backend provenance. Sampling is forward-only; use a fresh run after source edits.
Do not claim arbitrary seek or persistent live-object editing.

When an MCP host is configured to launch `tools/noon-mcp/src/server.mjs` with
`NOON_REPO`, `NOON_PYTHON`, and `NOON_PREVIEW_RUNTIME_CONFIG` as trusted startup
environment, use these tools rather than inventing a second execution route:

- `noon_capabilities` — static source/policy discovery, not a renderer claim.
- `noon_reference` — hash-checked ready example source.
- `noon_open_scene` — opens an isolated session and returns the initial PNG plus
  coherent metadata.
- `noon_sample_frames` — advances through nondecreasing times; at most 31 samples
  in one call, with the shared retained-frame budget enforced across calls.
- `noon_inspect` — metadata-only inspection of the last coherent retained state.
- `noon_close_scene` — closes the owned session; the handle becomes stale.

Session handles are transport-scoped capabilities. Treat stale/cross-scope errors,
source failures, cancellation, and quota failures as real diagnostics. Cancellation,
disconnect, and shutdown retire owned sessions/containers. Do not retry by bypassing
the service or by starting an unrestricted browser.

If Docker or the built browser package is unavailable, fall back to capability/source
review and say that rendered verification was not performed. The older local playground
remains useful for interactive development, but it is not a substitute for claiming
qualified CLI/MCP evidence:

```bash
bash scripts/build-web-demo.sh
python3 -m http.server --bind 127.0.0.1 --directory web 8080
```

## Distribution identity

For reproducible setup/evaluation work, generate the checkout-bound agent manifest:

```bash
cd /path/to/noon/tools/noon-mcp
node scripts/package-manifest.mjs
```

It records the private package/lock identities, direct dependency integrity/licenses,
skill hash/version, capability-export provenance, runner source hashes, pinned
Playwright/Pyodide identities, and environment requirements. A source-package manifest
intentionally reports no loaded runtime build identity; actual preview results must
supply the build identity observed by the running browser worker. See
`THIRD_PARTY_NOTICES.md` for runtime provenance/license notices.

## Semantic boundaries

Read [authoring constraints](references/authoring.md) before using text/math,
updaters, seeking, or Rust live changes. ManimCE is the compatibility reference;
ManimGL APIs are not interchangeable. An import-only change is a goal for supported
behavior, not a promise that every Manim recipe works.

Use authored scene time rather than wall-clock time. Python may continue authoring
after a logical segment-completion barrier; arbitrary callbacks can require host
execution. Never assume the whole future Python program was statically compiled.
Rust shares the scene semantics, not Python's interpreter: new Rust source requires
compilation or an explicit existing hot-reload mechanism.

## Completion report

Return the source, exact validation performed, observed frames/artifacts when
available, and remaining limitations. Keep declared compatibility, tests actually
run, and visual observations separate. No automatic fixes, hidden backend changes,
or claims of full parity from class names or an uninspected final image.
