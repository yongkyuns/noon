# Existing example starting points

These are references into the maintained tutorial manifest, not copied recipes or
independent support declarations. Query the current record and inspect its source:

```bash
python3 -B scripts/noon-capabilities.py --example parity-square-to-circle
```

Read the returned `repository_path` relative to the Noon checkout, not the skill's
installation directory. Use only records currently marked ready, and preserve their
candidate/qualified distinction. The skill validator rejects missing or unready
entries rather than silently omitting them.

| Example ID | What to study |
| --- | --- |
| `parity-create-circle` | Primitive creation and fill styling. |
| `parity-square-to-circle` | Create a square, morph it into a circle, then remove it. |
| `parity-square-and-circle` | Relative layout and parallel creation. |
| `parity-animated-square-to-circle` | Method animation followed by a content transform. |
| `parity-different-rotations` | Endpoint interpolation versus a rotation animation. |
| `manim-dot-example` | Minimal static geometry. |
| `manim-ellipse-example` | Arrangement of geometry; inspect the specific family case. |
| `manim-show-uncreate` | Reverse reveal and removal. |
| `parity-draw-border-then-fill-styled-square` | Outline/fill phases and intermediate appearance. |
| `manim-succession-example` | Sequential composition and timing. |

For SquareToCircle, do not demand a visible circle in the final frame: the source
ends with FadeOut. Inspect the square after creation, the transformation interior,
the circle before removal, and the final empty membership. A single nonblank-image
check would miss both an incorrect starting shape and an incorrect lifecycle.

For text, callbacks, and Rust, use the repository README's paired-example table
and current capability restrictions rather than extrapolating from these geometry
examples. Do not copy legacy Rust snapshot authoring into a new shared-API scene.

## Educational references

Use [teaching patterns](teaching-patterns.md) to select an explanatory structure,
then [the educational brief and annotated lesson](education.md) to connect the
structure to ordinary Noon source. These references do not extend the ready
tutorial table above or change `noon_reference` admission.

The existing **One Extra Column** showcase lesson is annotated in
[the CUDA walkthrough](education.md#worked-example-one-extra-column). It connects
source statements, values, addresses, and visible motion using an existing
maintained scene. Read its current showcase manifest and source-bound evidence;
do not call its historical capture a fresh validation of the current runtime.
Its helper named `narrate` creates on-screen captions, not voiceover audio.

For any showcase source referenced by a pattern, obtain support and replay limits
from current capability discovery and the showcase manifest. A gallery example,
a ready tutorial candidate, an external source study, and a newly rendered scene
have different evidence; keep that evidence explicit when reusing them.

This skill's instructions are first-party text. It adopts the general
capability-discovery and preview/inspection workflow; it does not vendor external
Manim skill code, examples, fonts, or assets. Existing fixture provenance stays in
the tutorial manifest and upstream corpus. Review licenses separately before
introducing additional external material.
