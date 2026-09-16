"""Sensor meaning and frame conventions, introduced before mechanization."""
from math import pi,cos,sin
from visuals import (Stage,Plot,text,equation,notes,arrow,car,rotating_frame,
                     TRUTH,GNSS,FUSED,INS,MUTED,Rotate,Transform,linear)


async def lesson_sensors(scene):
    stage=Stage(scene,3,'What the IMU measures','Why does a stationary accelerometer not read zero?')
    centre=(-3.1,-.1)
    stage.add(car(centre),arrow((-3.1,-.4),(-3.1,-1.75),GNSS),
              arrow((-3.8,-.1),(-3.8,1.25),FUSED),
              text('gravity',(-2.3,-1.3),21,GNSS),
              text('specific force',(-4.35,1.75),21,FUSED,max_width=3.8),
              *notes(['Side-view illustration','At rest: acceleration = 0','Specific force balances gravity.']))
    await stage.say('The accelerometer measures specific force: acceleration relative to free fall.',hold=8)
    await stage.clear()
    stage.add(equation(r'f^n = a^n - g^n',(-3.1,.6),48,max_width=6),
              equation(r'a^n = C_s^n f^s + g^n',(3.2,.6),35,max_width=6),
              text('Specific-force definition',(-3.1,-.7),24,FUSED),
              text('Navigation-frame acceleration',(3.2,-.7),24,GNSS),
              text('Local teaching model: Earth rotation is omitted here.',(0,-2),22,MUTED))
    await stage.say('A gyroscope measures angular rate. It does not directly report an orientation.',hold=7)
    await stage.finish('Rotate specific force into the navigation frame, then add the gravity model.')


async def lesson_frames(scene):
    stage=Stage(scene,4,'One vector, different coordinates','Which way does the sensor point relative to the vehicle and Earth?')
    centre=(-3.1,0)
    world=rotating_frame(centre,color=MUTED)
    sensor=rotating_frame(centre,color=FUSED)
    stage.add(world,sensor,car(centre),
              text('Planar yaw projection',(-3.1,2.02),20,MUTED),
              *notes(['Grey: navigation axes','Green: sensor axes','The physical vector is unchanged.']))
    await stage.say('A rotation changes a vector’s coordinates, not the physical vector itself.',hold=6)
    force=arrow(centre,(centre[0]+1.7,centre[1]+.8),GNSS)
    stage.add(force)
    await scene.play(Rotate(sensor,angle=pi/3),run_time=3)
    await stage.say('The same arrow has different components in the grey and green frames.',hold=6)
    await stage.clear()
    stage.add(*notes(['s: sensor axes','c: car axes','n: local North–East–Down','e: Earth-centred, Earth-fixed'],start_y=1.6),
              equation(r'v^c = C_s^c (C_s^e)^T v^e',(-3.05,.8),38,max_width=6.1),
              text('The superscript is the destination frame.',(-3.05,-.5),20,MUTED,max_width=6.2),
              text('Transpose reverses an orthonormal rotation.',(-3.05,-1.3),20,MUTED,max_width=6.2))
    await stage.finish('The reference propagates in ECEF; the tutorial draws local projections for readability.')
