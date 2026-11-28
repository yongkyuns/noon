---
name: noon-retro
description: Improve Noon's development workflow after an observed failure, repeated investigation, or requested session retrospective. Use to identify specific improvements to checks, navigation, evidence, and agent instructions; do not run as ceremony after every small change.
---

# Improve the workflow from observed work

Read [AGENTS.override.md](../../../AGENTS.override.md) and [AGENTS.md](../../../AGENTS.md). Ground the retrospective in the specified session, issue, diff, logs, and validation output. If the user names no earlier session, use the current work rather than inventing history.

## 1. Reconstruct the friction

Identify the concrete failure or avoidable effort, its consequence, and the evidence. Separate product defects, missing environment capabilities, unclear instructions, and failures to follow existing instructions.

Examples worth investigating include:

- A test passed while the original playback or rendering defect remained.
- Repeated exploration missed an existing shared Rust operation or validation command.
- A check existed but was not wired into the normal validation entry point.
- A source change was reviewed at the wrong revision or omitted dirty files.
- An architecture choice was repeatedly reopened because its rationale was hard to find.
- A website change passed static checks but failed in a browser or at a narrow viewport.

Use these as prompts for evidence, not findings to assert in every session.

## 2. Choose the smallest useful intervention

Inspect the current implementation before recommending new machinery:

| Observed problem | First place to inspect | Prefer |
| --- | --- | --- |
| Missed behavior or invariant | Existing tests and [validation strategy](../../../docs/architecture.md#15-validation-strategy) | A focused assertion or witness that detects the actual fault |
| Unrun or hidden check | [scripts/check.sh](../../../scripts/check.sh), [.github/ci/README.md](../../../.github/ci/README.md), applicable workflow | Wire or repair the existing check |
| Slow navigation | Existing docs, source naming, and `AGENTS.md` links | A precise pointer or clearer ownership location |
| Repeated design confusion | Relevant section of [docs/architecture.md](../../../docs/architecture.md) | Clarify the existing authoritative decision |
| Repeated writing/UI problem | [noon-documentation](../noon-documentation/SKILL.md), [project-site guide](../../../docs/project-site.md) | A concrete example or missing verification step |
| Expensive tool use | Commands and outputs from the session | Narrower queries, reusable evidence, or better access to the needed signal |

Use deterministic checks for mechanical requirements. Reserve prose for judgment, context, and reasons that tooling cannot establish. Check whether a proposed automation duplicates a working gate or adds more burden than the observed problem warrants.

Keep architecture and roadmap authority in `docs/architecture.md`. Keep task status in issues. Update skill instructions only when the new wording changes a demonstrated behavior; remove obsolete guidance rather than accumulating contradictory rules.

Preserve Noon's review policy. Do not recommend enabling automated Codex review or security review as a workflow fix. Improve source self-review, normal tests, CI, and evidence instead.

## 3. Recommend or implement within scope

Present a short prioritized set of changes. For each, explain the observed problem, the proposed edit, the expected benefit, and a verification method. Include the source location so another session can act without rediscovering it.

If the user requested a retrospective only, provide recommendations. If workflow fixes are already authorized, make the small concrete improvements and validate them without asking again. Do not treat this skill as permission to change branch protection, enable services, publish, merge, or send messages.

Prefer a few evidence-backed improvements to a new framework. Stop once the observed problems are addressed; do not expand into unrelated repository cleanup.

## Report

Separate changes actually made from suggestions. Name the check or realistic task used to verify each change, and record any remaining constraint honestly. Do not claim faster work or fewer defects without comparative evidence.

See [attribution](../ATTRIBUTION.md) for the upstream workflow influences.
