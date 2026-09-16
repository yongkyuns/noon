"""Paired sensor realizations isolate what the innovation gate changes."""
from dataclasses import replace
from math import sqrt
from model import Experiment,simulate,display_points
from visuals import Stage,Plot,text,equation,notes,FUSED,INS,GNSS,MUTED


async def lesson_robustness(scene):
    config=Experiment(outlier_metres=35)
    gated=simulate(config)
    ungated=simulate(replace(config,gate_enabled=False))
    event=gated[round(config.outlier_time*config.imu_hz)]
    stage=Stage(scene,13,'When a measurement is wrong','Will the filter accept a 35-metre GNSS outlier?')
    plot=Plot((80,105),(-5,12),'Simulation time / s','Position error / m',
              xticks=(80,85,90,95,100,105),yticks=(0,5,10))
    select=lambda rows:[s for s in rows if 80<=s.time<=105]
    stage.add(*plot.objects,plot.curve(display_points(select(gated),'error'),FUSED),
              plot.curve(display_points(select(ungated),'error'),INS),
              text('Gated',(-4.8,-2.90),20,FUSED),text('Ungated',(-2.2,-2.90),20,INS),
              equation(r'"NIS" = r^2 / S',(3.58,1.50),40),
              *notes([f'Outlier innovation: {event.innovation:.1f} m',
                      f'NIS: {event.innovation**2/event.innovation_variance:.1f}',
                      f'Threshold: {config.gate_sigma**2:.0f}',
                      'Decision: rejected'],start_y=.45,gap=.65))
    await stage.say('Both filters receive identical sensor noise. Only the gate setting differs.',hold=8)
    await stage.say('For this scalar Gaussian test, a 3-sigma threshold is NIS = 9. Higher dimensions use different thresholds.',hold=8)
    await stage.finish('Gating catches gross inconsistencies; it cannot rescue an incorrect model or every plausible outlier.')
