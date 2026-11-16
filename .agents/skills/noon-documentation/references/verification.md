# Verify Noon documentation changes

Run commands from the repository root. Re-read `docs/project-site.md` and
`.github/workflows/playground-project-site.yml` for current prerequisites. These
recipes identify existing gates; they do not require rebuilding every runtime for
a spelling correction. Required repository checks still apply.

## Choose checks by the changed surface

| Change | Evidence to seek |
| --- | --- |
| Wording or existing prose | Source accuracy, diff, affected links; rendered inspection if structure changes |
| New page, heading, route, source inclusion | Documentation build, relevant generator tests, links/fragments, navigation |
| Template, CSS, theme, navigation, copy/search | Source checks plus generated site and desktop/mobile interaction inspection |
| Example or advertised behavior | Its owning runnable example/test and honest platform scope |
| Live embed/runtime integration | Real runtime qualification in addition to layout checks |
| Architecture diagram | Source/SVG reproducibility plus visual inspection |

## Fast source and generator checks

Existing site source checks:

```sh
node --test scripts/project-site.test.mjs web/project-site-editorial.test.mjs web/project-site-controls.test.mjs web/project-site-design.test.mjs
```

With the documentation environment already set up:

```sh
target/project-site-tools/bin/python -m unittest discover -s scripts/site -p 'test_*.py' -v
```

If absent, create `target/project-site-tools` with `python3 -m venv` and install
`scripts/site/requirements.txt` there, as the existing build script does. Use the
owning example or test for changed snippets; a string assertion in a site test does
not establish that the example runs. `bash scripts/check.sh fast` is the repository
entry check; consult `AGENTS.md` for broader gates when code also changes.

## Build the real documentation bundle

`node scripts/build-project-site.mjs` stages the built playground and authored site
into `_site/`, then invokes `scripts/build-project-docs.sh` to build Python references,
Rustdoc, documentation pages, and generator tests. It expects the real web build,
showcase media, and provenance files described in `docs/project-site.md`; follow the
current workflow to prepare those assets. Do not fabricate them to make a gate pass.

For an existing valid staged `_site/` whose documentation alone needs rebuilding:

```sh
bash scripts/build-project-docs.sh "$PWD" "$PWD/_site"
```

This builds compiler-produced Rustdoc and checks references against current source.
It requires the repository toolchain, Rust dependencies, and Python dependencies.
The staging directory and `target/` artifacts are generated output; edit their
source owners instead of committing hand-modified generated HTML or signatures.

## Inspect the page and exercise navigation

After preparing a valid staged site and the browser dependencies documented by the
project, run the existing presentation and control checks:

```sh
node scripts/project-site-smoke.mjs
node scripts/project-site-design-smoke.mjs
python3 -m http.server --directory _site 8080
```

Open the server's local URL with an available browser. Inspect the edited page and
at least one representative long/code-heavy page. View the actual screenshots and
follow the task at desktop/mobile widths in both themes; use the design reference
for focus, reflow, links, copy, search, and no-JavaScript checks. The scripts save
evidence under `browser-smoke-artifacts/project-site/`.

The presentation suites deliberately install runtime placeholders, including in
`scripts/project-site-docs-smoke.mjs`. They establish layout and interaction behavior,
not actual scene rendering. Do not cite their passing result as GPU, export, or
cross-browser runtime evidence.

When a change affects real embedded playback, editing, or runtime ownership, follow
the separate qualification in `docs/project-site.md`, including the current
Chromium/Firefox/WebKit dependencies and:

```sh
xvfb-run -a node scripts/project-site-embed-smoke.mjs
```

Run further native/runtime gates only when the touched behavior or required checks
call for them. If an environment blocks a gate, record the missing prerequisite and
the specific unverified behavior; do not weaken the test or claim equivalent proof.

## Check changed architecture diagrams

Use the D2 version pinned by `.github/workflows/architecture-diagrams.yml`:

```sh
python3 scripts/architecture_diagrams.py
python3 scripts/architecture_diagrams.py --check
```

Inspect regenerated SVGs at full size and within the page. Run the diagram tool's
tests when its implementation changes, not for every wording-only page edit.

## Report the result precisely

State the reader improvement, changed source files, commands actually executed,
pass/fail results, browser widths/themes inspected, and remaining limits. Cite the
actual evidence locations when useful. Separate a build, a visual inspection, a
layout test, and a real runtime qualification. Do not deploy as an implicit side
effect of documentation work; follow the existing publication authorization.
