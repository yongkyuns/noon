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

The uploaded artifact remains `_site/`, containing both the introduction and the
playground. Capture tools, all four website validation/generation commands,
hidden files, build-revision verification and deployment concurrency are retained.
PRs use `playground-project-site.yml` to build a review artifact, not publish Pages.
This manual demo rebuild is not exact-byte promotion of the separately qualified
release archive, redistribution approval or release readiness; those remain #2433 work.

## Links

The homepage uses directory-relative links, including under the `/noon/` project
prefix. Existing root links with query parameters redirect to `playground.html`
with their entire query and fragment preserved. Hash-only homepage links stay on
the homepage. Explicit playground links work without the redirect script.

The README uses absolute Pages GIF URLs, linked to editable gallery entries. They
become available after the first deployment of this site. Before that deployment,
use the PR's `project-site-preview` artifact to review them. `REFERENCE.md` retains
the previous detailed README at the repository root, preserving relative links.
It is implementation detail, not a replacement for `docs/architecture.md`.

## Media

`web/site/examples.json` is an editorial allowlist of selected, original showcase scenes and clip intervals. Inclusion in the wider demonstration gallery alone does not approve a scene for project promotion. The
canonical source paths and durations remain in
`web/python/examples/noon_showcase_manifest.json`.

`scripts/capture-project-media.mjs` executes those unchanged Python sources through
the existing Noon raster host and WebGL2 renderer. It advances scene time at 30 Hz
and captures 30 frames per authored second. FFmpeg makes 480 × 270, 10 fps GIFs
for the README and 960 × 540, 30 fps MP4 clips for the site. Each clip is limited to 6 MB. These
sampled previews are **not real-time performance measurements**.

The script checks actual presentation, backend, dimensions, changing frames, and
published time (including the gallery's declared still intervals). It publishes
the generated bundle only after every clip passes. `site/media/provenance.json`
records the checkout, runtime identity, source and media hashes, frame times,
encoder, and browser. No generated illustration or Manim render is substituted.
The checked-in gallery PNGs provide posters while the videos are not playing.

Media are generated in the build, not committed as growing binary history. Pages
recaptures them with its runtime; PR qualification does the same with the PR build.
The homepage does not load the engine, does not autoplay, needs no third-party
fonts, and remains readable without JavaScript. Video controls are native browser
controls; reduced-motion users do not receive automatic motion.

## Validation and regeneration

After building the existing browser package, install the repository-pinned
browser tools and FFmpeg, then run:

```sh
npm install --no-save --ignore-scripts playwright@1.62.1 pngjs@7.0.0
npx playwright install --with-deps chromium
# Install ffmpeg through your operating system's package manager.
node --test scripts/project-site.test.mjs
node scripts/capture-project-media.mjs
node scripts/build-project-site.mjs
node scripts/project-site-smoke.mjs
python3 -m http.server --directory _site 8080
```

The structural suite covers curated IDs, links, accessible static markup,
no-autoplay behavior, legacy routing, and staging without rewriting the original
playground. The browser smoke checks 1440, 768, 390, and 320 px layouts, expanded
setup instructions, local assets, GIF decoding, MP4 playback, legacy redirects,
and no-JavaScript content. Captures and screenshots remain CI review artifacts.
Review their visible contents before merging; structural checks alone do not
establish editorial quality or full gallery compatibility.

Run `node --test web/pages-authorization.test.mjs web/pages-project-site.test.mjs`
for the deployment request and combined website workflow contracts. These tests
reject omitted/reordered capture checks, restored push-scope guards, wrong upload
directories and an authorization recheck before site generation. They make no
network deployment and do not replace actual browser capture/smoke qualification.
