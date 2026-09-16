"""A frozen correlated position fix: coordinates, conditioning, and correction.

The small pure-numerical example owns all displayed values. Named explanation
beats own composition only; there is no per-frame numerical calculation.
"""
from math import ceil, log, sqrt
from measurement import PositionFixExample, compare_position_fix, fix_matvec
from uncertainty import confidence_contour
from visuals import Stage, Plot, text, equation, notes, arrow, FUSED, GNSS, INS, MUTED

FIX_READ_HOLD = 7.0
FIX_LEFT = -3.25
FIX_RIGHT = 3.30
FIX_SQUARE_LEFT = (-5.20, -2.10, -1.30, 1.80)
FIX_SQUARE_RIGHT = (1.30, -2.10, 5.20, 1.80)


def fix_matrix_tex(matrix, digits=3):
    """Format only numbers; formula structure remains visible in lesson code."""
    return 'mat(' + '; '.join(', '.join(f'{x:.{digits}f}' for x in row) for row in matrix) + ')'


def fix_vector_text(vector):
    return '(' + ', '.join(f'{x:+.3f}' for x in vector) + ') m'


def fix_noise_limit(covariance):
    # Extrema of a 95% Gaussian contour, rounded outwards to whole units.
    return ceil(sqrt(-2*log(.05)*max(covariance[0][0], covariance[1][1])))


def fix_noise_plot(bounds, limit, x_label, y_label):
    # Equal bounds and ranges are essential: correlation must not be distorted.
    return Plot((-limit, limit), (-limit, limit), x_label, y_label, bounds=bounds,
                xticks=(-limit, 0, limit), yticks=(-limit, 0, limit))


async def _fix_innovation(stage):
    rows = ((r'r = z - h(hat(x)^-)', 'Measurement minus prediction'),
            (r'S = H P^- H^T + R', 'Uncertainty of that difference'),
            (r'K = P^- H^T S^(-1)', 'Map the innovation into a correction'))
    for index, (formula, meaning) in enumerate(rows):
        y = 1.6-index*1.15
        await stage.reveal(equation(formula, (FIX_LEFT, y), 33, max_width=6),
                           text(meaning, (FIX_RIGHT, y), 23, MUTED, max_width=6), hold=4)
    await stage.say('The reference observes three ECEF position coordinates. We isolate two coordinates to make one update inspectable.', hold=FIX_READ_HOLD)
    await stage.clear()


async def _fix_correlated_noise(stage, example):
    plot = fix_noise_plot(FIX_SQUARE_LEFT, fix_noise_limit(example.noise), 'Noise component 1 / m', 'Noise component 2 / m')
    stage.add(*plot.objects, plot.curve(confidence_contour((0, 0), example.noise), GNSS),
              *notes(['One declared linear example', 'Two position coordinates; H = I',
                      f'Prior mean: {fix_vector_text(example.prior)}',
                      f'Observed: {fix_vector_text(example.observation)}'], start_y=1.5, gap=.6),
              text('Measurement-noise 95% joint contour', (FIX_LEFT, -2.85), 20, GNSS, max_width=6))
    await stage.say('The tilted ellipse says the two errors tend to move together. It is a noise model, not a vehicle trajectory.', hold=FIX_READ_HOLD)
    await stage.reveal(equation('R = '+fix_matrix_tex(example.noise, 0), (FIX_RIGHT-1.35, -1.35), 32),
                       equation('P^- = '+fix_matrix_tex(example.covariance, 0), (FIX_RIGHT+1.35, -1.35), 32), hold=4)
    await stage.say('This covariance also results from rotating independent variances 9 and 1 m² by 45 degrees.', hold=FIX_READ_HOLD)
    await stage.clear()


async def _fix_whitening_geometry(stage, example, result):
    original = fix_noise_plot(FIX_SQUARE_LEFT, fix_noise_limit(example.noise), 'Noise component 1 / m', 'Noise component 2 / m')
    white = fix_noise_plot(FIX_SQUARE_RIGHT, 3, 'Whitened component 1', 'Whitened component 2')
    contour = confidence_contour((0, 0), example.noise)
    # Transform the same numerical points; do not independently invent a circle.
    transformed = [fix_matvec(result.whitening, point) for point in contour]
    stage.add(*original.objects, original.curve(contour, GNSS),
              arrow((-.8, 0), (.75, 0), FUSED), text('W', (0, .5), 30, FUSED),
              text('Original noise / metres', (FIX_LEFT, -2.85), 20, GNSS, max_width=6))
    await stage.say('Whitening changes observation coordinates. It stretches and shears the same noise contour into a circle.', hold=FIX_READ_HOLD)
    await stage.reveal(*white.objects, white.curve(transformed, FUSED),
                       text('Transformed noise / dimensionless', (FIX_RIGHT, -2.85), 20, FUSED, max_width=6), hold=4)
    await stage.say('Both are modelled 95% joint contours. Different units mean their on-screen sizes are not physical distances.', hold=FIX_READ_HOLD)
    await stage.clear()


async def _fix_transform_model(stage, result):
    stage.add(equation(r'R = L L^T', (FIX_LEFT, 1.70), 35),
              equation(r'W = L^(-1)', (FIX_RIGHT, 1.70), 35),
              equation('L = '+fix_matrix_tex(result.factor), (FIX_LEFT, .65), 30, max_width=6),
              equation('W = '+fix_matrix_tex(result.whitening), (FIX_RIGHT, .65), 30, max_width=6),
              text('Solve triangular systems; do not form a general inverse.', (0, -2.1), 23, MUTED))
    await stage.say('A lower-triangular Cholesky factor gives W. The second whitened row mixes both original measurements.', hold=FIX_READ_HOLD)
    await stage.reveal(equation(r'r_w = W r', (FIX_LEFT, -.65), 35),
                       text(f'rw = ({result.white_residual[0]:+.3f}, {result.white_residual[1]:+.3f})', (FIX_LEFT, -1.4), 24, GNSS),
                       equation(r'H_w = W H quad R_w = I', (FIX_RIGHT, -.90), 31, max_width=6), hold=5)
    await stage.say('Transform H as well as the residual. The observations change coordinates; the corrected position stays in metres.', hold=FIX_READ_HOLD)
    await stage.clear()


async def _fix_scalar_conditioning(stage, result):
    first, second = result.scalar_steps
    stage.add(equation(r'r_i = r_(w,i) - h_i delta hat(x)', (0, 1.7), 37, max_width=12),
              text('Keep the same linearization; update the error mean after each row.', (0, .75), 24, MUTED))
    await stage.say('Start with zero estimated error. After the first row, the second residual must use the already-corrected prediction.', hold=FIX_READ_HOLD)
    await stage.reveal(text('After row 1', (FIX_LEFT, -.35), 24, FUSED),
                       text(fix_vector_text(first.correction), (FIX_RIGHT, -.35), 27, FUSED),
                       text(f'S₁ = {first.innovation_variance:.3f}, not 1', (0, -1.35), 24, MUTED), hold=5)
    await stage.say('Whitening made measurement-noise variance one. Prediction uncertainty still contributes to the scalar innovation variance.', hold=FIX_READ_HOLD)
    await stage.clear()
    stage.add(text('The second residual changes sign', (0, 1.65), 30, GNSS),
              text('Original row 2 residual', (FIX_LEFT, .4), 24, MUTED),
              text(f'{second.original_residual:+.3f}', (FIX_RIGHT, .4), 30, MUTED))
    await stage.reveal(text('Conditional row 2 residual', (FIX_LEFT, -.65), 24, FUSED),
                       text(f'{second.residual:+.3f}', (FIX_RIGHT, -.65), 30, FUSED),
                       equation(r'r_2 = r_(w,2) - h_2 delta hat(x)_1', (0, -1.8), 33, max_width=12), hold=5)
    await stage.say('Reusing the original residual would apply the wrong second correction. All of this happens at one frozen sensor timestamp.', hold=FIX_READ_HOLD)
    await stage.clear()


async def _fix_batch_equivalence(stage, example, result):
    stage.add(equation(r'P^+ = (I-K H) P^- (I-K H)^T + K R K^T', (0, 1.55), 37, max_width=12.5),
              text('Joseph covariance update', (0, .55), 25, MUTED))
    await stage.say('A batch solve and two correctly whitened scalar updates must produce the same mean and covariance.', hold=FIX_READ_HOLD)
    await stage.reveal(text('Batch posterior', (FIX_LEFT, -.4), 24, FUSED),
                       text(fix_vector_text(result.posterior), (FIX_RIGHT, -.4), 27, FUSED),
                       text('Sequential posterior', (FIX_LEFT, -1.3), 24, GNSS),
                       text(fix_vector_text(tuple(x+d for x, d in zip(example.prior, result.scalar_steps[-1].correction))),
                            (FIX_RIGHT, -1.3), 27, GNSS), hold=5)
    await stage.say('Reversing row order gives the same result here: linear observations, the same prior, no per-row gates, and one fixed linearization.', hold=FIX_READ_HOLD)
    await stage.clear()


async def _fix_wrong_noise(stage, result):
    stage.add(text('Deleting correlation is not whitening', (0, 1.7), 30, GNSS),
              text('Same prior and observation; replace R by diag(R).', (0, .80), 24, MUTED),
              text('Correct correlated model', (FIX_LEFT, -.1), 24, FUSED),
              text(fix_vector_text(result.posterior), (FIX_RIGHT, -.1), 27, FUSED))
    await stage.say('Setting off-diagonal entries to zero changes the measurement model. It does not just change numerical coordinates.', hold=FIX_READ_HOLD)
    await stage.reveal(text('Incorrect independent model', (FIX_LEFT, -1.05), 24, INS),
                       text(fix_vector_text(result.diagonal_noise_posterior), (FIX_RIGHT, -1.05), 27, INS),
                       text('No ground truth is asserted for this single-fix example.', (0, -2.1), 22, MUTED), hold=5)
    await stage.say('The posterior changes. Ignoring correlation can overstate confidence in some directions and understate it in others.', hold=FIX_READ_HOLD)
    await stage.clear()


async def _fix_innovation_distance(stage, result):
    stage.add(equation(r'S_w = H_w P^- H_w^T + I', (0, 1.6), 39, max_width=12),
              equation('S_w = '+fix_matrix_tex(result.white_innovation_covariance), (FIX_LEFT, .1), 34, max_width=6),
              *notes(['Measurement noise: Rw = I', 'Innovation covariance: Sw ≠ I',
                      'The prior is still uncertain.'], start_y=.95, gap=.65))
    await stage.say('Whitening R does not whiten the whole innovation. Do not gate by the squared length of rw alone.', hold=FIX_READ_HOLD)
    await stage.clear()
    stage.add(equation(r'"NIS" = r^T S^(-1) r = r_w^T S_w^(-1) r_w', (0, 1.6), 37, max_width=12.5))
    await stage.reveal(text(f'Original NIS: {result.nis:.6f}', (FIX_LEFT, .3), 27, FUSED),
                       text(f'Whitened NIS: {result.white_nis:.6f}', (FIX_RIGHT, .3), 27, FUSED),
                       text(f'Noise-only squared length: {sum(x*x for x in result.white_residual):.3f} — a different statistic',
                            (0, -1), 23, INS), hold=5)
    await stage.say('The correct innovation distance is coordinate-invariant. Its test threshold depends on dimension and the chosen false-alarm probability.', hold=FIX_READ_HOLD)
    await stage.clear()


async def _fix_reference_bridge(stage):
    stage.add(text('Back to the three-coordinate reference', (0, 1.65), 30),
              *notes(['Receiver position → ECEF residual', 'Whiten residual and sensitivity',
                      'Gate with innovation covariance', 'Correct; inject; reset error coordinates'], start_y=1, gap=.70),
              equation(r'H = [I_3 quad 0 quad dots]', (FIX_LEFT, .75), 36, max_width=6),
              text('Position-only observations', (FIX_LEFT, -.3), 24, FUSED, max_width=6),
              text('Other states are informed through P.', (FIX_LEFT, -1.15), 21, MUTED, max_width=6),
              text('Reference rate-dependent noise scaling is separate from whitening.', (0, -2.2), 21, GNSS))
    await stage.say('This two-dimensional calculation isolates the update mathematics. It is not a new ECEF filter or a replay of the recorded drive.', hold=FIX_READ_HOLD)
    await stage.finish('Change coordinates consistently. Preserve correlation, condition each residual, and keep uncertainty distinct from truth.')


async def lesson_update(scene):
    example = PositionFixExample()
    result = compare_position_fix(example)
    stage = Stage(scene, 10, 'One complete GNSS update', 'How do correlated position errors become correct scalar updates?')
    await _fix_innovation(stage)
    await _fix_correlated_noise(stage, example)
    await _fix_whitening_geometry(stage, example, result)
    await _fix_transform_model(stage, result)
    await _fix_scalar_conditioning(stage, result)
    await _fix_batch_equivalence(stage, example, result)
    await _fix_wrong_noise(stage, result)
    await _fix_innovation_distance(stage, result)
    await _fix_reference_bridge(stage)
