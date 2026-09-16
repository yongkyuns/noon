"""The opening experiment: evidence first, then the question we will explain."""
from model import Experiment, simulate, metrics, display_points
from visuals import (Stage, Plot, text, equation, notes, car, LAYOUT,
                     TRUTH, INS, FUSED, GNSS, MUTED, Transform, linear)


async def lesson_fusion(scene):
    config = Experiment()
    samples = simulate(config)
    result = metrics(samples,config)
    stage = Stage(scene,1,'Two sensors, one estimate','Why does a tiny acceleration error become a large position error?')
    plot = Plot((0,config.duration),(-10,150),'Simulation time / s','Position error / m',
                xticks=(0,30,60,90,120),yticks=(0,50,100,150))
    # Recorded-run overview: lines intentionally show the full experiment.
    stride = config.imu_hz // 2
    inertial = plot.curve(display_points(samples,"inertial_error",stride),INS)
    fused = plot.curve(display_points(samples,"error",stride),FUSED)
    zero = plot.curve([(0,0),(config.duration,0)],TRUTH,1.5)
    stage.add(*plot.objects,zero,inertial,fused,
              text('IMU only',(-4.6,-2.90),20,INS),
              text('Fused',(-2.0,-2.90),20,FUSED))
    stage.add(*notes(['Synthetic, truth-known run',f'IMU: {config.imu_hz} Hz  |  GNSS: {config.gnss_hz} Hz',
                      f'Acceleration bias: {config.bias:.2f} m/s²',
                      f'GNSS unavailable: {config.outage_start:g}–{config.outage_end:g} s']))
    await stage.say('Start with the result. The red curve integrates an uncorrected sensor bias.',hold=6)
    cursor = plot.cursor(0)
    stage.add(cursor)
    await plot.move_cursor(scene, cursor, 0, config.outage_start, duration=6)
    await stage.say('Now GNSS disappears. The filter predicts, but cannot obtain new position fixes.',hold=5)
    await plot.move_cursor(scene, cursor, config.outage_start, config.outage_end, duration=6)
    await stage.say(f'Just before GNSS returns: {result["inertial_error_before_reacquisition_m"]:.1f} m IMU error, '
                    f'{result["filter_error_before_reacquisition_m"]:.1f} m fused error.',hold=7)
    await plot.move_cursor(scene, cursor, config.outage_end, config.duration, duration=6)
    await stage.clear()
    stage.add(car((-4.6,0),TRUTH),car((-2.8,0),INS),car((-4.4,-1),FUSED),
              text('Physical vehicle',(-4.55,.8),21,TRUTH),
              text('Estimated positions are beliefs, not extra cars.',(0,-2.1),24,MUTED),
              *notes(['A position fix can also', 'teach us velocity and bias.', 'How does information travel',
                      'between these quantities?']))
    await stage.finish('The answer is not averaging tracks: it is a model of how errors evolve together.')
