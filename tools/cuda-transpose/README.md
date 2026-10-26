# One Extra Column — CUDA transpose reproduced with Noon

This example is **reproduced with Noon for demonstration purposes**, based on
**CUDA Algorithm Lab by Arash (ourarash)**. The original work is MIT licensed.

- [Original author](https://github.com/ourarash)
- [Original repository](https://github.com/ourarash/cuda-algorithm-lab)
- [Original interactive bank-conflict visualization](https://github.com/ourarash/cuda-algorithm-lab/blob/main/matrix_transpose/03_padded/03_transpose_bank_conflicts_visualization.html)
- [Original padded CUDA kernel](https://github.com/ourarash/cuda-algorithm-lab/blob/main/matrix_transpose/03_padded/03_transpose_padded.cu)
- [Original unpadded shared-memory kernel](https://github.com/ourarash/cuda-algorithm-lab/blob/main/matrix_transpose/02_shared_memory/02_transpose_shared_memory.cu)

The animation adapts the explanation into a non-interactive, Manim-style Noon
scene. Explicit async construction awaits the normal play/wait continuations on
browsers with or without JavaScript Promise Integration. The full third-party copyright and MIT permission notice is retained in
[`cuda_transpose_banks.LICENSE`](../../web/python/examples/cuda_transpose_banks.LICENSE),
separate from the scene's short attribution header. Keep that notice with
redistributed copies of the scene or instrumented CUDA kernel.

## Manual playback

Paste the complete
[`cuda_transpose_banks.py`](../../web/python/examples/cuda_transpose_banks.py)
into the Noon Playground and run `CudaTransposeBanks`.

```python
SPEED = 1.0
COLUMN = 3                # 0..31
SYNTAX_HIGHLIGHT = True
```

Shared `Code` owns source lexing, indentation, smaller zero-padded gutters,
allocated row highlights and the reserved padded-declaration edit. The lesson
selects category and whole-token colors to connect CUDA source to its request
tray, bank cards, lane registers and source-row-group colors.
Source focus is established before each corresponding movement. The global
store follows completion of all read batches.

Numbered packets are pairs of independent rectangle/text objects. They animate
in parallel without submitting multiple family Transforms in one activation.
The active code-line background is a muted blue-gray with 20% fill and no outline;
syntax colors and data colors are independent of that emphasis.

The scene uses Noon's
`color_from_hex` helper and is not advertised as an import-only Manim port.

## One educational model; separate correctness tools

The demo always uses the same deterministic model of the CUDA indexing logic.
It is an educational visualization, not a trace viewer or a live CUDA program.
There is no capture input and no CPU/GPU playback mode. Row-load staging and
bank-service timing are illustrative, not GPU cycles or kernel benchmarks.

Optional NVIDIA hardware validation lives entirely in the companion tools:

```bash
python3 tools/cuda-transpose/capture_cuda.py --output /tmp/noon-cuda-capture.json
python3 tools/cuda-transpose/capture_cuda.py --verify /tmp/noon-cuda-capture.json
```

Run these commands from the repository root. Verification checks full CUDA
accesses, original-kernel outputs, source fingerprints, and sanitizer evidence,
then compares all 32 columns and both layouts against the actual model helpers
extracted from the demo source. It does not import or execute the Noon scene.
Captured physical shared addresses are normalized to the demo's tile-relative
base of zero; bank numbering may rotate, but values and contention must agree.

The browser fidelity suite still runs the ordinary demo unchanged. A capture is
an additional independent reference, never replacement data for the animation.
See [VALIDATION.md](VALIDATION.md) for strict hardware-backed qualification and
its limits. No physical CUDA pass is implied by local model or browser tests.

## Focused validation

From the repository root:

```bash
python3 -m py_compile web/python/examples/cuda_transpose_banks.py
python3 tests/cuda_transpose_banks_test.py
node --test web/cuda-transpose-banks.test.mjs
```

The checks cover all columns and both strides at five base offsets, broadcasts,
producer/consumer correspondence, deterministic model isolation, shared Code
options and source-edit authoring, source sequencing, authored data geometry,
independent packet animation arguments, subtle highlighting and attribution.
Shared Code tests own lexical matching and contextual syntax behavior. Helper tests use call spies, not a simulated Noon runtime.
These tests do not qualify rendering, replay/seek, frame rate, or CUDA execution.

The real browser qualification keeps all three original positive configurations
and six required negative controls. The existing CI and nightly/manual oracle
retain the same fifteen-minute budget. A successful browser run is functional
renderer evidence, not physical iPhone performance or CUDA hardware execution.
The previous async playback implementation was qualified in Chromium and
JSPI-disabled desktop WebKit; changes require fresh qualification rather than
inheriting that result. The gallery poster is source-bound to a retained actual
fidelity-run frame and must be refreshed whenever this scene changes. Tracking:
#2302 for shared preset/demo cleanup; original fidelity work #2125 / #2126.
