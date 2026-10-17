# PR #2175 validation evidence

This extension records correctness checks for the packed-renderer integration fix. It belongs alongside the separate performance evidence; these gallery frame counts are not performance measurements.

## Headline: completed merged-tree fast gate

The final `scripts/check.sh fast` invocation passed architecture checks, formatting, the workspace check with all targets/features, strict Clippy, 25 Python reference tests, and **3,410 Rust library tests** across 11 suites. There were zero Rust failures and 33 ignored tests. It ran with `CARGO_INCREMENTAL=0`.

- Revision: `aba3d4c6a27a24890ffac70fd4819ae7aff47009`.
- Full source tree: `aa5f45c05e94ca62d181a910584768f7d6bdbf95`.
- Raw log: [logs/check-fast-merged-2175.log.gz](logs/check-fast-merged-2175.log.gz).
- Invocation/provenance details: [logs/check-fast-merged-metadata.json](logs/check-fast-merged-metadata.json).

The earlier completed log is retained because it covers the source tree used for the browser package: **3,400 Rust library tests**, 25 Python reference tests, zero failures, and the same strict fast-gate stages. Gzip decompression restores both logs byte for byte; their metadata records original sizes and SHA-256 values. Neither log embeds the candidate Git SHA or exit-code footer. Those fields are separately recorded run metadata, with commit/tree mappings independently verified through Git objects.

## Production gallery captures

The unchanged `scripts/playground-gallery-runtime-smoke.mjs` assertions qualified `noon-transform-matching-shapes-breadth` through the ordinary production gallery and worker in Chrome **151.0.7922.34**.

| Backend | Result | Presented frames | Final authored time | Final objects | Browser errors / failed requests |
|---|---|---:|---:|---:|---:|
| WebGL2 | Passed | 115 | 2 seconds | 4 | 0 / 0 |
| WebGPU | Passed | 125 | 2 seconds | 4 | 0 / 0 |

Both captures include live samples in the previously failing transient interval before one second. They completed with a valid semantic execution result, retained mixed rendering, zero buffered deltas, and no pending presentation. JSON results and final screenshots are under `browser/webgl/` and `browser/webgpu/`. The redundant one-element `results.json` files were omitted; the original per-case JSONs are included unchanged.

These captures qualify revision `495580e2197022701a9190bae4f20047193f5be6`, tree `432c64b62949bad018d587ade585b92986f80324`. Local revision `2d4c255728455013d2fd26aa28ab2eeb0ec505af` has that exact full tree. The final merged-tree gate above is a separate capture. Both revisions have identical `crates/noon-render-wgpu` subtrees, as recorded in [source-identities.txt](source-identities.txt).

The release package was built from the local equivalent tree; its runtime identity was aligned to `495580e` before the browser runs. Each run verified the WASM and identity hashes before and after execution:

- Runtime build: `b0170cc832f3a35996637589a3004cfaa32046028f995392ad5c78775fd890c5`.
- WASM: **50,121,763 bytes**.
- WASM SHA-256: `ab3485e20f1c2c54dcbab9da26da3b66b736355c3134c34014584a84129c8822`.

The exact identity document is [runtime-build-identity.json](runtime-build-identity.json). The 50 MB binary is not bundled here. The dedicated smoke-export startup/explicit-cleanup suite was **left for CI**: this plain production package omits its fixture factories. These gallery captures must not be reported as that separate smoke suite passing.

## Reproduction and local harness settings

`wrapper.mjs` is the captured JavaScript executed through Node stdin. `reproduce.sh` supplies that same stdin execution mode so relative imports resolve against the source checkout. It requires Node 24 (captured: 24.19.0), Playwright (captured: 1.62.1), the matching generated package in a `495580e` checkout, and Chrome 151 at `/tmp/noon-chrome151/chrome-linux64/chrome`.

```bash
NOON_REPO=/path/to/noon bash /path/to/this/evidence/reproduce.sh
```

The wrapper asserts the recorded revision/build/WASM identity. It writes fresh results to `/tmp/noon-packed-browser-gallery-direct`. Its only harness adaptations select Chrome/backend flags, apply the local test certificate settings used by the working benchmark, and let pinned Pyodide assets use Chromium networking instead of the optional Node `route.fetch` byte cache. This avoids the environment's Node-side DNS failure while preserving production source and gallery assertions. Browser errors and failed requests remain strict failures.

## Evidence classification and inventory

Browser JSON/PNGs, the identity document, Git-object output, the captured wrapper, and decompressed command logs are raw evidence. `summary.json`, the log metadata, this README, and [native-session-results.md](native-session-results.md) are explanatory or derived records. No separate focused native stdout files were retained; their session-reported outcomes are labeled accordingly and are not presented as raw logs.

[MANIFEST.json](MANIFEST.json) records sizes and SHA-256 values. [content-review.json](content-review.json) records the credential/signed-URL scan and screenshot review. No production source edits or remote writes were made to assemble this extension.
