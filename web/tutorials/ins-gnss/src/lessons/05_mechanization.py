"""Separate physical propagation, numerical integration, and Earth corrections."""
from visuals import Stage,text,equation,arrow,notes,FUSED,GNSS,MUTED


async def lesson_mechanization(scene):
    stage=Stage(scene,5,'From measurements to motion','What happens at every IMU sample?')
    columns=(-4.5,0,4.5)
    labels=('Angular rate','Velocity','Position')
    for x,label in zip(columns,labels):
        stage.add(text(label,(x,1.65),27,FUSED,max_width=4),
                  text('integrate',(x,-1.55),21,MUTED))
    stage.add(text('Local teaching model: Earth-rate terms omitted.',(0,-2.2),20,MUTED),
              arrow((-3,1.65),(-1.5,1.65),MUTED),arrow((1.5,1.65),(3,1.65),MUTED))
    await stage.reveal(equation(r'dot(q) = 1/2 q ⊗ (0,omega)',(-4.5,.25),30,max_width=4))
    await stage.say('Gyroscope integration updates attitude. The quaternion product is not scalar multiplication.',hold=7)
    await stage.reveal(equation(r'dot(v) = C f + g',(0,.25),34,max_width=4))
    await stage.reveal(equation(r'dot(p) = v',(4.5,.25),40,max_width=4))
    await stage.say('Attitude decides how measured specific force projects into velocity and then position.',hold=6)
    await stage.clear()
    stage.add(equation(r'v_(k+1) = v_k + a_k Delta t',(0,1.55),38,max_width=12),
              equation(r'p_(k+1) = p_k + v_k Delta t + 1/2 a_k Delta t^2',(0,.25),38,max_width=12),
              text('Exact for constant acceleration over the interval.',(0,-.8),24,MUTED),
              text('ECEF adds Earth-rate terms; the reference uses second-order RK.',(0,-1.7),23,GNSS))
    await stage.finish('Sensor sample time is part of the mathematics. Animation frame time must not drive the filter.')
