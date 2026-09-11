---
name: noon-authoring
description: Author and review Noon animation scenes using capability-qualified ManimCE-style Python or the shared Rust API. Use when a user asks to create, explain, repair, or verify a Noon animation, or when scene source imports noon. Not a generic ManimGL skill or a contract for modifying Noon internals.
compatibility: Requires a trusted Noon checkout, Python 3.12+, and Node 22+. Discovery does not render. Isolated CLI/MCP preview additionally requires Docker and a prepared NOON_PREVIEW_RUNTIME_CONFIG; the local runner is not a remote or malicious-checkout sandbox.
metadata:
  version: "0.1.0"
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
4. Render through the qualified shared preview path when it is configured. Inspect
   the intended initial state, transition interiors, completion boundaries, and final
   membership. A final blank frame after `FadeOut` can be correct; a successful run
   alone is not visual verification. Apply the [verification checks](references/verification.md).
5. Revise source based on actual diagnostics and frames. Preserve the original
   mathematical and animation semantics; never conceal an unsupported operation by
   replacing `MathTex` with `Text` or removing an updater.

## Current preview route

The supported file-based agent preview is the shared checkout-bound CLI. Prepare the
same isolated runtime used by MCP, then render selected authored times:

```bash
bash scripts/build-web-demo.sh
cd tools/noon-mcp
npm ci --ignore-scripts --no-audit --no-fund
node scripts/setup-preview-runtime.mjs
# Export the absolute runtimeConfig path printed above.
export NOON_PREVIEW_RUNTIME_CONFIG=/absolute/path/to/runtime.json
cd ../..
node tools/noon-mcp/bin/noon-preview.mjs \
  --source scene.py --output noon-preview-output \
  --time 1 --time 1.5 --time 3
```

The CLI uses the same `AgentPreviewService`, isolated Docker runner, retained artifact
store, and observed runtime provenance as MCP. Read `manifest.json` and inspect the
actual `frame-*.png` files; do not substitute a successful process exit for visual
verification. Sampling is forward-only and bounded, and success/failure both await
session/container cleanup.

When the existing local MCP server is launched with that same
`NOON_PREVIEW_RUNTIME_CONFIG`, use its shared rendering tools rather than inventing
another render path:

1. `noon_open_scene` with the complete source;
2. `noon_sample_frames` with a nondecreasing schedule of useful authored times;
3. `noon_inspect` for the last coherent metadata when needed;
4. `noon_close_scene` when finished.

`open`/`sample` return actual `image/png` content plus retained descriptors carrying
source/build/requested-time/published-time/backend provenance. Session handles are
scope-local and become stale after close/cancellation. A new source edit starts a new
scene session; persistent live object editing is not implied.

If the isolated runtime is unavailable, the existing browser playground remains a
manual fallback. Build it with `bash scripts/build-web-demo.sh`, serve `web/` locally,
and report that the qualified CLI/MCP preview was unavailable. Do not claim rendering
occurred when only capability discovery or source inspection ran.

For a reproducible source bundle of this skill plus the canonical capability inventory,
use `node scripts/package-noon-agent.mjs --output <new-directory>` and verify it with
`node scripts/package-noon-agent.mjs --verify <bundle-directory>`. The bundle identifies
checkout runner source; actual loaded worker/WASM identity is still observed from each
preview artifact rather than fabricated from source metadata.

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
