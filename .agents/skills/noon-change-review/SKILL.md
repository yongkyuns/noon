---
name: noon-change-review
description: Perform source self-review of a Noon PR, branch, or working-tree change against its requirements, architecture, and validation evidence. Use before reporting a change ready or when the user requests review; preserve Noon's prohibition on automated Codex reviewers.
---

# Review a Noon change yourself

Read [AGENTS.override.md](../../../AGENTS.override.md) and [AGENTS.md](../../../AGENTS.md). Perform this source review yourself. Do not invoke another review agent, request an automated Codex review, or change review automation settings. Normal local validation and CI remain applicable.

## 1. Define the candidate

Identify the repository, target branch, and intended change from the user's request or PR. Infer routine context from the repository; ask only if an unresolved choice materially changes the review.

**Committed PR or branch**

1. Fetch the live PR head and actual target branch ref through the available GitHub connection or Git CLI. Do not assume PR base metadata is the current target tip.
2. Record immutable head and target commit SHAs. Obtain their merge base and review that merge base through the pinned head. For a local checkout, use `git merge-base` and `git diff <merge-base-sha> <head-sha> --`; resolve refs before passing them as arguments.
3. Inspect the complete changed-file list, additions, deletions, renames, and relevant surrounding code. Identify binary assets, generated output, and files absent from a truncated API diff; retrieve or render them separately as needed.
4. Keep unrelated local edits outside this committed candidate. Do not discard or stage them.

**Current working tree**

Run the following from the repository root. The first line selects uncommitted-only scope. For a whole-branch review, replace that assignment with the verified merge-base SHA of the head and actual target.

```bash
review_base="$(git rev-parse --verify 'HEAD^{commit}')"
git status --short
git diff --name-status "$review_base" --
git diff "$review_base" --
git diff --cached "$review_base" --
git diff --
git ls-files --others --exclude-standard -z
```

Use a merge base with the target for the whole branch, or `HEAD` when the request concerns only uncommitted work. The commit-to-working-tree diff includes the final tracked content; the cached and unstaged diffs explain how the index differs from it. Read every relevant nonignored untracked file explicitly, parsing NUL-delimited names safely. Inspect binary assets with suitable tools. Do not stage files merely to expose them to a review.

State whether the candidate is the final working tree or a staged-only commit. If the user requested staged-only review, compare the index with its intended parent and report later unstaged edits separately. An empty committed diff does not establish an empty working-tree change.

Record commit SHAs and status together with the inspected diff content and hashes of relevant untracked or binary files. Status alone cannot detect edits to an already modified file. Recheck before the conclusion; refresh affected inspection and validation if the candidate moves.

## 2. Recover intent and authority

Read the originating request, issue acceptance criteria, relevant documentation, and changed public interfaces. Use [docs/architecture.md](../../../docs/architecture.md) as the sole architecture and roadmap authority. Check whether source or old tests describe a migration baseline rather than the target design.

If no written spec exists, state which user requirements are available and what cannot be checked. Do not infer the intended behavior solely from the implementation under review.

## 3. Make separate inspection passes

**Requirements:** Trace each material requirement to implementation and evidence. Look for omitted cases, incomplete frontend exposure, changed defaults, unintended extra behavior, and documentation that promises more than the implementation supports.

**Correctness and architecture:** Follow the changed code and its callers, including failure paths. Select risks relevant to the change:

- One semantic authority and shared Rust behavior; direct typed native and Rust/WASM execution.
- Ownership, stale generational handles, atomic mutations, rollback, coherent revisions, and resource retirement.
- Pause/seek/restart/cancel behavior, logical completion, callback ordering, and stale asynchronous results.
- Rust lifetime, synchronization, `unsafe`/FFI assumptions, and platform-specific behavior.
- Locality, bounded queues/history, allocation and retained-resource behavior; measure performance claims instead of inferring them from code shape.
- Supported Rust/Python semantic equivalence and ordinary public-path examples.
- For website or prose changes, use [noon-documentation](../noon-documentation/SKILL.md) for the affected content and UI.

Treat generic refactoring preferences as suggestions. Preserve justified low-level tests and established domain terms. A larger interface test does not automatically replace focused numerical, ordering, or lifecycle coverage.

**Validation:** Inspect commands, results, artifacts, and their exact revision. Follow [the CI guide](../../../.github/ci/README.md), [scripts/check.sh](../../../scripts/check.sh), and applicable owning-issue gates. Distinguish local checks from required GitHub checks; compilation from execution; screenshots from interaction; simulation from physical measurements.

## 4. Report actionable findings

Lead with defects that affect correctness or completion. For each, name severity, affected location, the violated requirement or contract, a concrete failure scenario, and the evidence. Keep requirements, correctness, and maintainability provenance visible while ordering the report by practical consequence. Avoid style findings already enforced by tooling.

If no material finding remains, say what was inspected and tested and identify any unverified area. Do not equate a clean source review with proof that every runtime case works.

Before calling a PR merge-ready, refresh the live head, target, mergeability, and applicable checks. Follow [docs/merge-train.md](../../../docs/merge-train.md) and [.mergify.yml](../../../.mergify.yml); source review does not authorize merging or weaken qualification. Act on existing user authorization without asking again for routine review steps.

See [attribution](../ATTRIBUTION.md) for the upstream workflow influences.
