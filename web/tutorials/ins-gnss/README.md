# INS + GNSS: a Noon navigation tutorial

Fourteen chapters based on Fedor Baklanov's *INS/GNSS loose coupling filter*
documentation. The current sequence is **817.70 authored seconds** (about 13½
minutes), with pauseable reading notes. It is **not yet the planned full course**.

The numerical material comprises a reproducible **1D position/velocity/physical
accelerometer-bias KF** and a separate **single 2D linear position-fix example**.
The full **24-error-state ECEF filter is explained, not numerically reproduced or
validated**. Neither receiver GNSS nor a posterior estimate is presented as
independent ground truth.

## Run

From the Noon checkout:

```sh
bash scripts/build-web-demo.sh
python3 -m http.server --bind 127.0.0.1 --directory web 8080
```

Open `http://127.0.0.1:8080/tutorials/ins-gnss/`. Rebuild only the tutorial after
editing Python with `python3 web/tutorials/ins-gnss/tools/build.py`.

The page uses Noon's existing Python/rendering workers and configured Pyodide
resources. Do not install a separate package named `noon` or substitute Manim.
**Only Noon and the Python standard library are required**: no NumPy, SciPy,
Matplotlib, Pandas or runtime package installer.

Play/Pause controls forward sampling; Restart creates a fresh chapter session.
Arbitrary backward seek is not qualified and is not offered. Pausing stops host
sample requests rather than invoking the unsupported source-owned runtime pause
operation. Desktop or landscape viewing is recommended for the equations.

## Code organization

| Path | Responsibility |
| --- | --- |
| `src/model.py` | Deterministic sensor generation, 3-state KF, immutable update snapshots and metrics. No rendering imports. |
| `src/uncertainty.py` | Tested 2D Gaussian contour geometry. |
| `src/prediction.py` | Closed-form checks of the existing predictor and a signed variance budget. No second estimator. |
| `src/measurement.py` | One 2D position fix: correlated noise, whitening, batch/sequential comparison, innovation distance. |
| `src/visuals.py` | Shared layout/typography and helpers constructing ordinary Noon objects. |
| `src/lessons/` | Small named explanation functions in teaching order. |
| `src/entry.py` | One Scene entry point and chapter selection. |
| `chapters.json` | Titles, reading notes, expected durations and selected review times. |
| `player.js` | Existing worker lifecycle and forward playback. No numerical model, renderer or semantic state mirror. |
| `tools/` | Reproducible single-source browser bundle and numerical export. |
| `tests/` | Numerical, actual Pyodide, browser-frame and playback checks. |

Edit `src/`, not generated `scene.py`. Physical assumptions live in `Experiment`
and `PositionFixExample`; shared visual settings live in `Layout`. Diagram-specific
coordinates describe composition, not hidden numerical results. The bundler removes
only known local imports and rejects colliding top-level definitions. It introduces
no runtime loader or second animation system.

## Detailed worked lessons

**Chapter 7 — How a position fix learns bias (133.2 s).** Freeze simulation time at
75-second reacquisition. Inspect the same update's innovation, gain entries and
units, corrections, covariance contours and truth comparison. Removing only the
cross-covariances preserves the position correction but eliminates indirect velocity
and bias corrections. The actual position–bias contours use fixed prior-based
normalization, equal display scales and 95% joint Gaussian probability. Scalar
whiskers are separately labelled ±1 standard deviation. The noisy fix overshoots the
true bias; uncertainty is not confused with accuracy.

**Chapter 9 — What grows without GNSS? (140.1 s).** Follow error transport, the exact
teaching-model transition, the recorded outage and a signed position-variance
budget. The last accepted fix is at 44 s, not 45 s: 3,100 IMU steps precede the
75 s pre-update snapshot. Eleven tests compare closed-form accumulation with the
samplewise predictor. Removing only NEW process noise still leaves inherited
velocity/bias uncertainty to propagate. The graph explicitly uses true-minus-
estimated error, opposite chapter 1's overview convention. Marginal ±2σ bounds are
not joint contours or empirical coverage. Reacquisition is a discontinuity at one
timestamp, never physical travel smoothed over time.

**Chapter 10 — One complete GNSS update (198.45 s).** Nine small explanation functions
follow correlated measurement noise, Cholesky whitening, conditional scalar
residuals, Joseph covariance, batch equivalence, incorrect independence and invariant
innovation distance. `PositionFixExample` supplies prior mean `(0,0)` m, `P = 4I`
m², observation `(4,2)` m and `R = [[5,4],[4,5]]` m². The 2x2 routines intentionally
do not pretend to be a general linear algebra package.

The animation transforms the *same* contour vertices by W. Each panel has equal
horizontal/vertical scales; original coordinates are metres and whitened coordinates
are dimensionless. These are modelled 95% noise contours, not observed coverage.
The second scalar residual changes sign after row 1: about −0.894 becomes +0.166.
Whitening both r and H gives the same posterior as a batch solve; deleting R's
cross-terms changes the model. Whitening R does not whiten S: the correct NIS is
about 1.784615, whereas the squared noise-whitened residual norm is 4. All state
corrections remain in metres at one frozen event. No ground truth or full ECEF
performance claim is invented for this single-fix example.

## Numerical assumptions and export

Default drive: 120 seconds, 100 Hz IMU, 1 Hz GNSS, constant physical bias
0.02 m/s², independent acceleration-sample noise 0.04 m/s², receiver position
standard deviation 3 m and outage `[45,75)`. Separate seeded noise streams keep
paired experiments comparable. The state is `[p,v,b]`; compensation is `raw - b`.
The predictor is exact for this held-input/constant-bias model. Q uses per-sample
variance, not continuous-time spectral density. Corrections use a scalar position
measurement, a Joseph covariance update and a scalar 3-sigma gate (NIS threshold 9,
not a universal multidimensional threshold).

Truth is used for sensor generation/evaluation, not supplied to the KF. Initial
position/velocity means are zero with nonzero covariance. The integer sensor clock,
not wall time or animation frames, drives the simulation. Immutable snapshots retain
prior/posterior state and covariance at fixes. Rejected fixes keep candidate gain
but inject zero correction. Plots preserve both sides of state-update jumps.

```sh
python3 web/tutorials/ins-gnss/tools/export.py /tmp/ins-gnss-trace
```

The export contains `trace.csv` (12,001 samples), `updates.json`, `prediction.json`,
`measurement.json` and a provenance `manifest.json` with source/output hashes.
The 2D example is separate from the drive trace. For the default drive, errors at
74.99 s are 53.3366 m IMU-only and 4.8927 m fused; final physical-bias estimate is
0.0209436 m/s². These are one declared teaching run, not a product benchmark.

## Validation

```sh
python3 web/tutorials/ins-gnss/tests/test_model.py
node web/tutorials/ins-gnss/tests/pyodide_model.mjs \
  /absolute/path/to/pyodide.mjs \
  web/tutorials/ins-gnss/src/model.py
```

The same **49 numerical tests** execute in CPython and Pyodide. They cover retained
update replay, indirect correction, Joseph covariance, discontinuities, rejection,
contour radii, closed-form prediction, factor reconstruction, analytic batch results,
sequential equivalence, row ordering without per-row gates, units, translation,
wrong-H/wrong-R counterexamples and invalid input. A desktop Python pass does not
establish browser execution; use the repository-pinned Pyodide runtime.

With repository browser-test dependencies and a built web package:

```sh
NOON_TUTORIAL_BACKEND=webgpu node web/tutorials/ins-gnss/tests/review.mjs
NOON_TUTORIAL_BACKEND=webgl node web/tutorials/ins-gnss/tests/review.mjs
NOON_TUTORIAL_BACKEND=webgpu node web/tutorials/ins-gnss/tests/player.mjs
NOON_TUTORIAL_BACKEND=webgl node web/tutorials/ins-gnss/tests/player.mjs
```

The review captures intermediate frames through Noon's semantic preview host.
Playback checks cover frozen pause output, identical fresh replay, repeated resume,
all source-completion boundaries, mobile horizontal overflow and dense cursor
samples. Inspect PNGs as well as assertions; sample-and-screenshot timings are not
real-time rendering benchmarks or universal browser-parity guarantees.

CI builds the tested checkout through `scripts/build-web-demo.sh`, including
preflight, then passes the same run's compiled runtime to both browser jobs. It
uses no expiring deployment artifact from a different commit. The focused build
omits the extra wasm-opt pass; production optimization and full native/repository
qualification are separate gates. Exact outcomes belong to run reports and PR
comments, not a blanket statement that all revisions pass.

## Reference and remaining scope

Pinned upstream: `fedorbaklanov/open-aided-navigation` at
`0d33f229a0abbefeee084cc4306bdc3a47d5862f`. Under `demo/insGnssLoose/`, use
`Documentation_InsGnssFilterLoose.pdf` for the formulation,
`StateMapInsGnssLoose.m`/`ErrorStateMapInsGnssLoose.m` for state ordering,
`insGnssLooseOde.m` for ECEF propagation,
`insGnssLooseTransMat.m`/`insGnssLooseSysNoiseMat.m` for first-order discretization,
and `InsGnssFilterLoose.m` for observations, whitening, gating, NHC and correction.

Rotation notation uses destination superscript/source subscript; the displayed
attitude injection is explicitly left/navigation-frame. The source's additive
corrective offsets differ from our subtractive physical-bias convention. General
reset requirements are not a claim that the upstream code implements an exact
exponential-map/reset. Source-specific rate-dependent variance scaling is separate
from the 2D whitening example. Reference code and equations need an audit before
literal reproduction or a full-system validation claim.

The detailed full course, complete ECEF audit/reproduction, NHC and mounting
experiments, recorded-drive provenance and broader statistical evaluation remain
unfinished. No such result is fabricated by either teaching model.
