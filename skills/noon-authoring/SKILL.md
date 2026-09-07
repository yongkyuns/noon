---
name: noon-authoring
description: Author and review Noon animation scenes using capability-qualified ManimCE-style Python or the shared Rust API. Use when a user asks to create, explain, repair, or verify a Noon animation, or when scene source imports noon. Not a generic ManimGL skill or a contract for modifying Noon internals.
compatibility: Requires a trusted Noon repository checkout and Python 3.12+ for capability discovery. Previewing requires the repository's existing browser build and rendering environment; capability discovery alone does not render or install dependencies.
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
4. Use the existing playground to render the scene. Inspect the intended initial
   state, transition interiors, completion boundaries, and final membership. A
   final blank frame after `FadeOut` can be correct; a successful run alone is not
   visual verification. Apply the [verification checks](references/verification.md).
5. Revise source based on actual diagnostics and frames. Preserve the original
   mathematical and animation semantics; never conceal an unsupported operation by
   replacing `MathTex` with `Text` or removing an updater.

## Current preview route

Use an already-built playground when available. To build one explicitly:

```bash
bash scripts/build-web-demo.sh
python3 -m http.server --bind 127.0.0.1 --directory web 8080
```

Open the local playground, paste the source into **Python scene source**, and run
it. The build can require downloads and substantial dependencies; it is not a side
effect of capability discovery. Do not claim the preview ran when the required
browser/WASM environment is unavailable.

There is not yet a supported `noon render` command or shipped Noon MCP preview
server in this skill. The isolated runner and MCP adapter are tracked in #1197 and
#1198. Do not invent their installation or tool commands.

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
