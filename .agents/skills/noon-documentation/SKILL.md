---
name: noon-documentation
description: Write and improve Noon tutorials, how-to guides, API explanations, README content, architecture explanations, and documentation website UI. Use for natural technical prose, runnable Rust/Python examples, navigation, typography, diagrams, responsive layouts, themes, or accessibility of the existing Noon website. Ground claims in current source and check the rendered reading experience.
---

# Noon documentation

Help a reader complete a real task, understand the result, and find the next step.
Treat writing, examples, navigation, and presentation as one reading experience.

## 1. Establish the task and its authority

Read the applicable `AGENTS.md` and `AGENTS.override.md` before editing. Work from
the checked-out implementation and existing pages. Do not present an open proposal
or unmerged change as current behavior. Review the work yourself; do not invoke
Codex reviewers or review subagents.

Identify the audience, their starting knowledge, the task, and a concrete success
condition. Infer these from the request and nearby guides when possible. Ask only
when a missing distinction would materially change the result. Honor requested
spellings, terminology, titles, and scope.

Use repository-root paths below as the source map:

| Need | Read or edit |
| --- | --- |
| Public learning path | `docs/site/`, especially `getting-started.md`, `python.md`, `rust.md` |
| Navigation and publication | `scripts/site/navigation.json`, `docs/project-site.md` |
| Architecture and roadmap authority | `docs/architecture.md`; link to its decisions |
| Implementation explanation | `docs/type-map.md`, `REFERENCE.md`, owning source and tests |
| Website presentation | `web/site/`, `scripts/site/presentation.py` |
| Scene authoring practice | Existing `skills/noon-authoring/SKILL.md` |

Keep evolving architecture facts in their existing authority. Add a focused
explanation or link in the reader's path instead of a competing architecture guide.

## 2. Shape the page around its reader

Read [natural writing](references/natural-writing.md). Choose a primary job:
tutorial, task guide, reference, or explanation. Start with the outcome and the
minimum context needed to act. Put prerequisites before the step they affect.

For a tutorial, arrange steps as action → observable result → useful explanation.
Introduce concepts when the reader first needs them. Give working code before
advanced variations. Explain errors at the point of likely failure. End with a
specific next task; avoid repeated summaries and generic calls to action.

Use connected paragraphs for explanations, numbered steps for sequences, and
tables for comparisons or exact mappings. Prefer descriptive headings that also
make sense in search results and the contents rail.

## 3. Make every technical promise supportable

Trace APIs and commands to current source, manifests, examples, and relevant tests.
Keep working directory, setup, inputs, output, units, and platform constraints clear.
Use the existing source-inclusion mechanism for complete examples where available;
do not maintain a second handwritten copy of generated signatures or Rustdoc.

Preserve paired Rust/Python teaching examples where the capability supports both.
Explain real differences in syntax or host setup. Do not invent API symmetry or
suggest that the browser Python editor accepts Rust source.

Separate implemented behavior, measured evidence, known limitations, and planned
work. A layout placeholder does not establish renderer correctness. A native run
does not establish support for every browser, GPU backend, or export format.

## 4. Design the reading experience

Read [website design](references/website-design.md) before changing layout, CSS,
templates, navigation, diagrams, or interactive documentation controls.

Use the existing shared foundation, content layout, teal accent, typography,
themes, and interaction patterns. Make the next action and the explanation easy
to find. Improve hierarchy and spacing before adding decorative surfaces.
Match the size of the intervention to the request; do not replace the framework,
introduce a design dependency, or redesign the brand for a documentation edit.

Maintain useful deep links, page titles, heading fragments, native navigation,
code-copy behavior, and readable no-JavaScript content. Keep live examples on the
existing playground/runtime path and retain their loading and error states.

## 5. Validate the changed experience

Use [verification](references/verification.md) to select checks for the changed
surface. Run required repository gates and the smallest additional checks that
resolve the actual risk. Do not invent a test just to restate the edited sentence.

For rendered content or UI changes, build and inspect the affected page and a
representative long/code-heavy page on desktop and mobile, in light and dark
themes. Follow the reader's task with keyboard and pointer. Check links, fragments,
copying, overflow, focus, search/navigation, and reduced-motion behavior as relevant.
Inspect actual screenshots; a successful build or screenshot capture is not a
visual review. If access or prerequisites block inspection, state the gap exactly.

## 6. Finish with evidence

Explain what became clearer or easier for the reader, the files changed, commands
actually run and their results, and any remaining limitation. Distinguish layout
evidence from real runtime evidence. Report no unexecuted check as passing.

Follow `docs/project-site.md` for publication. A merged documentation change or
successful PR check does not itself publish the site; deployment remains a separate
explicitly authorized operation under the existing workflow.

Adaptation context and upstream credit: [skill attribution](../ATTRIBUTION.md).
