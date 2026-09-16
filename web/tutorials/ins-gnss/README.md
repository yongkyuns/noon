# INS + GNSS: a Noon navigation tutorial

An original, chaptered Noon demo based on Dr. Fedor Baklanov's *INS/GNSS loose
coupling filter* documentation. The 14 scenes introduce sensor errors, frames,
mechanization, Kalman correction, covariance, error states, vehicle constraints,
calibration, and failure detection. Each chapter includes a question, staged
explanation, a takeaway, and accompanying reading notes.

**Numerical scope:** the live computed plots come from a reproducible **1D
position/velocity/physical-accelerometer-bias filter**. The full **24-error-state
ECEF filter is explained, not numerically reproduced or validated**. The
animations and notes keep that distinction visible. This is an approximately
seven-minute chapter sequence with pauseable reading, not the proposed full-length
45–60-minute course or a recorded-drive benchmark.

## Run inside the Noon checkout

```sh
bash scripts/build-web-demo.sh
python3 -m http.server --bind 127.0.0.1 --directory web 8080
```

Open `http://127.0.0.1:8080/tutorials/ins-gnss/`. To rebuild only the tutorial after
editing its Python modules:

```sh
python3 web/tutorials/ins-gnss/tools/build.py
```

The browser uses Noon's existing Python and rendering workers. Do not install a
different package called `noon`, invoke Manim, or substitute a plotting renderer.
The browser must be able to load Noon's configured Pyodide resources. Scene code
needs only the Python standard library and Noon: **no NumPy, SciPy, Matplotlib,
Pandas, or runtime package installer**.

Play/Pause controls forward sampling of the authored timeline. Restart creates a
fresh chapter session. There is deliberately no arbitrary backward seek button:
these source-driven scenes require Python continuation and have not been qualified
for arbitrary engine rewind. Pausing stops host sample requests; it does not call
the unsupported source-owned runtime pause operation. Desktop or landscape viewing
is recommended for equations; the page also avoids horizontal overflow on phones.

## Code map

| Path | Responsibility |
| --- | --- |
| `src/model.py` | Immutable experiment settings, deterministic sensor generation, 3-state KF, trace samples, metrics. No rendering imports. |
| `src/visuals.py` | Colors, typography, layout, explicit plot coordinates, ordinary geometry, and short reveal/caption helpers. No filter math. |
| `src/lessons/` | Small lesson functions in teaching order. Explicit `async`/`await` is intentional: reusable helpers cross Noon continuation barriers. |
| `src/entry.py` | One Scene entry point and chapter selection. |
| `chapters.json` | Chapter titles, expected durations, and reading notes. |
| `player.js` | Existing Noon worker lifecycle and forward playback controls. No scene graph, renderer, or numerical simulation in JavaScript. |
| `tools/build.py` | Concatenates known source modules for Noon's single-source entry point; removes only imports of these local modules. |
| `tools/export.py` | Reproduces the teaching trace and provenance without Noon. |
| `tests/` | Numerical contracts, actual Pyodide execution, browser chapter checks, and player/motion checks. |

Edit `src/`, not generated `scene.py`. Change physical assumptions in `Experiment`,
visual conventions in `Layout`/the palette, and prose in individual lessons and
`chapters.json`. Coordinates in a particular diagram describe its composition;
shared frame boundaries, label fitting, colors, and pacing live in the visual
helper module. Do not hide unsupported operations inside these helpers.

## Numerical assumptions

Default experiment: 120 seconds, 100 Hz IMU, 1 Hz GNSS, 0.02 m/s² constant physical
bias, 0.04 m/s² independent acceleration sample noise, and 3 m GNSS standard
deviation. GNSS is unavailable over `[45,75)` seconds. Separate seeded noise streams
make paired experiments comparable even when measurements are withheld.

The filter state is `[p,v,b]` and compensates acceleration as `raw - b`. It uses the
exact held-input transition for that model, sample-noise process covariance, a
scalar position observation, and a Joseph covariance update. This is not the same
signed bias convention as the reference's additive corrective offsets. A scalar
3-sigma innovation gate has NIS threshold 9; that is not a universal threshold for
multidimensional measurements.

Truth generates measurements and evaluates errors; it is not fed into the filter.
Initial position and velocity means are explicitly zero with nonzero covariance.
The simulation uses integer sensor ticks, not animation frames or wall-clock time.
Plots decimate smooth propagation but retain both sides of each position correction.
The opening graph is explicitly a complete recorded trace plus a time cursor, not a
live trace reveal. Joint 95% ellipse graphics are labeled illustrative; bias and
position traces are computed results.

## Tests and data export

From the repository root:

```sh
python3 web/tutorials/ins-gnss/tests/test_model.py
python3 web/tutorials/ins-gnss/tools/export.py /tmp/ins-gnss-trace
```

The export contains all 12,001 samples, pre-update positions, innovations, decisions,
full 3×3 covariances, configuration, metrics, and SHA-256 provenance. Default results
include 53.3366 m IMU-only versus 4.8927 m fused position error at 74.99 s, and a final
physical-bias estimate of 0.0209436 m/s². Those are one declared teaching run, not a
navigation-product accuracy claim.

With the repository browser-test dependencies installed and the browser package built:

```sh
NOON_TUTORIAL_BACKEND=webgpu node web/tutorials/ins-gnss/tests/review.mjs
NOON_TUTORIAL_BACKEND=webgl node web/tutorials/ins-gnss/tests/review.mjs
NOON_TUTORIAL_BACKEND=webgpu node web/tutorials/ins-gnss/tests/player.mjs
NOON_TUTORIAL_BACKEND=webgl node web/tutorials/ins-gnss/tests/player.mjs
```

`review.mjs` uses Noon's existing semantic preview host, checks actual published
chapter times, captures intermediate frames, and fails on browser/Python errors.
`player.mjs` tests actual controls, frozen pause output, identical fresh replay,
source completion, mobile overflow, and 91 dense cursor-motion samples. Captured
sample-and-screenshot timings are **not** standalone rendering frame-rate benchmarks.
Review the PNGs as well as the assertions: valid geometry does not prove readability.

To execute the model in the repository-pinned Pyodide interpreter using Node:

```sh
node web/tutorials/ins-gnss/tests/pyodide_model.mjs \
  /absolute/path/to/pyodide.mjs \
  web/tutorials/ins-gnss/src/model.py
```

Initial qualification used Noon engine revision
`088c746a2b658ce3ce2810c41924a7768edceba2`, Pyodide 314.0.5/Python 3.14.2,
and Chromium 151.0.7922.34. Runtime/source hashes and actual outcomes are recorded
in generated test reports; this README is not a blanket browser-parity guarantee.

## Reference and convention ledger

Upstream revision: `0d33f229a0abbefeee084cc4306bdc3a47d5862f` in
[fedorbaklanov/open-aided-navigation](https://github.com/fedorbaklanov/open-aided-navigation/tree/0d33f229a0abbefeee084cc4306bdc3a47d5862f).
The source is credited, not copied as a screenshot presentation.

- `demo/insGnssLoose/Documentation_InsGnssFilterLoose.pdf`: 17-page technical reference.
- `StateMapInsGnssLoose.m` / `ErrorStateMapInsGnssLoose.m`: 26 nominal coefficients versus 24 error coordinates.
- `insGnssLooseOde.m`: ECEF mechanization, Earth-rate terms, gravity helper.
- `insGnssLooseTransMat.m` / `insGnssLooseSysNoiseMat.m`: first-order transition/noise discretization.
- `InsGnssFilterLoose.m`: initialization, position observations, whitening, gating, NHC, correction.

The diagrams use destination superscript/source subscript for rotations. Quaternion
injection is presented with an explicitly left/navigation-frame convention. General
error-state reset requirements must not be mistaken for a claim that the upstream
implementation uses an exact exponential-map/reset implementation. Receiver position
is an observation, not independent ground truth. No NHC, scale-factor, mounting-state,
real-drive, or full ECEF performance result is fabricated by the 1D model.
