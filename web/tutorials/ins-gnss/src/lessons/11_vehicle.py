"""A physical constraint is a fallible measurement model, not a hard truth."""
from visuals import Stage,text,equation,notes,car,arrow,TRUTH,FUSED,GNSS,INS,MUTED


async def lesson_vehicle(scene):
    stage=Stage(scene,11,'Let the vehicle help','What information is contained in “a car usually does not move sideways”?')
    centre=(-3.2,0)
    stage.add(car(centre),arrow(centre,(-.9,0),FUSED),arrow(centre,(-3.2,1.45),INS),
              text('forward',(-1.7,-.45),22,FUSED),text('lateral',(-4.05,1.50),22,INS),
              equation(r'v^c = C_s^c (C_s^e)^T v^e',(3.5,1.3),31),
              equation(r'v_y^c approx 0 quad v_z^c approx 0',(3.5,-.1),32))
    await stage.say('Rotate the estimate into the car frame. Lateral and vertical velocity should usually be small.',hold=8)
    await stage.clear()
    stage.add(text('A soft pseudo-measurement, not a clamp',(0,1.6),30,FUSED),
              equation(r'r_("NHC") = -mat(v_y^c; v_z^c)',(-3,.05),38,max_width=6),
              *notes(['Use nonzero measurement noise.','Reject inappropriate dynamics.','Sideslip and bumps can violate it.'],start_y=.8),
              text('The reference gates NHC using angular rate and acceleration magnitude.',(0,-1.7),23,MUTED))
    await stage.finish('A constraint helps only while its physical assumptions are valid. Zero uncertainty would be false confidence.')


async def lesson_calibration(scene):
    stage=Stage(scene,12,'Calibration and observability','Which unknowns can the available motion actually distinguish?')
    stage.add(equation(r'f_("corrected") = s_f ⊙ f_("raw") + b_f',(0,1.55),40,max_width=12),
              text('Reference convention: s starts near 1; b is an additive correction.',(0,.45),24,MUTED),
              text('Offset: additive   |   Scale: proportional   |   Mounting: rotates axes',(0,-.75),25,FUSED))
    await stage.say('These corrective offsets are not the physical raw-measurement bias used by the 1D simulator.',hold=8)
    await stage.clear()
    stage.add(*notes(['Stationary gravity constrains tilt.','Gravity alone does not give yaw.','Course can initialize heading','only with suitable vehicle motion.']),
              text('More parameters ≠ more information',(-3.2,1.35),26,GNSS,max_width=6.2),
              equation(r'delta y approx H delta x',(-3.2,.05),38,max_width=6.2),
              text('Different errors can produce similar residuals.',(-3.2,-1.2),22,MUTED,max_width=6.2))
    await stage.finish('Calibration requires informative motion; small estimated covariance alone does not prove observability.')
