# Educational scene authoring

Use this guide when the scene is meant to explain something. Apply the existing
[gallery editorial contract](../../../docs/gallery-curation.md) to ordinary
authoring: one primary outcome, a short storyboard, meaningful motion, a readable
end state, and actual review. Use the current supported APIs and preview boundaries
in [authoring](authoring.md) and [verification](verification.md).

## Contents

- [Write a small brief](#write-a-small-brief)
- [Choose evidence and sequence](#choose-evidence-and-sequence)
- [Narration and revision](#narration-and-revision)
- [Worked example: One Extra Column](#worked-example-one-extra-column)
- [Review and report](#review-and-report)

## Write a small brief

Record these decisions before implementing a new explanation. Infer reasonable
defaults from the request; ask only when an unresolved choice changes the lesson.

| Decision | Useful form |
| --- | --- |
| Audience and prerequisites | What the viewer already understands, and the terms that need introduction. |
| Learning outcome | One thing the viewer should be able to explain or predict afterward. |
| Question or misconception | The specific uncertainty the animation will resolve. |
| Visible evidence | The relationship, comparison, or changing state that supports the answer. |
| Model and assumptions | Formula or algorithm; units, bounds, simplifications, and data provenance. |
| Review question | A new case that requires using the explanation rather than remembering its wording. |

For example: a programmer who understands array indexing should be able to predict
why changing a shared-memory row stride changes bank contention while preserving
the transposed values. Show the address-to-bank mapping and the unchanged output.
Do not substitute a more dramatic movement for that evidence.

Choose one or two [teaching patterns](teaching-patterns.md) by what the explanation
must do. A pattern about expanding one operation into a system can help explain
memory accesses as well as matrix attention. Read only relevant cards and current
implementation examples; do not load an entire external video repository.

## Choose evidence and sequence

For each beat, write the claim, the visible state that supports it, what changes,
what remains available for comparison, and any narration or inspection hold. A
small Markdown table in the task notes is sufficient; this is not a scene format
or another runtime timeline.

- Introduce a concrete case before showing every case at once. Keep enough context
  visible to show where an enlarged detail belongs.
- Preserve the identity of a quantity through movement and transformation. Use
  labels as well as color; changing layout must not silently change meaning.
- Hold units and chart scales fixed during a comparison, or explain their change.
  Derive related positions and numbers from the same model where appropriate.
- Label a computed model, measured data, and illustrative values accurately.
  A plausible-looking plot or random attention weight is not a measurement.
- Use motion for correspondence, order, change, or causality. Decide explicitly
  whether simultaneous motion means concurrency or merely compares two cases.
- Inspect the motion path. Endpoint interpolation can suggest shrinking or a
  different transformation even when both endpoints are mathematically correct.
- Give the viewer time to inspect a consequential result before changing it.
  Prefer a purposeful hold to unrelated ambient motion.
- Use contrast, a local highlight, or a temporary reduction in detail to direct
  attention. Move diagrams away from text before a transfer crosses that text.
- Reserve effects and music for a stated role in the explanation. A quiet or
  silent scene is a valid design; decorative complexity is not a completion test.

Preserve literal mathematical or physical timing where it is the subject. Smooth
easing is a presentation choice, not a reason to distort a uniform velocity,
discrete state transition, rotation, or limit argument.

## Narration and revision

Plan spoken wording, visible notation, and captions together. They can differ in
form while agreeing in meaning: a formula may remain on screen while the voice
explains what its terms represent. Introduce the relevant object before referring
to it; retain it while the viewer needs to inspect it.

When the selected route supports prepared narration, use accepted audio and its
measured duration. Attach important visual actions to meaningful phrase cues, and
allow lead-ins and holds. Identify each cue as provider-timed, aligned, estimated,
or manually reviewed. A duration or words-per-minute estimate does not establish
word alignment. Do not stretch every action automatically to fill a spoken span.

Keep synthesis, recording import, decoding, and alignment in asset preparation.
Rendering and replay should consume immutable accepted assets through Noon's shared
timing. Check current support before promising this complete workflow; the existing
[audio lesson](../../../web/python/examples/background_music_lesson.md) describes
finite PCM facilities, not universal narration or ordinary-scene audio export.
[Cached narration](https://github.com/yongkyuns/noon/issues/2263) and
[educational integration](https://github.com/yongkyuns/noon/issues/2581) track the
remaining work. Never invent a `say()` API from an external engine.

For visual-only or cue-only revisions, retain accepted speech. If spoken wording,
pronunciation, voice, or delivery changes, replace the affected audio and review
alignment, captions, and dependent later timing. Keep unrelated passages reusable.

Use `noon_annotate_frame` for a retained preview artifact, or the offline
`noon-feedback` CLI for an existing preview manifest. For a human reviewer, generate
a `noon-feedback review` page, collect the downloaded note, and finalize it with
`noon-feedback import`. State the observed problem,
desired correction, and meaning/state to preserve; select an interval containing
the published capture time and, when useful, a normalized image region. The tool
retains the actual source/build/run/time/image evidence instead of asking you to
reconstruct it. See [captured frame feedback](verification.md#captured-frame-feedback).

Check the original source association before editing. Changed source makes the
feedback stale; load the preserved source and inspect the target rather than
silently reusing a timestamp or rectangle. Rehearse the affected interval through
a fresh bounded run, select the candidate capture, and retain the before/after
comparison. Keep feedback pending until reviewed. Stills cannot prove pacing,
speech correspondence, or learner understanding. This workflow does not imply a
live pause-and-capture integration, arbitrary live rollback, or cross-edit object reconciliation.

## Worked example: One Extra Column

Study the existing [CudaTransposeBanks source](../../../web/python/examples/cuda_transpose_banks.py),
its [usage and attribution](../../../tools/cuda-transpose/README.md), and
[validation contract](../../../tools/cuda-transpose/VALIDATION.md). Reuse that
maintained example instead of adding a second bank-conflict demonstration. Its
source adapts CUDA Algorithm Lab by Arash under MIT; keep the existing
[copyright and permission notice](../../../web/python/examples/cuda_transpose_banks.LICENSE).

### Brief and model

**Audience:** programmers familiar with row-major array indexing and a transpose.
Introduce a warp, a lane, a shared-memory bank, and the block barrier as needed.

**Outcome:** explain why one unused column changes the bank mapping of a column
read without changing the output values. **Misconception:** all requests to the
same bank are broadcasts, or padding changes the transpose itself.

The fixture uses one interior 32 by 32 tile, a 32 by 8 thread block, 32-bit words,
and defaults `COLUMN = 3`, `SPEED = 1.0`. `reference_trace` computes the indexing
and values for all 1,024 outputs; `service_batches` groups distinct words per bank
and allows repeated requests for the same word to broadcast. At tile-relative
base zero, the selected read uses:

```text
input value = 32 * lane + column
shared word = lane * stride + column
bank = shared word % 32
output index = column * 32 + lane
```

The indexing and values are **computed**. Row-load staging, warp arrival order,
bank-service order, and durations are **illustrative**. The animation does not run
CUDA or measure cycles or kernel speedup. Optional hardware validation remains a
separate tool; its observations do not replace the scene model.

### Follow the existing sequence

| Source stage | Evidence and instructional role | Preserve or check |
| --- | --- | --- |
| `load_tile`, `focus("copy")` | Highlight both sides of the CUDA assignment before values move into the tile. Introduce one visible transfer before repeating row groups. | Keep source focus active throughout the transfer; count 1,024 loaded words. |
| `focus("barrier")`, `observe("barrier")` | Establish that the whole tile is ready before the selected column is read. | State that illustrated warp arrival order is not recorded hardware order. |
| `move_step("read", ...)` | Retain the selected column's values as the grid opens into the numbered request tray. | Packet number labels the lane; color follows the source row group, not the thread identity. |
| `bank_replay`, `observe("banks")` | Show `word = lane * stride + column` and `bank = word % 32` before moving requests. Contrast distinct words with a same-word broadcast. | Keep the same word-to-bank model for the formula, labels, and motion. |
| `load_tile` with stride 33 | Change the declaration to add one unused float per row, then rerun the same explanation. | Present a recompile and rerun, not a live resize of a GPU array. |
| `move_step("store", ...)`, `observe("output_row")` | Move the same returned packets into one output row and show the resulting values. | Complete all shared reads before the global store; retain value correspondence. |
| `finish`, `observe("finished")` | Compare 32 versus one schematic read round and 4,096 versus 4,224 bytes per tile. | Explain the 128-byte cost; do not turn the round ratio into a whole-kernel speedup claim. |

The default [showcase manifest](../../../web/python/examples/noon_showcase_manifest.json)
provides these **authored review targets**, not new runtime observations:

| Authored time, seconds | Inspect |
| --- | --- |
| 6.29 | Row loading with the source assignment visible. |
| 31.21 | Unpadded column requests sharing one bank. |
| 64.0499998399999 | Padded transfer endpoint with one request in each bank. |
| 73 | The final comparison inside the declared 72.64–76.14 hold. |

Read current source and manifest before reusing those times, particularly after
changing `SPEED` or explanatory holds. Also inspect transition interiors and named
observer checkpoints; a clean endpoint alone can hide detached labels or paths
crossing text. The helper `narrate()` only replaces **on-screen captions**. The
existing lesson contains no generated voiceover or verified spoken alignment.

### Review evidence and transfer

The [poster evidence](../../../web/thumbnails/showcase/showcase-cuda-transpose-evidence.json)
records a WebGPU frame at requested and published time `64.0499998399999`, runtime
source revision `391fa2b9841ea0c1bd8d5c7985916446dd5a3086`, and
`cudaValidated: false`. Check its source/image hashes before using it. This is
historical source-bound evidence, not a current-run pass or a learner study. Use
the maintained validation commands for a new execution claim.

Ask the learner: **with stride 34, do all 32 lanes still reach different banks?**
Under this fixture's model, offsets advance by two modulo 32, so 16 banks each
receive two distinct words and require two schematic rounds. Ask them to predict
that mapping before showing it. This checks use of the rule beyond memorizing
the 32-to-33 example.

For a different subject, apply the same explanatory structure to a dependency
graph: establish one item, preserve its identity through a dependency, expand to
several items, show a blocked dependency, change one relationship, and compare the
outcome. Define that graph's actual rules and choose current supported Noon APIs;
do not import CUDA timing assumptions into the new domain.

## Review and report

Check factual/model accuracy, visual correspondence, readable labels and scales,
motion semantics, and sufficient inspection time separately. If speech exists,
listen to technical pronunciation and inspect the referenced visual at each
important cue. Check both transition interiors and held results at the intended
viewport; review full playback for pacing.

Report the source and pattern used, exact capability/validation work performed,
actual artifacts inspected, supported host/replay/export restrictions, and an
unresolved problem when one remains. Separate source plausibility, runtime
correctness, visual quality, and learner understanding. Obtain a learner answer
when feasible, and report it only when it was actually observed.
