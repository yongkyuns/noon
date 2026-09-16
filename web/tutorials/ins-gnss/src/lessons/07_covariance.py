"""Follow one real GNSS event from innovation to indirect bias correction.

All displayed numbers come from the same recorded update at reacquisition.
The simulated time is frozen while explanations advance in authored time.
No new filter, rendering callback, or animation scheduler is introduced here.
"""
from math import sqrt
from model import Experiment, correct, simulate, display_points
from visuals import (Stage, Plot, text, equation, notes, confidence_ellipse,
                     FUSED, GNSS, INS, UNCERTAINTY, MUTED, LAYOUT)
from noon import Dot, Line


# Diagram composition, not hidden physical/model parameters.
TABLE_COLUMNS = (-5.25, -2.25, .75, 4.1)
TABLE_ROWS = (-.10, -1.00, -1.90)
PAIR_BOUNDS = (-5.70, -2.05, -1.75, 1.90)
STATE_LABELS = ('Position', 'Velocity', 'Physical bias')
STATE_UNITS = ('m', 'm/s', 'm/s²')
STATE_DIGITS = (3, 4, 6)
READ_HOLD = 7.0


def _value(value, index):
    return f'{value:+.{STATE_DIGITS[index]}f} {STATE_UNITS[index]}'


def _interval(plot, mean, sigma, row, color, label):
    """A marginal +/-1 sigma whisker: never label this a joint 95% interval."""
    return [Line(plot.point(mean - sigma, row), plot.point(mean + sigma, row), color=color),
            Dot(radius=.075, color=color).move_to(plot.point(mean, row)),
            text(label, plot.point(mean, row + .40), 21, color, max_width=4.5)]


async def _innovation(stage, event):
    sigma_prior = sqrt(event.prior_covariance[0][0])
    variance = event.innovation_variance - event.prior_covariance[0][0]
    sigma_fix = sqrt(variance)
    low = min(event.prior_position - 2 * sigma_prior, event.observation - 2 * sigma_fix)
    high = max(event.prior_position + 2 * sigma_prior, event.observation + 2 * sigma_fix)
    plot = Plot((low, high), (-.6, 3.4), 'Position / m', f'Frozen at simulation t = {event.time:.2f} s',
                xticks=tuple(low + i * (high - low) / 4 for i in range(5)))
    stage.add(*plot.objects,
              *_interval(plot, event.prior_position, sigma_prior, 2.5, INS, 'Prior'),
              *_interval(plot, event.observation, sigma_fix, 1, GNSS, 'GNSS fix'),
              *notes([f'Prior: {event.prior_position:.3f} m', f'Fix: {event.observation:.3f} m',
                      'Whiskers: ±1 standard deviation'], start_y=1.55))
    await stage.say('GNSS returns after the outage. Freeze this instant: no simulation time passes during the calculation.', hold=READ_HOLD)
    await stage.reveal(equation(r'r = z - hat(p)^-', (LAYOUT.note_x, -.9)),
                       text(f'r = {event.innovation:+.3f} m', (LAYOUT.note_x, -1.75), 27, GNSS), hold=READ_HOLD)
    await stage.say('The negative innovation says the predicted position is ahead of this fix—not necessarily ahead of truth.', hold=READ_HOLD)
    await stage.clear()


async def _gain(stage, event):
    covariance = event.prior_covariance
    variance = event.innovation_variance - covariance[0][0]
    columns = TABLE_COLUMNS
    stage.add(equation(r'H = mat(1, 0, 0) quad K = P^- H^T / S', (0, 1.85), 32, max_width=12),
              text(f'S = {covariance[0][0]:.3f} + {variance:.3f} = {event.innovation_variance:.3f} m²',
                   (0, 1.10), 24, MUTED))
    for x, label in zip(columns, ('State', 'P column for position', 'Gain', 'Correction = gain × r')):
        stage.add(text(label, (x, .48), 19, MUTED, max_width=2.85))
    covariance_units = ('m²', 'm²/s', 'm²/s²')
    gain_units = ('', '/s', '/s²')
    for i, y in enumerate(TABLE_ROWS):
        await stage.reveal(
            text(STATE_LABELS[i], (columns[0], y), 22, max_width=2.7),
            text(f'{covariance[i][0]:+.6f} {covariance_units[i]}', (columns[1], y), 20, max_width=2.8),
            text(f'{event.gain[i]:+.6f} {gain_units[i]}', (columns[2], y), 23, FUSED, max_width=2.8),
            text(_value(event.correction[i], i), (columns[3], y), 24, FUSED, max_width=3.1),
            hold=4.0)
    await stage.say('H measures only position. Nonzero cross-covariances give velocity and bias nonzero gains.', hold=READ_HOLD)
    await stage.say('The bias gain is negative. Multiplying it by the negative innovation increases the estimated bias.', hold=READ_HOLD)
    await stage.clear()


async def _joint_uncertainty(stage, event):
    """Same prior-based scale and equal screen scale on both dimensionless axes."""
    sigma_p = sqrt(event.prior_covariance[0][0])
    sigma_b = sqrt(event.prior_covariance[2][2])
    rho = event.prior_covariance[0][2] / (sigma_p * sigma_b)
    plot = Plot((-3, 3), (-3, 3), 'Position shift / prior σp', 'Bias shift / prior σb',
                bounds=PAIR_BOUNDS, xticks=(-2, 0, 2), yticks=(-2, 0, 2))
    screen_scale = (PAIR_BOUNDS[2] - PAIR_BOUNDS[0]) / 6
    prior = confidence_ellipse(plot.point(0, 0), ((1., rho), (rho, 1.)),
                               scale_x=screen_scale, scale_y=screen_scale)
    posterior_mean = (event.correction[0] / sigma_p, event.correction[2] / sigma_b)
    p = event.covariance
    posterior_pair = ((p[0][0] / sigma_p**2, p[0][2] / (sigma_p * sigma_b)),
                      (p[2][0] / (sigma_p * sigma_b), p[2][2] / sigma_b**2))
    posterior = confidence_ellipse(plot.point(*posterior_mean), posterior_pair,
                                   scale_x=screen_scale, scale_y=screen_scale).set_color(FUSED)
    stage.add(*plot.objects, prior,
              Dot(radius=.06, color=UNCERTAINTY).move_to(plot.point(0, 0)),
              *notes(['Actual position–bias covariance', 'Both contours: joint 95%', f'Prior correlation: {rho:.3f}',
                      f'Fixed scales: σp = {sigma_p:.3f} m', f'σb = {sigma_b:.6f} m/s²'], gap=.65))
    await stage.say('This tilted ellipse comes from the actual covariance. Both axes use fixed prior-standard-deviation units.', hold=READ_HOLD)
    fix_line = plot.cursor(event.innovation / sigma_p)
    await stage.reveal(fix_line, hold=2.0)
    measurement_variance = event.innovation_variance - event.prior_covariance[0][0]
    await stage.say(f'Gold marks the fix’s position. Its uncertainty is not zero: the update still uses R = {measurement_variance:g} m².', hold=READ_HOLD)
    await stage.reveal(posterior,
                       Dot(radius=.075, color=FUSED).move_to(plot.point(*posterior_mean)), hold=READ_HOLD)
    await stage.say('The green posterior moves toward lower position and higher bias, and contracts. The axes never rescale.', hold=READ_HOLD)
    await stage.clear()


async def _counterfactual(stage, event):
    """One controlled ablation: same prior means, marginals, measurement and R."""
    p = event.prior_covariance
    diagonal = [[p[i][i] if i == j else 0.0 for j in range(3)] for i in range(3)]
    r = event.innovation_variance - p[0][0]
    without, _, _, _, accepted = correct(event.prior_state, diagonal, event.observation, r)
    if not accepted:
        raise ValueError('The worked counterfactual must pass the same innovation gate')
    columns = (-4.8, -.7, 3.8)
    stage.add(text('Counterfactual: erase cross-covariances only', (0, 1.8), 29, GNSS),
              text('Same means, marginal variances, fix and measurement uncertainty.', (0, 1.0), 22, MUTED))
    for x, label in zip(columns, ('State', 'Actual correction', 'With zero cross-covariance')):
        stage.add(text(label, (x, .40), 20, MUTED, max_width=4))
    for i, y in enumerate(TABLE_ROWS):
        stage.add(text(STATE_LABELS[i], (columns[0], y), 24),
                  text(_value(event.correction[i], i), (columns[1], y), 25, FUSED),
                  text(_value(without[i] - event.prior_state[i], i), (columns[2], y), 25, GNSS))
    await stage.say('Position receives the same correction. But velocity and bias receive none when their position cross-terms are zero.', hold=READ_HOLD)
    await stage.say('This is a single-update comparison, not a second drive. It isolates exactly where indirect information enters.', hold=READ_HOLD)
    await stage.clear()


async def _truth_check(stage, event, config):
    columns = TABLE_COLUMNS
    truth = (event.truth, event.truth_velocity, config.bias)
    stage.add(text(f'After the update — still t = {event.time:.2f} s', (0, 1.8), 29),
              text('Truth is used below for evaluation only; it was not supplied to the estimator.', (0, 1.02), 22, MUTED))
    for x, label in zip(columns, ('State', 'Before', 'After', 'Simulation truth')):
        stage.add(text(label, (x, .43), 20, MUTED, max_width=2.9))
    for i, y in enumerate(TABLE_ROWS):
        stage.add(text(STATE_LABELS[i], (columns[0], y), 22, max_width=2.7),
                  text(_value(event.prior_state[i], i), (columns[1], y), 23, INS, max_width=2.9),
                  text(_value(event.state[i], i), (columns[2], y), 23, FUSED, max_width=2.9))
    await stage.say('Apply all three corrections to the prior state. A Kalman update changes the estimate, not the physical vehicle.', hold=READ_HOLD)
    await stage.reveal(*(text(_value(truth[i], i), (columns[3], y), 23, GNSS, max_width=3.0)
                         for i, y in enumerate(TABLE_ROWS)), hold=READ_HOLD)
    await stage.say('The bias correction moves in the right direction but overshoots truth. An accepted noisy fix is not perfect truth.', hold=READ_HOLD)
    await stage.clear()


async def lesson_covariance(scene):
    config = Experiment()
    samples = simulate(config)
    event = samples[round(config.outage_end * config.imu_hz)]
    if event.accepted is not True:
        raise ValueError('The worked reacquisition event must be accepted')
    stage = Stage(scene, 7, 'How a position fix learns bias', 'One measured position. Three corrected state components. Why?')
    await _innovation(stage, event)
    await _gain(stage, event)
    await _joint_uncertainty(stage, event)
    await _counterfactual(stage, event)
    await _truth_check(stage, event, config)
    plot = Plot((0, config.duration), (-.01, .06), 'Simulation time / s', 'Estimated physical bias / (m/s²)',
                xticks=(0, 30, 60, 90, 120), yticks=(0, .02, .04, .06))
    stage.add(*plot.objects, plot.curve(display_points(samples, 'bias', config.imu_hz // 2), FUSED),
              plot.curve([(0, config.bias), (config.duration, config.bias)], GNSS, 1.5),
              plot.cursor(event.time),
              *notes(['Return to the complete run', 'Green: estimated physical bias', 'Gold: injected physical bias',
                      'The vertical line marks our update.', 'Update jumps are not smoothed.'], gap=.65))
    await stage.finish('Prediction builds cross-covariance; the position residual uses it to correct velocity and bias.')
