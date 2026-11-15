# Merge train for `master`

Noon's PR merge train uses [Mergify's merge queue](https://docs.mergify.com/merge-queue/) rather than a custom merge bot. GitHub's [native merge queue](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/configuring-pull-request-merges/managing-a-merge-queue) is not available for the current public **user-owned** `yongkyuns/noon` repository; it would become an alternative if the repository moved to an eligible organization.

## How it works

A PR is eligible for the queue **only** with the `merge-train` label, when it is non-draft and targets `master`. Both the automatic rule and queue admission enforce that label; a manual Mergify queue command cannot bypass it. No existing PRs are automatically queued. The existing PR Fast Gate's four always-on checks and the architecture/layer ratchets must pass on the original head before Mergify accepts it.

Mergify maintains a **serial, cumulative** train, with a maximum batch of two PRs and at most two concurrent speculative checks. When new qualification is necessary, it creates temporary draft PRs containing the current `master` plus queued changes, so the existing `pull_request` workflows qualify the *combined source*, not just a stale individual branch. Mergify may direct-merge an already-qualified head if its target tree is unchanged and the existing results are reusable; this is not a new untested combined revision. The merge rule requires those same six always-on GitHub Actions checks to succeed on the queue revision. It also rejects any reported GitHub Actions failures and waits for pending checks, including path-selected Rust, WASM, browser, performance and rendering workflows. Skipped checks for irrelevant paths remain skipped. It uses the repository's current **merge-commit** history, not squash or rebase.

A failed candidate is not merged; subsequent queue entries must be tested against a valid new cumulative state. External merges into `master` cause the train to be re-evaluated. GitHub's existing branch protection rules remain authoritative; Mergify is not configured to bypass them. The `.github/ci/merge-train-config.test.mjs` policy test is exercised by PR Fast Gate's existing web preflight and detects accidental weakening of mandatory gates.

## One-time activation (repository owner)

1. Review and merge the change containing `.mergify.yml` after its ordinary PR checks pass. This file alone does **not** activate a queue.
2. In the [Mergify dashboard](https://dashboard.mergify.com/), install its GitHub App with access limited to **`yongkyuns/noon`**, and enable **Merge Queue** and **Workflow Automation** for that repository. Check Mergify's configuration-validation result. Mergify's published open-source tier is free, but verify current terms before enabling an external App.
3. Create a GitHub PR label named `merge-train`. Before the first merge, inspect GitHub **Settings → Branches / Rulesets** for `master`. Retain existing branch-protection rules and required checks; configure the six always-on checks as required if not already present. **Do not mark path-filtered jobs as globally required**: absent reports would permanently block unrelated PRs. Branch-protection settings could not be inspected through the connected GitHub App.
4. Choose one low-risk, already-qualified non-draft PR for a dry run. Apply `merge-train`, observe its queued draft PR and all applicable jobs, and confirm the exact integration SHA and merge operation before entrusting the train with other work. Test a conflicting or deliberately failing PR, and an external advancement of `master`, before treating the queue as production-qualified.

To remove a candidate before merge, remove `merge-train` and use Mergify's **dequeue** command/control. Do both: removing only the label may leave a previously queued PR in the queue. To stop merges, pause the queue in Mergify. Avoid direct merges outside the queue once adoption is complete.

## Boundary and caveats

The train changes **merge orchestration only**, not test selection, benchmark thresholds, CI timeouts, test baselines, release permissions, or GitHub Pages deployment. Normal PR workflows (including conditional browser/renderer gates) still select the relevant paths. The queue's dynamic failed/pending predicates use Mergify's [app-qualified check conditions and glob operator](https://docs.mergify.com/configuration/conditions/); their live behavior must be confirmed in the installed Mergify product. A workflow that is neither triggered nor reported cannot be qualified by a status check, so maintainers must still verify any explicit/manual qualifications required by a PR or release.

This is not proof of an active merge train until the GitHub App is installed, the repository's administrative branch settings are checked, and the first real queued integration plus failure cases have been observed. Track the activation and acceptance in [#2495](https://github.com/yongkyuns/noon/issues/2495).
