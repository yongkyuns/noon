"""Introduce F and Q by physical dependencies before exposing dense matrices."""
from visuals import Stage,text,equation,notes,arrow,FUSED,GNSS,INS,MUTED


async def lesson_prediction(scene):
    stage=Stage(scene,9,'Predict the uncertainty too','How do sensor errors reach velocity and position?')
    positions=[(-4.8,.7),(-1.6,.7),(1.6,.7),(4.8,.7)]
    labels=['gyro error','attitude error','velocity error','position error']
    for p,label in zip(positions,labels):stage.add(text(label,p,20,max_width=2.35))
    for left,right in zip(positions,positions[1:]):
        stage.add(arrow((left[0]+1.25,left[1]),(right[0]-1.25,right[1]),FUSED))
    stage.add(text('Accelerometer bias also enters velocity error directly.',(0,-.8),25,GNSS))
    await stage.say('F is a local sensitivity model: it says which errors cause other errors to change.',hold=8)
    await stage.clear()
    stage.add(equation(r'dot(delta x) = F delta x + G w',(-3,1.3),37,max_width=6),
              equation(r'P_(k+1)^- = Phi P_k^+ Phi^T + Q_d',(-3,-.25),36,max_width=6),
              *notes(['F: error dynamics','G: noise directions','Φ: one-step transition','Qd: accumulated process noise']))
    await stage.say('Propagation moves existing uncertainty through Φ and adds new process uncertainty through Qd.',hold=8)
    await stage.clear()
    stage.add(equation(r'Phi approx I + F Delta t',(0,1.45),42,max_width=12),
              equation(r'Q_d approx G Q_c G^T Delta t',(0,.1),42,max_width=12),
              text('These are the reference implementation’s first-order approximations.',(0,-1.25),24,MUTED))
    await stage.finish('Do not confuse per-sample noise variance with continuous-time spectral density.')
