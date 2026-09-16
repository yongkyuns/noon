"""One prescribed attitude correction, from geometry to acceleration and reset."""
from math import ceil, radians
from attitude import AttitudeExample, attitude_example
from visuals import (Stage, Plot, text, equation, notes, arrow, rotating_frame,
                     FUSED, GNSS, INS, TRUTH, MUTED, Rotate, linear)

ATTITUDE_READ_HOLD = 7.0
ATTITUDE_LEFT = -3.25
ATTITUDE_RIGHT = 3.30
ATTITUDE_ROTATION_SECONDS = 6.0


def attitude_numbers(vector, digits=3):
    return '(' + ', '.join(f'{x:+.{digits}f}' for x in vector) + ')'


def attitude_matrix_source(matrix):
    return 'mat(' + '; '.join(', '.join(f'{x:.3f}' for x in row) for row in matrix) + ')'


async def _attitude_dimensions(stage):
    stage.add(text('Nominal orientation', (ATTITUDE_LEFT, 1.65), 28, FUSED),
              text('Local rotation error', (ATTITUDE_RIGHT, 1.65), 28, GNSS),
              equation(r'q = (q_w,q_x,q_y,q_z)', (ATTITUDE_LEFT, .55), 34, max_width=6),
              equation(r'delta theta = (delta theta_x,delta theta_y,delta theta_z)',
                       (ATTITUDE_RIGHT, .55), 32, max_width=6),
              text('4 stored coefficients', (ATTITUDE_LEFT, -.6), 24, FUSED),
              text('3 independent coordinates / rad', (ATTITUDE_RIGHT, -.6), 23, GNSS))
    await stage.say('A quaternion stores orientation. The filter estimates a small rotation around that nominal orientation.', hold=ATTITUDE_READ_HOLD)
    await stage.reveal(equation(r'q_w^2 + q_x^2 + q_y^2 + q_z^2 = 1', (0, -1.75), 35, max_width=12), hold=4)
    await stage.say('The unit-length constraint removes one degree of freedom. Four coefficients do not mean four rotation errors.', hold=ATTITUDE_READ_HOLD)
    await stage.clear()


async def _attitude_rotation_vector(stage, result):
    stage.add(equation(r'phi = theta u', (ATTITUDE_LEFT, 1.5), 42),
              equation(r'Q(phi) = (cos(theta/2), u sin(theta/2))', (0, .3), 36, max_width=12),
              *notes(['θ: rotation angle in radians', 'u: unit rotation axis'],
                     start_y=1.65, gap=.56),
              text('Scalar first: Q maps a 3-vector into a unit quaternion.', (0, -1), 25, MUTED))
    await stage.say('A rotation vector carries an axis and an angle. The half-angle formula produces the four quaternion coefficients.', hold=ATTITUDE_READ_HOLD)
    await stage.reveal(text('Prescribed correction: '+attitude_numbers(result['correction_rad'])+' rad',
                           (0, -1.8), 24, GNSS, max_width=12),
                       text('Q(phi) = '+attitude_numbers(result['delta_q'],4), (0,-2.5), 23, FUSED, max_width=12), hold=5)
    await stage.say('For tiny angles, Q(phi) is approximately (1, phi/2). This example computes the exact unit quaternion.', hold=ATTITUDE_READ_HOLD)
    await stage.clear()


async def _attitude_injection(stage, config, result):
    origin = (ATTITUDE_LEFT, -.05)
    truth = rotating_frame((0,0), length=1.7, color=TRUTH).rotate(radians(config.true_pitch_deg)).shift(origin)
    estimate = rotating_frame((0,0), length=1.7, color=INS).rotate(radians(config.prior_pitch_deg)).shift(origin)
    stage.add(truth, estimate,
              text('North–Up side projection', (ATTITUDE_LEFT, 2.05), 21, MUTED),
              text('White: true frame   Red: estimated frame', (ATTITUDE_LEFT, -2.2), 19, MUTED, max_width=6.2),
              *notes([f'True pitch: {config.true_pitch_deg:g}°', f'Prior estimate: {config.prior_pitch_deg:g}°',
                      f'Prescribed correction: {config.correction_pitch_deg:+g}°',
                      'Sensor time is frozen.'], start_y=1.55, gap=.60),
              equation(r'q^+ = Q(phi^n) ⊗ q^-', (ATTITUDE_RIGHT, -1.45), 33, max_width=6))
    await stage.say('Illustrate an estimate correction, not vehicle motion. The true frame stays fixed while the red estimate rotates.', hold=ATTITUDE_READ_HOLD)
    await stage.scene.play(Rotate(estimate, angle=radians(config.correction_pitch_deg)),
                           run_time=ATTITUDE_ROTATION_SECONDS, rate_func=linear)
    post_pitch = config.prior_pitch_deg + config.correction_pitch_deg
    await stage.reveal(text(f'Corrected estimate: {post_pitch:g}°', (ATTITUDE_RIGHT, -.95), 25, FUSED), hold=5)
    await stage.say(f'True-minus-estimated pitch error changes from {result["prior_error_deg"][1]:g}° to {result["post_error_deg"][1]:.0f}°. It is smaller, not zero.', hold=ATTITUDE_READ_HOLD)
    await stage.clear()


async def _attitude_product_order(stage, config, result):
    stage.add(equation(r'Q(phi^n) ⊗ q', (ATTITUDE_LEFT, 1.65), 36),
              equation(r'q ⊗ Q(phi^s)', (ATTITUDE_RIGHT, 1.65), 36),
              text('Navigation-frame correction', (ATTITUDE_LEFT, 1), 22, FUSED),
              text('Sensor-frame correction', (ATTITUDE_RIGHT, 1), 22, INS))
    for origin, vector, color in (((-5.,0), result['order_left_vector_ned'], FUSED),
                                  ((1.6,0), result['order_right_vector_ned'], INS)):
        scale = 2.5
        stage.add(arrow(origin, (origin[0]+2.9, origin[1]), MUTED, 1.5),
                  arrow(origin, (origin[0], origin[1]+.75), MUTED, 1.5),
                  arrow(origin, (origin[0]+scale*vector[1], origin[1]-scale*vector[2]), color),
                  text('E', (origin[0]+3.04, -.28), 18, MUTED),
                  text('Up', (origin[0]-.36, .65), 18, MUTED))
    await stage.say(f'Order test: yaw {config.order_yaw_deg:g}°, then the same {config.order_correction_deg:g}° x-axis numbers on different sides. The axes differ.', hold=ATTITUDE_READ_HOLD)
    await stage.reveal(text('NED vector: '+attitude_numbers(result['order_left_vector_ned']),
                           (ATTITUDE_LEFT, -1.8), 20, FUSED, max_width=6),
                       text('NED vector: '+attitude_numbers(result['order_right_vector_ned']),
                           (ATTITUDE_RIGHT, -1.8), 20, INS, max_width=6),
                       equation(r'phi^s = (C_s^n)^T phi^n', (0, -2.45), 29, max_width=12), hold=5)
    await stage.say('Left and right forms agree only when the correction vector is expressed in the matching frame. Do not just swap factors.', hold=ATTITUDE_READ_HOLD)
    await stage.clear()


async def _attitude_specific_force(stage, config, result):
    origin = (ATTITUDE_LEFT, -.10)
    scale = .16  # Scene units per m/s²; shared by both force arrows.
    a = result['prior_acceleration_m_s2']
    # C f followed head-to-tail by +g; screen Up is minus NED Down.
    rotated_force = (a[0], a[2]-config.gravity_m_s2)
    force_end = (origin[0]+scale*rotated_force[0], origin[1]-scale*rotated_force[1])
    accel_end = (origin[0]+scale*a[0], origin[1]-scale*a[2])
    stage.add(arrow(origin, force_end, INS), arrow(force_end, accel_end, GNSS),
              text('Rotated force', (ATTITUDE_LEFT-1.85, .9), 21, INS, max_width=3),
              text('Add gravity', (ATTITUDE_LEFT+1.2, .9), 21, GNSS, max_width=3),
              text('Resting sensor; unchanged raw measurement', (0, 1.95), 26),
              equation(r'hat(a)^n = C(hat(q)) f^s + g^n', (ATTITUDE_RIGHT, 1.25), 32, max_width=6),
              text('Measured fˢ / (m/s²)', (ATTITUDE_RIGHT, .15), 22, MUTED),
              text(attitude_numbers(result['sensor_specific_force_m_s2']), (ATTITUDE_RIGHT, -.5), 24),
              text('Prior estimate · North–Up · shared scale', (ATTITUDE_LEFT, -1.85), 19, MUTED, max_width=6))
    await stage.say('At rest, correct attitude makes rotated specific force cancel gravity. Tilt leaves a false horizontal acceleration.', hold=ATTITUDE_READ_HOLD)
    await stage.reveal(text(f'Prior north acceleration: {a[0]:+.3f} m/s²', (0, -2.4), 24, INS), hold=5)
    await stage.say(f'After the prescribed correction, north acceleration is {result["post_acceleration_m_s2"][0]:+.3f} m/s². The IMU measurement did not change.', hold=ATTITUDE_READ_HOLD)
    await stage.clear()


async def _attitude_coasting(stage, config, result):
    end = config.coast_seconds
    bound = 10*ceil(max(abs(result[key]) for key in ('prior_coast_north_error_m','post_coast_north_error_m'))/10)
    plot = Plot((0,end), (-bound,0), 'Hypothetical elapsed time / s', 'Estimated minus true north position / m',
                xticks=(0,end/2,end), yticks=(-bound,-bound/2,0))
    stage.add(*plot.objects)
    for field, color in [('prior',INS), ('post',FUSED)]:
        acceleration = result[field+'_acceleration_m_s2'][0]
        stage.add(plot.curve([(end*i/80, .5*acceleration*(end*i/80)**2) for i in range(81)],color))
    stage.add(*notes(['Two constant-tilt thought experiments', 'Start with exact position and velocity.',
                      'Hold each attitude error fixed.', 'No new noise or aiding corrections.']),
              text('Prior tilt', (-4.55,-2.88), 20, INS), text('Corrected tilt', (-1.7,-2.88), 20, FUSED))
    await stage.say('These are hypothetical coasts, not output from the full INS. Holding tilt fixed isolates two integrations of acceleration error.', hold=ATTITUDE_READ_HOLD)
    cursor = plot.cursor(0); stage.add(cursor)
    await plot.move_cursor(stage.scene, cursor, 0, end, duration=8)
    await stage.say(f'At {end:g} seconds: {result["prior_coast_north_error_m"]:+.2f} m versus {result["post_coast_north_error_m"]:+.2f} m. Correcting attitude changes future drift.', hold=ATTITUDE_READ_HOLD)
    await stage.clear()


async def _attitude_reset(stage, result):
    stage.add(equation(r'Q(delta theta_("new")) = Q(delta theta_("old")) ⊗ Q(phi)^(-1)',
                       (0,1.55), 34, max_width=12.5),
              text('The physical orientation does not change during reset.', (0,.4), 25, MUTED),
              equation(r'delta hat(theta) ← 0 quad P_("new") = J P_("old") J^T', (0,-.75), 36, max_width=12))
    await stage.say('After injection, zero means no estimated correction around the NEW nominal. It does not mean the attitude is perfect.', hold=ATTITUDE_READ_HOLD)
    await stage.reveal(equation(r'J approx I + 1/2 [phi]_times', (0,-1.8), 35, max_width=12), hold=5)
    await stage.say('This plus sign is for the stated left error. The full filter also transforms attitude cross-covariances with other states.', hold=ATTITUDE_READ_HOLD)
    await stage.clear()
    stage.add(text('An attitude-only covariance reset', (0,1.95), 29),
              equation('P_("old") = '+attitude_matrix_source(result['reset_before_deg2']),
                       (ATTITUDE_LEFT,.75), 32, max_width=6.2),
              equation('P_("new") = '+attitude_matrix_source(result['reset_after_deg2']),
                       (ATTITUDE_RIGHT,.75), 32, max_width=6.2),
              text('Displayed in deg²; computed in rad²', (0,-.65), 24, MUTED),
              text('Numbers use the exact local reset Jacobian, not its first-order truncation.', (0,-1.8), 21, GNSS))
    await stage.say('Reset re-expresses the existing uncertainty. It is not a new observation and should not be described as extra information.', hold=ATTITUDE_READ_HOLD)
    await stage.clear()


async def _attitude_reference_bridge(stage, result):
    stage.add(text('Propagate → estimate error → inject → reset', (0,1.65), 29, FUSED),
              text('Corrected quaternion: '+attitude_numbers(result['posterior_q'],4), (0,.4), 26),
              text('Prescribed correction, not an attitude-filter performance result.', (0,-.75), 23, GNSS),
              text('Reference ECEF equations require their own frame/sign audit.', (0,-1.8), 23, MUTED))
    await stage.say('This exact quaternion example clarifies the convention; it is not a literal reproduction of the upstream small-Euler injection.', hold=ATTITUDE_READ_HOLD)
    await stage.finish('Attitude is a rotation, its error is local, and resetting that error must preserve the same physical orientation.')


async def lesson_error_state(scene):
    config = AttitudeExample()
    result = attitude_example(config)
    stage = Stage(scene,8,'Attitude correction and reset',
                  'Why use three error coordinates around a four-coefficient quaternion?')
    await _attitude_dimensions(stage)
    await _attitude_rotation_vector(stage,result)
    await _attitude_injection(stage,config,result)
    await _attitude_product_order(stage,config,result)
    await _attitude_specific_force(stage,config,result)
    await _attitude_coasting(stage,config,result)
    await _attitude_reset(stage,result)
    await _attitude_reference_bridge(stage,result)
