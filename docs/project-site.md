# Project website and README media

The public introduction is authored in `web/site/`. The existing playground stays
in `web/index.html` so engine tests and development URLs do not change.
`scripts/build-project-site.mjs` copies the built `web/` tree to `_site/`, keeps the
playground at `playground.html`, and installs the introduction at the site root.
All runtime modules, workers, manifests, and resources remain beside the
playground. The staging script never edits its inputs.

Production uses the manual-only `.github/workflows/pages.yml` deployment. A merge
or successful PR check does not publish the site. After separate deployment
approval, dispatch on `master` with its full approved commit as `expected_sha` and
`confirm_deploy=true` (the default is false). The request and actual checkout are
checked before building and again after media capture, staging and site smoke,
immediately before Pages configuration and upload. Required environment reviewers
remain owner-managed GitHub settings; naming `github-pages` does not enable them.

The uploaded artifact remains `_site/`, containing the introduction, generated
documentation/API references, live embeds and the full playground. Hidden files,
build-revision verification and deployment concurrency are retained. PRs use
`playground-project-site.yml` to build a review artifact, not publish Pages.
This manual demo rebuild is not exact-byte promotion of the separately qualified
release archive, redistribution approval or release readiness; those remain #2433 work.

## Links and documentation

The homepage uses directory-relative links, including under the `/noon/` project
prefix. Existing root links with query parameters redirect to `playground.html`
with their entire query and fragment preserved. Hash-only homepage links stay on
the homepage. Explicit playground links work without the redirect script.

The documentation builder renders canonical Markdown and source-derived Python
API entries, and stages compiler-generated Rustdoc. It preserves source revision
provenance and fails on missing local destinations. `REFERENCE.md` retains the
previous detailed README at the repository root. It is implementation detail,
not a replacement for `docs/architecture.md`.

## Live examples

The homepage and documentation mount the selected scenes on load when the
primary pointer is a mouse or trackpad. For touch input, the cards start with
gallery thumbnails. **Play scene** mounts one live embed and retires the previous
one, releasing its workers and discarding its draft.
Each scene has a compact **View / Code** switch. The code and canvas panels stay
mounted; switching views does not reset the draft or execute the source again.

`embed.html` is generated from the canonical playground shell. It retains
`main.js` and `python-editor-bootstrap.js`, including the gallery's initial live
run and CodeMirror editor. `site/embed-host.js` enables the existing auto-run
checkbox before initialization. Typing pauses, composition handling, cancellation,
source replacement, diagnostics and recovery still belong to the gallery's
ordinary run path; the embed does not implement another editor, debounce loop,
source buffer, clock, renderer or worker protocol.

Normal presentation contains the scene and the two view-switch tabs. Loading or
failure adds a small message and, when needed, Retry. Code displays the original
editor diagnostics on failure. Focusing the View panel exposes keyboard playback
shortcuts through the existing playback controls, without displaying their full
toolbar. The controls follow the website theme; source editing retains the
canonical gallery syntax theme. Authored scene colors are not restyled.

Every live scene has its own context. Desktop scenes can run concurrently.
Navigation retires the contexts and
the canonical gallery workers. Back/forward cache retains the editor DOM and
resumes through the ordinary Run action. Merely changing browser tabs does not
discard drafts. Audio availability remains subject to the existing host/browser
policy; the site does not bypass user-gesture requirements for sound.

`site/preview.js` owns embed lifetime, same-origin shell messages and load-error
retry. The initial runtime run is owned by `live-authoring-bootstrap.js`, not the
embed adapter. Transport failure is not silently replaced by a prerecorded clip.
Without JavaScript the site still presents documentation and full-editor links.

## README media

`web/site/examples.json` is an editorial allowlist of selected, original showcase
scenes and capture intervals. Inclusion in the wider gallery alone does not
approve a scene for project presentation. The canonical paths and durations remain
in `web/python/examples/noon_showcase_manifest.json`.

`scripts/capture-project-media.mjs` executes those unchanged Python sources through
the existing raster host and WebGL2 renderer. It captures 30 frames per authored
second. FFmpeg produces 480 x 270, 10 fps GIFs for the README and 960 x 540, 30 fps
MP4 review clips. These assets remain in the build, but the website's embedded
examples execute the live runtime instead of playing the clips.

The script checks actual presentation, backend, dimensions, changing frames,
published time and the 6 MB per-asset limit. It publishes the bundle only after
every clip passes. `site/media/provenance.json` records the checkout, runtime,
source and media hashes, frame times, encoder and browser. No generated
illustration or Manim render is substituted. Captures are not performance tests.

## Validation and regeneration

After building the existing browser package, install the repository-pinned
browser tools and FFmpeg, then run:

```sh
npm install --no-save --ignore-scripts playwright@1.62.1 pngjs@7.0.0
npx playwright install --with-deps chromium firefox webkit
# Install ffmpeg and xvfb through your operating system's package manager.
node --test scripts/project-site.test.mjs web/project-site-editorial.test.mjs web/project-site-controls.test.mjs
node scripts/capture-project-media.mjs
node scripts/build-project-site.mjs
node scripts/project-site-smoke.mjs
xvfb-run -a node scripts/project-site-embed-smoke.mjs
python3 -m http.server --directory _site 8080
```

The structural suite covers curated sources, desktop autoplay, touch activation, trusted shell
messages, independent contexts, theme changes, legacy routing and staging.
The layout suite checks 1440/768/390/320 px, both themes, local links/fragments,
no-JavaScript reading, tab accessibility, useful editing space and retry. It uses
explicit runtime placeholders and must not be reported as rendering evidence.

The separate three-engine runtime gate loads the actual homepage and Examples
page with all four live contexts at once. It requires automatic initial execution
without Run/Play gestures, actual pixel changes, keyboard playback, View/Code
preservation, hot edits through the visible CodeMirror editor, error recovery,
and observed worker retirement on navigation. It records rendered frames and
failure evidence. Review the output before merge; passing source/layout checks
alone does not qualify the composed runtime or performance on physical devices.

Run `node --test web/pages-authorization.test.mjs web/pages-project-site.test.mjs`
for the deployment request and combined website workflow contracts. These tests
make no network deployment and do not replace actual browser qualification.
