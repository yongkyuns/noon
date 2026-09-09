# Noon agent override

## No automated Codex reviews

Codex code review and Codex security review are disabled for this repository.

- Never request, invoke, trigger, or schedule Codex code review or security review.
- Never post or generate `@codex review` or `@codex security review` comments.
- Never enable Codex automatic review, personal review triggers, repository review automation, or team review automation.
- Do not interpret “review”, “ready for review”, “review the PR”, “review and merge”, “PR ready for review”, or similar wording as permission to invoke another agent or automated reviewer. Perform the requested source review yourself.
- Do not add `## Code Review Rules` or other Codex-review-specific instructions to `AGENTS.md` or nested agent instruction files.
- If an external Codex review setting is found enabled, treat that as unwanted configuration and report it rather than relying on it.

Normal repository validation, CI, self-review, human review, and merge-readiness checks remain unchanged.
