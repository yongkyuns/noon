## Roadmap ownership

- Owning issue/case: <!-- e.g. #957 A1.2 -->
- Architecture layer: <!-- Semantic Scene / Execution Plan / Runtime / Renderer / frontend / tooling -->

## What changed

<!-- Keep this focused on the behavior/architecture this PR changes. -->

## Architecture check

- [ ] I read the relevant parts of `docs/architecture.md` and `AGENTS.md`.
- [ ] This PR does not introduce a second scene authority, semantic ID space, scheduler, runtime, renderer authority, or feature-specific mutation system.
- [ ] The Rust-native path remains typed and in-memory; no JSON/serialization/frontend bridge became an ordinary engine boundary.
- [ ] Shared semantics live in Rust rather than a Python/JS parallel implementation.
- [ ] Local operations remain local through lowering, runtime, renderer, and GPU/resource updates.

## Migration seam

<!-- Write `None`, or name every temporary compatibility/migration seam and the issue that deletes it. -->

## Validation

<!-- Exact commands/tests run, plus any focused invariant/performance coverage added. -->

## Cleanup and handoff

- [ ] Obsolete code made unnecessary by this change was deleted where in scope.
- [ ] Any temporary compatibility code has an explicit deletion owner.
- [ ] The owning issue/PR description says what landed and what remains.
