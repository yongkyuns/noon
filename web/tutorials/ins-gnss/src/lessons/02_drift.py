"""An analytic limit case separates error growth from random noise."""
from math import sin, radians
from visuals import Stage, Plot, text, equation, notes, INS, FUSED, GNSS, Transform, linear


async def lesson_drift(scene):
    stage = Stage(scene,2,'Small errors accumulate','What does a constant acceleration error do after two integrations?')
    bias, horizon = .02, 60.0
    plot = Plot((0,horizon),(0,40),'Elapsed time / s','Position error / m',
                xticks=(0,20,40,60),yticks=(0,10,20,30,40))
    trace=[(i*horizon/120,.5*bias*(i*horizon/120)**2) for i in range(121)]
    stage.add(*plot.objects,plot.curve(trace,INS))
    await stage.reveal(equation(r'delta a = b_a',(3.58,1.55)))
    await stage.say('Assume perfect initial position and velocity; the only error is a constant bias.',hold=6)
    await stage.reveal(equation(r'delta v(t) = b_a t',(3.58,.30)))
    await stage.reveal(equation(r'delta p(t) = 1/2 b_a t^2',(3.58,-1.0)))
    cursor = plot.cursor(0)
    stage.add(cursor)
    await scene.play(Transform(cursor,plot.cursor(horizon)),run_time=8,rate_func=linear)
    await stage.say(f'{bias:.2f} m/s² × {horizon:g} s × {horizon:g} s / 2 = {.5*bias*horizon*horizon:.0f} m.',hold=7)
    await stage.clear()
    gravity,tilt=9.81,.5
    leakage=gravity*sin(radians(tilt))
    stage.add(text('A second route to acceleration error',(0,1.8),29),
              equation(r'delta a approx g theta',(0,.65),42,max_width=10),
              text(f'{tilt:g}° tilt → {leakage:.4f} m/s² gravity leakage',(0,-.45),27,GNSS),
              text(f'About {.5*leakage*horizon*horizon:.0f} m after {horizon:g} s in this simplified model.',(0,-1.50),25,INS))
    await stage.finish('Attitude accuracy matters because a small tilt projects gravity onto a horizontal axis.')
