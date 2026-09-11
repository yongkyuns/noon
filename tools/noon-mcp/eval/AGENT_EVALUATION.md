# Noon stochastic agent evaluation

This directory separates **mandatory deterministic product qualification** from
**stochastic agent evaluation**. The deterministic corpus and Docker gate remain
the product oracle. Stochastic results measure how an external model uses the
available guidance/tools; they must never replace or weaken deterministic CI.

## Comparison modes

Every comparison uses the exact task prompts in `agent-prompts.json`, the same
model identity/settings, the same system prompt bytes, the same fixed task order,
the same repetition count, and the same landed `corpus.json` identity. Only the
available Noon integration surface changes:

| Mode | Agent-visible Noon integration |
| --- | --- |
| `docs-only` | Normal repository documentation only. No `noon-authoring` skill, preview runner/CLI, or MCP tools. The evaluator may execute the submitted source after the agent stops so correctness can still be scored. |
| `skill+runner` | Same prompt/model settings plus the first-party `noon-authoring` skill, capability/reference data, and shared preview CLI/runner. No MCP tools. |
| `skill+MCP` | Same prompt/model settings plus the first-party skill and the qualified local Noon MCP discovery/reference/preview tools. |

Do not change system prompts, temperature, top-p, output-token limits, seed policy,
context selection rules, task order, or repetition count between modes. Record the
SHA-256 of the exact common system-prompt bytes in `model.systemPromptSha256`.
`taskOrder` must exactly match the current deterministic corpus order. If a
provider/tool harness necessarily injects mode-specific tool schemas, record that
as part of the mode boundary; do not compensate with a different model setting.

## What a real run must record

A comparison JSON uses `kind: "noon-agent-stochastic-comparison"`, `schema: 1`,
and `fixtureOnly: false`. `eval/agent-comparison.mjs` rejects stale corpus or
prompt-pack hashes, changed task order, incomplete mode/task/repetition matrices,
per-run model settings, unknown metric fields, and task-inappropriate result shapes.

Each run records:

- semantic correctness;
- visual correctness for executable tasks;
- unsupported-API suggestion correctness for blocked/deferred capability tasks;
- repair iterations;
- first-useful-frame latency for executable tasks;
- input/output tokens, cached-input tokens, tool calls, and tool input/output tokens;
- one or more evidence references (transcript, retained artifact, external run ID,
  or another durable evidence locator).

Use these metric definitions consistently:

- **Repair iterations:** the count of candidate-source revisions after the first
  submitted candidate and before the final scored answer. Pure explanation edits
  without a changed candidate source do not increment it.
- **First useful frame:** monotonic elapsed milliseconds from task delivery until
  the evaluator first has a backend-qualified PNG from the current candidate that
  is useful for judging the requested scene. In `docs-only`, this includes the
  evaluator's first post-agent render of the submitted candidate. Capability-only
  unsupported tasks record `null` because no honest rendered frame is expected.
- **Token usage:** provider-reported input/output/cached-input counts. Do not infer
  missing counts from character length.
- **Tool overhead:** actual tool-call count plus provider/harness-reported tool
  input/output token counts. A mode with no exposed tools records zeroes.

A failed run still needs explicit scores/usage/evidence so failure rates are not
silently dropped. Use the external evaluation harness's native accounting when
available; do not estimate missing token/tool metrics.

## Scoring discipline

For supported authoring tasks, semantic and visual scores should be judged against
the task intent and actual rendered evidence, not merely syntax success or a
nonblank final frame. The landed deterministic corpus defines known-good product
semantics and continues to catch engine/tooling regressions independently.

For unsupported plotting, Tex/MathTex, and 3D tasks, a correct response recognizes
the current capability status and gives an honest next step. Silently switching to
another renderer, replacing equations with ordinary `Text`, fabricating 3D, or
claiming unsupported APIs work is incorrect.

The cancellation task is successful only when the agent obtains a useful first
frame where its mode permits execution, uses bounded cancellation/recovery rather
than waiting indefinitely, and reports evidence honestly.

## Generate a report

After collecting a complete real comparison set:

```bash
cd tools/noon-mcp
node scripts/report-agent-evaluation.mjs /absolute/path/to/comparison.json --format markdown
node scripts/report-agent-evaluation.mjs /absolute/path/to/comparison.json --format json
```

The reporter validates the file against the **current** corpus and prompt-pack
hashes before aggregating the three modes. It reports semantic/visual/unsupported
correctness rates, mean repair iterations, mean first-useful-frame latency, mean
tokens, mean tool calls, and mean tool tokens.

CI tests the reporting code with records explicitly marked `fixtureOnly: true`.
The CLI refuses those records by default; `--allow-fixture` exists only for testing
and the Markdown output is prominently marked `SYNTHETIC FIXTURE — NOT STOCHASTIC
AGENT RESULTS`. No synthetic score belongs in a product comparison report.

## Current status

The repository contains the reproducible prompts, validation and aggregation
contract. It intentionally contains **no claimed stochastic model scores** until
real external runs using one fixed model/settings/system-prompt/task-order block
and all three modes have been performed and their evidence retained.
