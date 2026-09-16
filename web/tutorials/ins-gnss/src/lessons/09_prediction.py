"""Prediction during a GNSS outage: dynamics, actual replay, then a budget.

All run values come from the same retained 1D experiment as chapter 7. The
closed-form calculation independently checks the stepwise covariance. Nothing
integrates on the rendering clock; each beat uses ordinary Noon objects.
"""
from math import sqrt
from model import Experiment, simulate
from prediction import outage_prediction
from visuals import (Stage, Plot, text, equation, notes, FUSED,
                     GNSS, UNCERTAINTY, MUTED)
from noon import Dot, Line

READ_HOLD = 7.0
REPLAY_SECONDS = 8.0
EXAMPLE_BIAS_ERROR = .01  # A declared thought experiment, not the run's estimate.
BUDGET_COLUMNS = (-3.3, 1.5, 4.8)


async def _error_transport(stage, elapsed):
    stage.add(text('First: propagate a possible error', (0, 1.85), 29),
              text('For every component: error = true value − estimated value.', (0, 1.10), 22, MUTED),
              equation(r'dot(delta p) = delta v', (-3.1, .1), 38, max_width=6),
              *notes(['Teaching state: position, velocity, bias', 'Corrected acceleration = raw − bias',
                      'Bias is constant in this model.'], start_y=.65, gap=.70))
    await stage.say('Position error inherits velocity error. A constant unknown bias keeps changing velocity error.', hold=READ_HOLD)
    await stage.reveal(equation(r'delta v(T) = delta v_0 - T delta b_0', (-3.1, -1.0), 33, max_width=6), hold=4)
    await stage.reveal(equation(r'delta p(T) = delta p_0 + T delta v_0 - 1/2 T^2 delta b_0',
                               (0, -2.10), 33, max_width=12.5), hold=5)
    drift = -.5 * elapsed**2 * EXAMPLE_BIAS_ERROR
    await stage.say(f'Example: bias error +{EXAMPLE_BIAS_ERROR:g} m/s² alone gives {drift:.3f} m position error after {elapsed:g} s.', hold=READ_HOLD)
    await stage.clear()


async def _exact_transition(stage, elapsed):
    stage.add(equation(r'delta x(T) = Phi(T) delta x_0', (-3.1, 1.65), 37, max_width=6),
              equation(r'Phi(T) = mat(1,T,-T^2/2; 0,1,-T; 0,0,1)', (-3.1, .05), 36, max_width=6.2),
              *notes(['Each row says where an error goes.', 'Row 1: position at the end', 'Row 2: velocity at the end',
                      'Row 3: unchanged bias error'], gap=.72),
              text('Exact for this constant-bias 1D model, before new noise is added.', (0, -2.1), 22, MUTED))
    await stage.say('Phi packages those three equations. The negative signs follow from subtracting the estimated physical bias.', hold=READ_HOLD)
    await stage.say(f'At T = {elapsed:g} s, the position row is [1, {elapsed:g}, {-elapsed**2/2:g}]. Bias uncertainty gets a large multiplier.', hold=READ_HOLD)
    await stage.clear()


async def _outage_replay(stage, samples, start, event, config):
    plot = Plot((start.time, event.time), (-20, 20), 'Simulation time / s',
                'Position error: true − estimate / m',
                xticks=(start.time, (start.time + event.time)/2, event.time), yticks=(-20, -10, 0, 10, 20))
    stride = max(1, config.imu_hz // 4)
    window = [s for s in samples if start.time <= s.time < event.time][::stride]
    # Both traces stop at the PRE-update event. The posterior is added separately.
    error_points = [(s.time, -s.error) for s in window] + [(event.time, event.truth - event.prior_position)]
    sigma_points = [(s.time, s.sigma) for s in window] + [(event.time, sqrt(event.prior_covariance[0][0]))]
    cursor = plot.cursor(start.time).set_color(MUTED)
    stage.add(*plot.objects,
              plot.curve([(x, 2*y) for x, y in sigma_points], UNCERTAINTY, 2),
              plot.curve([(x, -2*y) for x, y in sigma_points], UNCERTAINTY, 2),
              plot.curve(error_points, FUSED), cursor,
              text('Error', (-4.8, -2.85), 20, FUSED),
              text('Marginal ±2σ', (-2.2, -2.85), 20, UNCERTAINTY),
              *notes([f'Last accepted fix: {start.time:g} s',
                      f'Unavailable: [{config.outage_start:g}, {config.outage_end:g}) s',
                      f'Next observed fix: {event.time:g} s',
                      f'Prediction interval: {event.time-start.time:g} s',
                      'Complete recorded trace + cursor'], gap=.66))
    await stage.say(f'The outage label spans {config.outage_end-config.outage_start:g} seconds. But {event.time-start.time:g} seconds separate the last accepted fix and reacquisition.', hold=READ_HOLD)
    middle = (start.time + event.time)/2
    await plot.move_cursor(stage.scene, cursor, start.time, middle, REPLAY_SECONDS)
    mid = samples[round(middle * config.imu_hz)]
    await stage.say(f'At {middle:g} s: position σ = {mid.sigma:.3f} m. Estimated bias and its variance are unchanged without a fix.', hold=READ_HOLD)
    await plot.move_cursor(stage.scene, cursor, middle, event.time, REPLAY_SECONDS)
    await stage.say(f'Before the {event.time:g}-second update, position σ has grown from {start.sigma:.3f} m to {sqrt(event.prior_covariance[0][0]):.3f} m.', hold=READ_HOLD)
    # A correction is a discontinuity at one timestamp, not physical motion.
    jumps = [(event.truth-event.prior_position, -event.error),
             (2*sqrt(event.prior_covariance[0][0]), 2*event.sigma),
             (-2*sqrt(event.prior_covariance[0][0]), -2*event.sigma)]
    await stage.reveal(*(Line(plot.point(event.time, a), plot.point(event.time, b),
                              color=GNSS) for a, b in jumps),
                       Dot(radius=.06, color=GNSS).move_to(plot.point(event.time, -event.error)), hold=4)
    await stage.say(f'The fix changes the estimate at the same timestamp. Position σ drops to {event.sigma:.3f} m; the vehicle does not jump.', hold=READ_HOLD)
    await stage.clear()


async def _variance_budget(stage, event, process, terms):
    columns = BUDGET_COLUMNS
    stage.add(text(f'Where did the {event.prior_covariance[0][0]:.3f} m² variance come from?', (0, 1.90), 29),
              equation(r'P_(p p)(T) = h P_0 h^T + Q_(p p)', (0, 1.05), 35, max_width=12),
              equation(r'h = (1,T,-T^2/2)', (0, .30), 29, max_width=12))
    diagonal = terms['position'] + terms['velocity'] + terms['bias']
    cross = terms['position_velocity'] + terms['position_bias'] + terms['velocity_bias']
    rows = (('Initial marginal terms', diagonal), ('Initial cross-covariance terms', cross),
            ('New acceleration sample noise', process[0][0]))
    for y, (name, value) in zip((-.40, -1.13, -1.86), rows):
        stage.add(text(name, (columns[0], y), 23, max_width=6.0),
                  text(f'{value:+.3f} m²', (columns[1], y), 24, FUSED, max_width=3),
                  text('P₀' if name.startswith('Initial') else 'Q', (columns[2], y), 23, MUTED))
    await stage.say('Propagate the complete covariance, not just its diagonal. Here the cross-terms add substantial position variance.', hold=READ_HOLD)
    await stage.say('Cross-terms can be negative in other cases. These signed algebraic terms are not independent noise sources.', hold=READ_HOLD)
    await stage.clear()


async def _no_new_noise(stage, start, event, inherited):
    stage.add(text('Thought experiment: remove only NEW sample noise', (0, 1.8), 29, GNSS),
              text('Keep the same initial covariance and the same prediction interval.', (0, 1.00), 22, MUTED))
    for y, name, value, color in (
            (.0, 'At the last accepted fix', start.sigma, MUTED),
            (-.85, 'At reacquisition, with Q = 0', sqrt(inherited[0][0]), GNSS),
            (-1.70, 'Actual pre-update prediction', sqrt(event.prior_covariance[0][0]), FUSED)):
        stage.add(text(name, (-2.45, y), 25, color, max_width=7.4),
                  text(f'σp = {value:.3f} m', (3.7, y), 27, color, max_width=4.6))
    await stage.say('Even a noiseless new acceleration sample cannot erase the uncertainty already present in velocity and bias.', hold=READ_HOLD)
    await stage.say('This is a covariance-only comparison from the same starting point, not a second drive or a perfect-IMU accuracy claim.', hold=READ_HOLD)
    await stage.clear()


async def _reference_discretization(stage):
    stage.add(text('Now connect the example to the full reference', (0, 1.85), 29),
              equation(r'dot(delta x) = F delta x + G w', (-3.1, .7), 35, max_width=6),
              equation(r'P^- = Phi P^+ Phi^T + Q_d', (-3.1, -.65), 35, max_width=6),
              *notes(['Full ECEF model is nonlinear.', 'F and G are local linearizations.', 'Reference uses first-order steps:',
                      'Φ ≈ I + F Δt', 'Qd ≈ G Qc Gᵀ Δt'], gap=.64),
              text('Our closed form is exact for the teaching model only.', (0, -2.13), 22, GNSS))
    await stage.say('The same propagation structure applies, but our simple Phi is not the reference’s full ECEF transition matrix.', hold=READ_HOLD)
    await stage.finish('Covariance remembers coupled errors. New process noise adds uncertainty; it is not the only reason uncertainty grows.')


async def lesson_prediction(scene):
    config = Experiment()
    samples = simulate(config)
    start, event, inherited, process, terms = outage_prediction(samples, config)
    stage = Stage(scene, 9, 'What grows without GNSS?', 'Follow the same outage from error dynamics to a numerical covariance budget.')
    await _error_transport(stage, event.time - start.time)
    await _exact_transition(stage, event.time - start.time)
    await _outage_replay(stage, samples, start, event, config)
    await _variance_budget(stage, event, process, terms)
    await _no_new_noise(stage, start, event, inherited)
    await _reference_discretization(stage)
