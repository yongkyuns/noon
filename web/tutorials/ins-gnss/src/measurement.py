"""A fixed two-position-observation example for the whitening lesson.

This is a single linear Gaussian correction, not an INS or another estimator
runtime. The 2x2 routines are deliberately limited to this teaching example.
State and observation coordinates are metres. Whitening changes observation
coordinates only; corrected state coordinates remain metres.
"""
from dataclasses import dataclass
from math import isfinite, sqrt
from model import add, identity, multiply, transpose


@dataclass(frozen=True)
class PositionFixExample:
    prior: tuple = (0.0, 0.0)
    covariance: tuple = ((4.0, 0.0), (0.0, 4.0))
    observation: tuple = (4.0, 2.0)
    noise: tuple = ((5.0, 4.0), (4.0, 5.0))


@dataclass(frozen=True)
class ScalarFixStep:
    row: int
    sensitivity: tuple
    original_residual: float
    residual: float
    innovation_variance: float
    gain: tuple
    correction: tuple
    covariance: tuple


@dataclass(frozen=True)
class FixComparison:
    residual: tuple
    factor: tuple
    whitening: tuple
    white_residual: tuple
    white_sensitivity: tuple
    white_innovation_covariance: tuple
    gain: tuple
    posterior: tuple
    covariance: tuple
    scalar_steps: tuple
    diagonal_noise_posterior: tuple
    diagonal_noise_covariance: tuple
    nis: float
    white_nis: float


def fix_matrix(rows):
    return tuple(tuple(row) for row in rows)


def fix_matvec(matrix, vector):
    return tuple(sum(a*b for a, b in zip(row, vector)) for row in matrix)


def fix_cholesky(covariance):
    """Lower triangular L for a finite symmetric positive-definite 2x2 matrix."""
    if len(covariance) != 2 or any(len(row) != 2 for row in covariance):
        raise ValueError('Expected a 2x2 covariance')
    a, b, c, d = (*covariance[0], *covariance[1])
    if not all(isfinite(x) for x in (a, b, c, d)):
        raise ValueError('Covariance must be finite')
    if abs(b-c) > 1e-12*max(abs(a), abs(b), abs(c), abs(d)) or a <= 0:
        raise ValueError('Covariance must be symmetric positive definite')
    l00 = sqrt(a)
    l10 = b/l00
    remainder = d-l10*l10
    if remainder <= 0:
        raise ValueError('Covariance must be positive definite')
    return ((l00, 0.0), (l10, sqrt(remainder)))


def fix_forward(factor, vector):
    """Solve Lx=b; the factor is produced by fix_cholesky."""
    x = vector[0]/factor[0][0]
    return (x, (vector[1]-factor[1][0]*x)/factor[1][1])


def fix_solve(covariance, vector):
    """Solve a 2x2 SPD system by two triangular solves; no matrix inverse."""
    factor = fix_cholesky(covariance)
    y = fix_forward(factor, vector)
    x1 = y[1]/factor[1][1]
    return ((y[0]-factor[1][0]*x1)/factor[0][0], x1)


def fix_batch(prior, covariance, observation, noise):
    """H=I batch correction with Joseph covariance; returns (mean, P, K)."""
    residual = tuple(z-x for z, x in zip(observation, prior))
    innovation = add(covariance, noise)
    # K = P S^-1; symmetry of S lets each row be found by a solve.
    gain = tuple(fix_solve(innovation, row) for row in covariance)
    correction = fix_matvec(gain, residual)
    posterior = tuple(x+dx for x, dx in zip(prior, correction))
    remaining = [[float(i == j)-gain[i][j] for j in range(2)] for i in range(2)]
    joseph = add(multiply(multiply(remaining, covariance), transpose(remaining)),
                 multiply(multiply(gain, noise), transpose(gain)))
    return posterior, fix_matrix(joseph), gain


def fix_sequential(covariance, white_residual, white_sensitivity, order=(0, 1)):
    """Update an error mean from zero using the same fixed linearization.

    Each whitened measurement has noise variance one. Recompute the conditional
    residual after each row; reusing the original residual is incorrect.
    There is no per-row gate: that could make ordering alter acceptance.
    """
    if tuple(sorted(order)) != (0, 1):
        raise ValueError('Process each of the two observations exactly once')
    correction = (0.0, 0.0)
    current = fix_matrix(covariance)
    steps = []
    for row in order:
        h = white_sensitivity[row]
        residual = white_residual[row]-sum(a*b for a, b in zip(h, correction))
        ph = fix_matvec(current, h)
        variance = 1.0+sum(a*b for a, b in zip(h, ph))
        gain = tuple(x/variance for x in ph)
        correction = tuple(x+k*residual for x, k in zip(correction, gain))
        remaining = [[float(i == j)-gain[i]*h[j] for j in range(2)] for i in range(2)]
        joseph = add(multiply(multiply(remaining, current), transpose(remaining)),
                     [[a*b for b in gain] for a in gain])
        current = fix_matrix(joseph)
        steps.append(ScalarFixStep(row, tuple(h), white_residual[row], residual,
                                   variance, gain, correction, current))
    return tuple(steps)


def compare_position_fix(example=PositionFixExample()):
    """Compute every value used by chapter 10 from this one declared example."""
    if len(example.prior) != 2 or len(example.observation) != 2:
        raise ValueError('Expected two position coordinates')
    if not all(isfinite(x) for x in (*example.prior, *example.observation)):
        raise ValueError('Position coordinates must be finite')
    fix_cholesky(example.covariance)
    factor = fix_cholesky(example.noise)
    whitening = fix_matrix(transpose([fix_forward(factor, e) for e in identity(2)]))
    residual = tuple(z-x for z, x in zip(example.observation, example.prior))
    white_residual = fix_forward(factor, residual)
    sensitivity = whitening  # H=I in the original metre coordinates.
    innovation = add(example.covariance, example.noise)
    white_innovation = fix_matrix(multiply(multiply(whitening, innovation), transpose(whitening)))
    posterior, covariance, gain = fix_batch(example.prior, example.covariance,
                                           example.observation, example.noise)
    steps = fix_sequential(example.covariance, white_residual, sensitivity)
    diagonal_noise = ((example.noise[0][0], 0.0), (0.0, example.noise[1][1]))
    naive, naive_covariance, _ = fix_batch(example.prior, example.covariance,
                                          example.observation, diagonal_noise)
    nis = sum(a*b for a, b in zip(residual, fix_solve(innovation, residual)))
    white_nis = sum(a*b for a, b in zip(white_residual, fix_solve(white_innovation, white_residual)))
    return FixComparison(residual, factor, whitening, white_residual, sensitivity,
                         white_innovation, gain, posterior, covariance, steps,
                         naive, naive_covariance, nis, white_nis)
