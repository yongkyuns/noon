# One Extra Column — CUDA transpose reproduced with Noon

This example is **reproduced with Noon for demonstration purposes**, based on
**CUDA Algorithm Lab by Arash (ourarash)**. The original work is MIT licensed.

- [Original author](https://github.com/ourarash)
- [Original repository](https://github.com/ourarash/cuda-algorithm-lab)
- [Original interactive bank-conflict visualization](https://github.com/ourarash/cuda-algorithm-lab/blob/main/matrix_transpose/03_padded/03_transpose_bank_conflicts_visualization.html)
- [Original padded CUDA kernel](https://github.com/ourarash/cuda-algorithm-lab/blob/main/matrix_transpose/03_padded/03_transpose_padded.cu)
- [Original unpadded shared-memory kernel](https://github.com/ourarash/cuda-algorithm-lab/blob/main/matrix_transpose/02_shared_memory/02_transpose_shared_memory.cu)

The animation adapts the explanation into a non-interactive, Manim-style Noon
scene. The full third-party copyright and MIT permission notice is retained in
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
GPU_TRACE = None          # Explicitly labeled CPU reference replay
SYNTAX_HIGHLIGHT = True
```

This is the user-approved scene with syntax-colored CUDA statements, a request
tray, bank cards, lane registers, and consistent source-row-group colors.
Source focus is established before each corresponding movement. The global
store follows completion of all read batches.

Numbered packets are pairs of independent rectangle/text objects. They animate
in parallel without submitting multiple family Transforms in one activation.
The active code-line background is a muted blue-gray with 20% fill and no outline;
syntax colors and data colors are independent of that emphasis.

[`syntax_highlighting_smoke.py`](syntax_highlighting_smoke.py) isolates the
`Text(t2c=...)` capability for manual testing. The scene uses Noon's
`color_from_hex` helper and is not advertised as an import-only Manim port.

## Optional CUDA-recorded addresses and values

On an NVIDIA CUDA machine, from this directory:

```bash
nvcc -std=c++17 -O2 record_cuda.cu -o record_cuda
./record_cuda 3 > gpu_trace.py
```

The recorder executes instrumented unpadded and padded kernels. It validates
both complete 1,024-word outputs and the recorded accesses before printing a
`GPU_TRACE = {...}` assignment. Replace `GPU_TRACE = None` in the Noon source
with that assignment; keep `COLUMN` equal to the recorder argument.

The default replay evaluates the CUDA indexing expressions on the CPU. It is
not CUDA execution. Recorded mode uses captured selected-read addresses and
values, not a captured instruction timeline. Row-load staging and bank-service
order are illustrative. The two layouts use equal animation duration per
schematic service round; those durations are not GPU cycles, latency
measurements, or evidence of a 32x whole-kernel speedup. Instrumentation changes
execution and is not a benchmark.

## Focused validation

From the repository root:

```bash
python3 -m py_compile web/python/examples/cuda_transpose_banks.py tools/cuda-transpose/syntax_highlighting_smoke.py
python3 tests/cuda_transpose_banks_test.py
node --test web/cuda-transpose-banks.test.mjs
```

The 39 checks cover 320 trace configurations, broadcasts, producer/consumer
correspondence, invalid traces, syntax token ranges, source sequencing,
authored geometry, independent packet animation arguments, subtle highlighting,
and attribution. Helper tests use call spies, not a simulated Noon runtime.
These tests do not qualify rendering, replay/seek, frame rate, or CUDA execution.

The user approved the manually tested scene before this PR update. Automated
full Noon/browser qualification, CUDA recorder compilation/execution, and a
renderer-generated gallery thumbnail remain separate outstanding checks.
This update does not register or publish a gallery entry. Tracking: #2125;
PR: #2126.
