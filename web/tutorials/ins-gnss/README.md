# INS + GNSS: a Noon navigation tutorial

An original, chaptered tutorial based on Fedor Baklanov's *INS/GNSS loose coupling
filter* documentation. Fourteen chapters introduce sensor errors, frames,
mechanization, Kalman correction, covariance, error states, vehicle constraints,
calibration and failure detection. The current sequence is **662.65 authored
seconds**, with pauseable reading notes; it is not the planned 45–60-minute course.

**Numerical scope:** the computed results use a reproducible **1D
position/velocity/physical-accelerometer-bias KF**. The full **24-error-state ECEF
filter is explained, not numerically reproduced or validated**. The animation,
reading notes and page footer retain this distinction.

## Run

From the Noon checkout:

```sh
bash scripts/build-web-demo.sh
python3 -m http.server --bind 127.0.0.1 --directory web 8080
```

Open `http://127.0.0.1:8080/tutorials/ins-gnss/`. After editing tutorial Python,
rebuild just its single-source browser entry with:

```sh
python3 web/tutorials/ins-gnss/tools/build.py
```

The page uses Noon's existing Python and rendering workers. Do not install a
separate package named `noon` or substitute Manim. The browser must be able to load
Noon's configured Pyodide resources. Scene code uses **only Noon and the Python
standard library**: no NumPy, SciPy, Matplotlib, Pandas or package installer.

Play/Pause controls forward sampling. Restart creates a fresh chapter session.
Arbitrary backward seek is not qualified and is not offered. Pausing stops host
sample requests; it does not invoke the unsupported source-owned runtime pause
operation. Desktop or landscape viewing is recommended for equations.

## Code organization

| Path | Responsibility |
| --- | --- |
| `src/model.py` | Immutable configuration, deterministic sensor generation, 3-state KF, retained update snapshots and metrics. No rendering imports. |
| `src/uncertainty.py` | Independently tested 2D Gaussian contour geometry. No Noon dependency. |
| `src/prediction.py` | Closed-form covariance checks and a signed variance budget for measurement-free propagation. No second estimator. |
| `src/visuals.py` | Shared layout, typography and small helpers constructing ordinary Noon objects. |
| `src/lessons/` | Lesson functions in teaching order. Explicit async helpers cross the normal authoring continuation barriers. |
| `src/entry.py` | One Scene entry and chapter selection. |
| `chapters.json` | Titles, reading notes, expected durations and selected review times. |
| `player.js` | Existing worker lifecycle and forward-playback controls; no numerical model or alternative renderer. |
| `tools/` | Reproducible source bundle and numerical export. |
| `tests/` | Numerical, Pyodide, actual browser-frame and playback checks. |

Edit `src/`, not generated `scene.py`. Physical assumptions belong in `Experiment`;
shared visual settings belong in `Layout` and the palette. Diagram-specific
coordinates describe composition, not hidden numerical results. The bundler removes
only imports of the known local modules; it does not add another Python loader.

## Worked event: chapter 7

The expanded lesson freezes simulation time at the **75-second GNSS reacquisition**.
It progresses through the innovation, gain entries and units, actual state
corrections, prior/posterior uncertainty, a controlled comparison and evaluation
against truth. Each beat is a small named async function.

All displayed values come from the same retained update. A controlled comparison
removes only cross-covariances while retaining means, marginal variances, the fix
and measurement variance. Position receives the same correction; velocity and bias
receive zero correction. The final truth comparison shows that an accepted noisy
fix can overshoot the true bias rather than magically recovering it.

The joint position–bias contours use the actual covariance, fixed prior-based
normalization and equal display scale on both axes. They are **modelled 95% joint
Gaussian contours**, not empirical coverage results. The separate scalar whiskers
show **±1 standard deviation**, not joint confidence intervals.

## Worked prediction: chapter 9

The 140.1-second lesson follows error transport → exact teaching-model Phi →
the recorded outage → a signed position-variance budget → removal of new noise
only → the reference's different discretization. `src/prediction.py` is pure
analysis of the existing KF, not another estimator. The exporter adds
`prediction.json` and its source/output hashes.

The last accepted fix is at 44 s, not 45 s: the covariance propagates 3,100
100-Hz samples to the 75 s pre-update snapshot. Eleven additional tests check
that closed-form accumulation agrees with the original samplewise predictor,
including intermediate checkpoints, cross-term signs and zero-new-noise behavior.
The current chapter explicitly plots true-minus-estimated error; its sign is
opposite chapter 1's estimate-minus-truth overview. Bounds are marginal ±2σ,
not joint probability contours or empirical coverage. The correction is a
vertical jump at one timestamp; no time interpolation smooths it into motion.

## Numerical assumptions and provenance

Defaults: 120 seconds, 100 Hz IMU, 1 Hz GNSS, constant physical bias 0.02 m/s²,
independent acceleration sample noise 0.04 m/s², GNSS standard deviation 3 m and
outage `[45,75)` seconds. Separate seeded streams preserve paired noise realizations.

The state is `[p,v,b]`; acceleration compensation is `raw - b`. The filter uses the
exact held-input transition for this model, sample-noise process covariance, scalar
position observations and Joseph covariance correction. A scalar 3-sigma innovation
gate has NIS threshold 9; this is not a universal multidimensional threshold.

Truth generates observations and evaluates error; it is not supplied to the KF.
Initial position/velocity means are explicitly zero with nonzero covariance.
Simulation uses integer sensor ticks, never animation-frame wall time. Plots retain
both sides of position, velocity and bias corrections at each measurement timestamp.
The opening graph is a complete recorded trace with a cursor, not a live trace reveal.

`Sample` retains immutable prior/posterior state and covariance at observation
events. Candidate gain and injected correction are distinct: a rejected event keeps
its candidate gain but injects zero correction. Prediction-only samples have no
measurement snapshot.

```sh
python3 web/tutorials/ins-gnss/tools/export.py /tmp/ins-gnss-trace
```

The export includes `trace.csv` with 12,001 samples, `updates.json` with complete
before/after observation events, `prediction.json` with the outage covariance
budget, and `manifest.json` with settings, metrics and SHA-256 provenance.
Evaluation truth is explicitly labeled. For the default run, position errors at
74.99 s are 53.3366 m IMU-only and 4.8927 m fused; the final bias estimate is
0.0209436 m/s². This is a declared teaching run, not a product benchmark.

## Validation

```sh
python3 web/tutorials/ins-gnss/tests/test_model.py
node web/tutorials/ins-gnss/tests/pyodide_model.mjs \
  /absolute/path/to/pyodide.mjs \
  web/tutorials/ins-gnss/src/model.py
```

The same **33 tests** execute in CPython and Pyodide. They cover the original model,
retained-event replay, indirect correction, Joseph versus conditional covariance,
update discontinuities, rejected snapshots, the Mahalanobis radius of every
contour vertex and closed-form prediction against the samplewise filter.
Pyodide qualification uses the repository-pinned runtime, not a claim that desktop
Python compatibility proves browser functionality.

With the repository browser-test dependencies and built web package:

```sh
NOON_TUTORIAL_BACKEND=webgpu node web/tutorials/ins-gnss/tests/review.mjs
NOON_TUTORIAL_BACKEND=webgl node web/tutorials/ins-gnss/tests/review.mjs
NOON_TUTORIAL_BACKEND=webgpu node web/tutorials/ins-gnss/tests/player.mjs
NOON_TUTORIAL_BACKEND=webgl node web/tutorials/ins-gnss/tests/player.mjs
```

The review uses Noon's semantic preview host and captures actual intermediate
frames. Playback tests cover frozen pause output, identical fresh replay, repeated
resume, all chapter completions, mobile horizontal overflow and 91 dense cursor
samples. Inspect the PNGs as well as the assertions. Sample-and-screenshot timing
is **not** a real-time rendering benchmark or universal browser-parity guarantee.

CI builds the tested checkout through `scripts/build-web-demo.sh`, including its
preflight, then supplies that same run's compiled runtime to both browser jobs.
It no longer depends on an expiring deployment artifact from another commit.
The focused build disables the extra wasm-opt pass; production optimization and
full native/repository qualification remain separate gates. Reports retain runtime
identity, tested tutorial source and numerical exports.

## Reference conventions and remaining scope

Upstream revision: `0d33f229a0abbefeee084cc4306bdc3a47d5862f` in
[fedorbaklanov/open-aided-navigation](https://github.com/fedorbaklanov/open-aided-navigation/tree/0d33f229a0abbefeee084cc4306bdc3a47d5862f).
Relevant files under `demo/insGnssLoose/` are:

- `Documentation_InsGnssFilterLoose.pdf`: technical reference.
- `StateMapInsGnssLoose.m` / `ErrorStateMapInsGnssLoose.m`: 26 nominal coefficients and 24 error coordinates.
- `insGnssLooseOde.m`: ECEF mechanization and Earth-rate terms.
- `insGnssLooseTransMat.m` / `insGnssLooseSysNoiseMat.m`: first-order discretization.
- `InsGnssFilterLoose.m`: initialization, observations, whitening, gating, NHC and correction.

Rotation notation uses destination superscript/source subscript. Quaternion
injection is explicitly left/navigation-frame. The reference's additive corrective
offsets differ in sign from this simulator's subtractive physical-bias convention.
General error-state reset requirements do not claim that the upstream code uses an
exact exponential-map/reset implementation. Receiver GNSS is an observation, not
independent ground truth.

The detailed full course, complete ECEF model audit/reproduction, NHC and mounting
experiments, recorded-drive provenance and broader statistical evaluation remain
unfinished. No such numerical result is fabricated by the 1D teaching model.
