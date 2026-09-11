# Third-party notices for Noon local agent tooling

This checkout-bound package does not copy a third-party scene corpus into Noon. The
preview setup installs or fetches the pinned runtime components below, and the MCP
package installs its JavaScript dependencies from the committed npm lockfile with
lifecycle hooks disabled.

## Playwright

- Component: Playwright JavaScript package and official browser container runtime
- Version: 1.62.1
- Container base: pinned in `preview.Dockerfile` by OCI SHA-256 digest
- Source: https://github.com/microsoft/playwright/tree/v1.62.1
- License: Apache License 2.0
- License text: https://github.com/microsoft/playwright/blob/v1.62.1/LICENSE

Noon derives the upstream pinned Playwright seccomp profile for its nested Chromium
sandbox contract. The setup helper verifies the exact upstream Git blob before
writing the derived local profile; the derived profile remains subject to the
applicable upstream license and is not a replacement Playwright distribution.

## Pyodide

- Component: `pyodide-core` runtime archive used by isolated Python previews
- Version: 314.0.5
- Archive identity: SHA-256 `f528dccea95fa8ec54295fd65bf86dd61183d11f0e52563dc8eadda45e0f78d6`
- Source: https://github.com/pyodide/pyodide/tree/314.0.5
- License: Mozilla Public License 2.0
- License text: https://github.com/pyodide/pyodide/blob/314.0.5/LICENSE

The preview image verifies the archive SHA-256 before extraction and serves the
pinned runtime locally while the execution container remains `--network=none`.

## Model Context Protocol TypeScript SDK and Zod

Direct JavaScript dependencies are pinned by `package.json` and
`package-lock.json`. The distribution manifest records their exact installed
versions, npm integrity values, and lockfile-declared licenses. At this revision:

- `@modelcontextprotocol/server` 2.0.0 — MIT
- `@modelcontextprotocol/client` 2.0.0 — MIT (development/test client)
- `zod` 4.2.0 — MIT

Transitive package identities and licenses remain recorded in the committed npm
lockfile and installed package metadata. Noon does not vendor their source into
this directory.

## External evaluation material

The initial #1199 distribution/setup slice adds no external scene or asset corpus.
Any future evaluation corpus copied from an external project must be reviewed for
license/provenance independently before it is committed; a URL or compatibility
reference alone is not permission to redistribute assets.
