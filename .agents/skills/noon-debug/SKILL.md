---
name: noon-debug
description: Diagnose and fix Noon rendering, authoring, playback, audio, export, or performance failures. Use for a reported regression or reproducible incorrect behavior; use noon-design for an undecided interface and noon-documentation for prose or website work.
---

# Debug a Noon failure

Read [AGENTS.override.md](../../../AGENTS.override.md), [AGENTS.md](../../../AGENTS.md), and the relevant sections of [the architecture](../../../docs/architecture.md). Use the current issue and source to locate the owning layer. Run commands from the repository root.

## 1. Establish the failure

- State the user's observable symptom and expected behavior in one or two sentences. Carry forward decisions already made; investigate routine implementation details yourself.
- Record the revision, entry point, scene or fixture, target, renderer/backend, and exact event sequence. Include native/browser, Rust/Python, and export/live playback distinctions when they matter.
- For playback, identify the clock and state transition involved: authored time, wall time, segment completion, presentation, pause, seek, cancellation, or restart. Use the architecture's definitions.
- Preserve a small failing scene, trace, assertion, image, or measurement before editing. Redact credentials and unrelated user data from logs and artifacts.
- Resolve material ambiguity with a focused question only when the available code, issue, and evidence cannot settle it.

## 2. Build the smallest useful feedback loop

Choose an existing test, fixture replay, ordinary authoring example, browser interaction, or native capture that reaches the failure. Inspect [the CI guide](../../../.github/ci/README.md) and [the authoring skill](../../../skills/noon-authoring/SKILL.md) for current entry points instead of inventing commands or APIs.

- Demonstrate that the loop fails for the reported reason. A compilation failure, missing GPU, absent browser, or unavailable tool is an environment problem, not reproduction of the product defect.
- Prefer deterministic input and a quick loop, but allow expensive rendering, rare races, and platform-specific failures. Use code inspection and provisional hypotheses to construct a better reproducer when necessary.
- For intermittent failures, record attempts, failure frequency, seeds/event order, and relevant scheduling conditions. State the limits of any substitute environment.
- Reduce the scene or event sequence while preserving the symptom. Retain the original scenario for final verification.
- For visual or audible failures, inspect the actual output at the relevant times. A passing data assertion or successful file write alone does not establish visual quality or correct audio.

Finish this phase with an observed failure, or a precise account of what could not be reproduced and the evidence still available. Do not label an unobserved fix as verified.

## 3. Test explanations

List a small number of plausible causes, the observation that distinguishes each, and the cheapest experiment. Change one relevant factor at a time. Instrument the semantic/runtime/renderer/host boundary that can answer the question; avoid unrelated logging.

For performance changes, compare the same workload, build mode, target, backend, and measurement conditions. Separate CPU time, wall time, allocation/resource growth, bridge traffic, and GPU work. Investigate whether local changes trigger work proportional to the whole scene. Report variability and instrumentation overhead where material.

Follow evidence across ownership boundaries:

- Check shared Rust semantics before adding wrapper-specific behavior.
- Distinguish authored state, runtime effective state, and derived renderer resources.
- Check coherent publication, stale identities/results, cancellation, resource retirement, and continuation barriers when the failure crosses a lifetime or timing boundary.
- Identify genuine cross-context transport; a browser target alone does not justify another serialized engine path.

Update or reject each hypothesis using the resulting observations. Keep the explanation no broader than the evidence supports.

## 4. Fix and verify

Fix the responsible layer and remove temporary instrumentation that no longer helps. Preserve existing architectural and behavioral coverage; retire a test only after identifying which replacement protects its useful invariant.

Add or update a focused regression test when it provides lasting fault detection. Exercise observable contracts, including ordering and lifecycle when those are part of the contract. Use an independent expected result; avoid computing the expectation through the same faulty algorithm.

Where practical, show the regression test fails before the fix and passes afterward. Then rerun the original scenario. For supported common Rust/Python semantics, maintain the paired executable evidence required by the operating contract. Include native and direct Rust/WASM checks when the changed behavior spans those paths.

Read [scripts/check.sh](../../../scripts/check.sh) before choosing the completion gate. Use focused checks during iteration and the operating contract's required mode before completion. Keep required GitHub qualification separate from local results. For site failures, follow [the project-site workflow](../../../docs/project-site.md).

## Report

Explain the cause, the behavior changed, the evidence before and after, and any remaining limitation. List the exact commands and environments actually exercised. Distinguish compiled, executed, rendered, heard, and measured evidence. Link the owning issue or PR when one exists.

See [attribution](../ATTRIBUTION.md) for the upstream workflow influences.
