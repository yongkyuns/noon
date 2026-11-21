---
name: noon-design
description: Resolve Noon API, ownership, module-boundary, and architecture decisions using concrete alternatives and caller examples. Use for an unsettled design or recurring architectural friction; preserve accepted decisions and record architectural changes in the existing authoritative document.
---

# Design a Noon change

Read [AGENTS.override.md](../../../AGENTS.override.md), [AGENTS.md](../../../AGENTS.md), and the relevant sections of [docs/architecture.md](../../../docs/architecture.md). Treat existing code as implementation evidence and the architecture document as the design authority.

## 1. Frame the decision

Identify the user-visible capability or concrete maintenance problem, the affected callers, and the layer that owns the behavior. State the constraints and acceptance evidence before proposing a structure.

Carry forward settled requirements and user preferences. Resolve implementation details yourself. Ask focused questions only when an unresolved product or architectural choice materially affects the result.

Use the existing architecture's terms and definitions. Check ambiguous terms against the document and actual call sites. Surface material disagreements between intended and implemented behavior; do not silently treat an old implementation as a new requirement.

## 2. Explore meaningful alternatives

For a consequential interface decision, sketch two or three genuinely different approaches. For a local, obvious correction, explain the direct choice and proceed without manufacturing alternatives.

Consider different priorities: make the common caller simple, minimize the exposed contract, or retain necessary extension points. Evaluate all candidates against the same requirements; do not make one candidate artificially weak.

For each candidate, show:

1. A short ordinary Rust calling example. Include equivalent Python usage when shared public semantics are involved.
2. Types and operations the caller must understand, including ownership, lifetimes, errors, ordering, and cancellation where relevant.
3. The semantic/runtime/renderer/host responsibilities it hides and exposes.
4. Dependency and target consequences, including direct native and Rust/WASM paths.
5. The evidence that would distinguish it from the alternatives: an executable example, invariant test, or focused measurement.

Use a small prototype only when it answers a specific uncertainty. Keep it outside the production design until qualified, and remove temporary scaffolding when that question is resolved.

## 3. Judge the tradeoffs

Prefer a coherent interface that removes complexity for callers while preserving clear ownership. Explain the cost in concrete terms rather than ranking designs by the number of types or lines alone.

- Keep common semantic behavior in shared Rust. Frontend ergonomics may differ without duplicating scene or runtime truth.
- Preserve ordinary typed in-process boundaries. Introduce transport only at an actual external or cross-context boundary.
- Respect continuation and logical completion contracts; choose host mechanisms that preserve them without blocking browser servicing.
- Make a local operation's work proportional to its actual affected state. Identify hidden full-scene scans, unnecessary relowering, or repeated resource transfer.
- Justify a new crate through dependencies, compilation targets, or reuse. Use modules for conceptual organization alone.
- Evaluate ownership, ABI, testing, and dependency isolation when judging an abstraction; implementation count alone does not decide its value.
- Preserve behaviorally valuable tests when reshaping modules. Compare fault coverage before retiring old tests.
- Remove obsolete greenfield compatibility paths within the task's scope. Give unavoidable temporary migration code an explicit deletion owner and condition.

Recommend a candidate and explain why its caller experience and implementation costs fit the task. Identify remaining uncertainty and the smallest useful next experiment.

## 4. Record the accepted decision once

When architecture changes, update the relevant section of `docs/architecture.md` with the chosen contract and the tradeoff needed to understand it. Keep terminology there when it is architectural. Do not introduce a competing architecture document, glossary authority, or ADR collection.

Keep implementation slices, acceptance criteria, dependencies, and remaining qualification in the owning GitHub issue. Prefer slices that can demonstrate useful behavior through the normal public path; for broad mechanical changes, state an explicit migration sequence rather than claiming each partial state is independently complete.

For ordinary local choices, a concise issue or PR explanation is sufficient. Avoid recording every implementation detail as a lasting design rule.

## Finish

Implement within the user's authorized scope when the decision is settled. Report the selected behavior, examples or evidence, and where a lasting decision was recorded. Use [noon-change-review](../noon-change-review/SKILL.md) for source self-review before reporting implementation complete; it does not invoke another reviewer.

See [attribution](../ATTRIBUTION.md) for the upstream workflow influences.
