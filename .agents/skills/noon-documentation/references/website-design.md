# Design and review the Noon documentation website

## Begin with the existing page

Read `docs/project-site.md` and open the affected page before editing. Identify the
specific obstacle: unclear starting point, weak heading hierarchy, cramped example,
hidden navigation, inaccessible control, or missing explanation. Capture a baseline
when a visual comparison will help. Choose the smallest coherent improvement.

| Surface | Existing owner |
| --- | --- |
| Shared colors, font stacks, header, focus and responsive primitives | `web/site/foundation.css` |
| Documentation columns, reading rhythm, code and tables | `web/site/docs.css` |
| Landing page and live examples | `web/site/site.css`, `web/site/live.css`, `web/site/index.html` |
| Generated page markup | `scripts/site/presentation.py`, `scripts/site/build_docs.py` |
| Search, navigation and documentation controls | `web/site/docs.js`, `web/site/documentation-ui.js` |
| Initial and selected theme | `web/site/theme-init.js`, `web/site/theme.js` |
| Guide order and groups | `scripts/site/navigation.json` |

Use the current `--paper`, `--ink`, `--muted`, `--rule`, `--panel`, `--accent`,
`--font`, and `--mono` tokens and their theme definitions. Extend a shared pattern
when needed; do not scatter hard-coded substitutes through pages or add remote
fonts, icon kits, a CSS framework, or a separate design system for a small change.

## Make the content hierarchy visible

Give the page one clear title, a concise purpose, and an obvious first action.
Keep prose at the existing readable measure. Use spacing, type scale, weight, and
alignment to distinguish title, sections, body, captions, and controls. Match
neighboring pages so a reader can transfer what they learn about navigation.

Keep the article the main visual surface. Use the sidebar for topics and the
contents rail for this page; neither should compete with the explanation. Prefer
plain text links or a compact task table for navigation. Use cards only when a set
of independent destinations benefits from them, not for every paragraph. A large
hero, decorative gradient, floating panel, or repeated CTA needs a task-specific
reason. Let the actual animation carry visual interest where it teaches the topic.

Long code blocks and tables may scroll within their own container. Keep labels,
copy controls, error messages, captions, and surrounding prose visible. Do not
shrink body text or clip the entire page to hide overflow. Wrap prose and long URLs;
retain code whitespace where it matters. Ensure copied code excludes prompt marks,
line numbers, hidden UI, and unrelated output.

## Keep navigation and interaction dependable

Keep current-page indication, breadcrumbs, useful heading anchors, and clear next
steps. Preserve route prefixes and incoming fragments. Mobile navigation must open
and close predictably, keep focus understandable, and offer sufficient touch space.
Use real links, buttons, labels, lists, and semantic headings before ARIA patches.
Preserve the skip link and no-JavaScript reading/navigation path.

Test search with a known symbol, an empty query, and no results; the feedback should
explain what happened. Verify code copying including its failure fallback. Check
theme persistence and the initial load to avoid unreadable flashes. Disabled,
loading, empty, failure, and success states should have distinct, useful wording.

Embedded examples must use the existing playground integration. Preserve View/Code
drafts, keyboard controls, loading, retry, errors, and host ownership. Presentation
placeholders are test fixtures; do not ship them as a substitute for a working demo.

## Use precise, maintainable diagrams

Use a diagram for a relationship that prose alone makes hard to follow: ownership,
lowering, lifecycle, or execution order. Use a table for a simple file/type mapping.
Each figure should answer one question and have a scope-specific caption and text
alternative. Keep labels short and readable at the article's actual width; provide
access to the full SVG when detail requires it.

Edit `docs/diagrams/*.d2` and shared `_style.d2` as appropriate, then regenerate with
`scripts/architecture_diagrams.py`. Keep source and SVG together. Inspect connector
direction, clipping, labels, and contrast in both themes; parsing successfully does
not prove a diagram is readable. Use exact source-driven diagrams, not AI-generated
bitmap approximations of technical structure.

## Inspect accessibility and responsive behavior

Inspect desktop and mobile screenshots of the actual generated page, including a
long heading, long prose, code, a table, and a diagram where applicable. Check the
existing 1440, 768, 390, and 320 CSS-pixel layout widths and both themes for changes
to shared layout. A page-specific edit can use a smaller representative set plus
the narrow-width check. Follow links and complete the task at the narrow size.

Test keyboard order, visible focus, focused headings below sticky headers, menu
and search dismissal, labels, and touch targets. Check text contrast in real normal,
hover, focus, and selected states, including syntax colors and muted captions.
WCAG's minimum text contrast is 4.5:1 for ordinary text and 3:1 for qualifying large
text; see [the W3C explanation](https://www.w3.org/WAI/WCAG22/Understanding/contrast-minimum.html).

Check reflow at 320 CSS pixels, or the equivalent 1280-pixel viewport at 400% zoom.
Prose should not require horizontal scrolling. Intrinsically two-dimensional
material such as a data table can retain its own scrollable area; see
[W3C reflow guidance](https://www.w3.org/WAI/WCAG22/Understanding/reflow.html).
Respect reduced-motion preferences in navigation and presentation effects; retain
usable playback controls for authored animations. Screenshots and a few passing
checks do not establish full accessibility conformance. Report exactly what was
inspected, and use [verification](verification.md) for the repository's test paths.
