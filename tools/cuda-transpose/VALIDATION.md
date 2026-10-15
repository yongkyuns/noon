# CUDA transpose visual-fidelity validation

The lesson is reproduced with Noon for demonstration purposes from
[CUDA Algorithm Lab by Arash](https://github.com/ourarash/cuda-algorithm-lab).
The original visualization and kernel links remain in the scene header. The
original MIT notice is in `web/python/examples/cuda_transpose_banks.LICENSE`.

## What a passing browser run establishes

The actual `cuda_transpose_banks.py` scene runs through the same Python authoring,
shared Rust execution, and retained renderer as the Playground. A test-only,
read-only observer queries real object identities, effective centers, sizes,
colors, text, and fill alpha before and after actual `Scene.play` calls. It never
implements a replacement renderer, scheduler, interpolation loop, or scene.
Ordinary manual playback does not install this observer.

An independent closed-form oracle checks the 32-bit address, lane, value, and
output-index correspondence. Fixed screen-layout contracts independently check
that a request reaches the correctly labeled bank, returns to its lane register,
and ends in the transposed output row. Both load-assignment lines must be active
during loads; shared-read and global-store highlights are checked separately.
Ready counters follow completed loads, all eight warp indicators reach the block
barrier, and the store cannot precede the selected read rounds.

The browser advances at exact authored times with `sampleToAuthoredTime`, which
waits for the matching renderer publication. Midpoint and near-endpoint PNGs are
checked for packet coverage, color, label coverage, vacated-source ghosts, and
active/inactive source-row contrast. Active CUDA text and bank labels must have
rendered glyph coverage. Legitimate overlapping paths may obscure packets at a
midpoint; endpoint checks require all 32 packets to be unoccluded. No OCR is used.
Semantic checks verify exact label content; pixel checks verify that glyphs are
actually present, not that rasterized digits have been independently recognized.

Positive runs cover columns **3, 0, and 31**, at speeds **1, 2, and 0.5**,
respectively, for both layouts. Six negative variants alter the actual scene:
wrong bank, wrong read highlight, detached packet label, invisible packet,
early global store, and nearly invisible highlight. Each must fail with its
specific assertion category. An unrelated Python/renderer crash is not an
accepted negative-control result.

## Run the CPU-backed browser qualification

Build the repository's normal web package and install the browser dependencies
used by CI. Then run from the repository root:

```bash
python3 tests/cuda_visual_validation_test.py
node --test scripts/cuda-transpose-pixels.test.mjs
node scripts/cuda-transpose-qualification.mjs
```

The existing `browser-authoring` CI job runs these commands after downloading
and verifying its normal `web-pkg` artifact. There is no second WASM build and no
GPU dependency for this CPU-backed job. Failures are fatal. Reports and selected
PNG/debug-frame evidence are uploaded even after failure.

The default report explicitly says `oracle: independent-cpu-closed-form` and
`cudaValidated: false`. Passing it is **not** evidence that CUDA executed.

## Capture independent CUDA evidence

On a machine with an NVIDIA GPU, NVCC, and Compute Sanitizer:

```bash
python3 tools/cuda-transpose/capture_cuda.py --output /tmp/noon-cuda-capture.json
python3 tools/cuda-transpose/capture_cuda.py --verify /tmp/noon-cuda-capture.json
```

This compiles the instrumented recorder and unchanged excerpts of both original,
uninstrumented kernels. Each original output is compared against the recorded
output and an independent CPU transpose. The capture retains all 1,024 loads,
1,024 shared reads/global destinations, and output words for both layouts, for
all 32 selected columns. It also requires successful `memcheck`, `racecheck`, and
`synccheck` runs with clean summaries and nonzero error exit codes on detection.

The capture records source SHA-256 fingerprints, executable identity, compiler
flags/version, GPU/driver/runtime metadata, input configuration, sanitizer logs
and checksums. Missing or stale source identities, missing records, incorrect
outputs, missing sanitizer evidence, or failed tools are rejected. It writes no
capture if NVCC or Compute Sanitizer is unavailable. A file hash provides integrity
and reproducibility, **not signed hardware attestation**.

The original function excerpts are pinned in `original_kernels.cuh`; their
upstream blob identities and author attribution are recorded there. Changes to
either that header or `record_cuda.cu` invalidate old captures.

## Require GPU-backed browser qualification

```bash
NOON_CUDA_CAPTURE=/tmp/noon-cuda-capture.json \
NOON_CUDA_REQUIRE_GPU=1 \
node scripts/cuda-transpose-qualification.mjs
```

The driver verifies the capture before replay, supplies its actual addresses and
values to the scene, and includes the capture fingerprint/provenance in its
report. Missing GPU evidence is an error, not a CPU fallback. `cudaValidated`
records acceptance of the capture; the report's separate `status` must also be
`passed` to claim that the visual qualification passed.

The original pasteable recorder interface is retained:

```bash
nvcc -std=c++17 -O2 tools/cuda-transpose/record_cuda.cu -o record_cuda
./record_cuda 3 > gpu_trace.py
```

That small assignment alone is useful for manual replay, but is not the complete
source-bound, sanitizer-qualified evidence bundle required by the strict runner.

## Evidence and scope

Default output: `cuda-transpose-qualification/report.json`, per-case observed
runtime queries, selected PNGs and debug frames, and a failure screenshot.
`NOON_CUDA_ARTIFACTS` can change the destination. Source/probe fingerprints bind
the report to the tested files. Synthetic observations/images are confined to
unit tests of the predicates; they are never published as renderer/GPU evidence.

The fixed fixture is one full interior 32-by-32 tile, a 32-by-8 thread block,
32-bit floats with `input[r,c] = 32*r+c`, and strides 32/33. It does not qualify
edge tiles, arbitrary element types, wider memory instructions, multiple blocks,
or the entire upstream repository. Source lines are a semantic explanation,
not a disassembly or a cycle-accurate simulation. Instrumentation can change
execution. Service order, travel duration, and relative animation speed do not
constitute measured hardware bank-service order or kernel speedup. Nsight
performance qualification is not implemented by this suite.

Current source-owned continuations support monotonic authored-time sampling,
not seek/restart controls. These tests use fresh executions for the speed/column
cases and explicitly do **not** claim rewind/seek equivalence.

Local predicate tests and syntax checks can pass without a Noon build. Full
browser qualification is established only by a successful run of the real
browser driver; CUDA-backed qualification additionally requires a genuine
capture. Missing runtimes/toolchains are never recorded as a pass.
