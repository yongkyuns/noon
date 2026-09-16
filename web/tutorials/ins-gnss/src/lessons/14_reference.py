"""Make the boundary between teaching simulation and full reference explicit."""
from visuals import Stage,text,equation,arrow,FUSED,GNSS,MUTED


async def lesson_reference(scene):
    stage=Stage(scene,14,'The complete reference filter','How do the ideas map to the 24-error-state implementation?')
    blocks=[('Position',3),('Velocity',3),('Attitude error',3),('Accel. offsets',3),
            ('Gyro offsets',3),('Accel. scales',3),('Gyro scales',3),('Mounting error',3)]
    for i,(name,count) in enumerate(blocks):
        column,row=i//4,i%4
        x=-3.25+6.5*column;y=1.6-row*.95
        await stage.reveal(text(f'{name}: {count}',(x,y),26,FUSED,max_width=6),hold=1.2)
    await stage.say('Eight three-dimensional blocks: 24 error coordinates. Two quaternions make 26 nominal coefficients.',hold=8)
    await stage.clear()
    stage.add(equation(r'"IMU" → "propagate" → "GNSS / NHC" → "correct"',(0,1.55),35,max_width=12),
              text('1D simulation: actual computed results, known truth, physical bias.',(0,.3),24,FUSED),
              text('Full reference: ECEF, 24 error states, calibration, vehicle constraints.',(0,-.75),23,GNSS),
              text('Recorded GNSS is a noisy observation—not independent ground truth.',(0,-1.8),23,MUTED))
    await stage.say('The simplified numerical example demonstrates the mechanism; it is not a validation of the full ECEF implementation.',hold=8)
    await stage.finish('Every result needs a model, a frame convention, a timestamp, and an honest account of uncertainty.')
