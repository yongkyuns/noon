"""Nominal coefficients are not the same thing as independent error coordinates."""
from visuals import Stage,text,equation,notes,arrow,FUSED,GNSS,MUTED


async def lesson_error_state(scene):
    stage=Stage(scene,8,'Nominal state versus error state','Why keep a quaternion but estimate a three-dimensional attitude error?')
    stage.add(text('Nominal navigation solution',(-3.25,1.5),27,FUSED,max_width=6.2),
              equation(r'hat(x) = (p,v,q,b_a,b_g)',(-3.25,.2),36,max_width=6.2),
              text('Small local correction',(3.3,1.5),27,GNSS,max_width=6),
              equation(r'delta x = (delta p,delta v,delta theta,delta b_a,delta b_g)',(3.3,.2),30,max_width=6.1),
              text('4 quaternion coefficients',(-3.25,-1.1),24,FUSED),
              text('3 local rotation coordinates',(3.3,-1.1),24,GNSS))
    await stage.say('A unit quaternion has four coefficients, but only three independent degrees of freedom.',hold=8)
    await stage.clear()
    stage.add(equation(r'hat(p)^+ = hat(p)^- + delta hat(p)',(0,1.55),38,max_width=12),
              equation(r'q^+ = delta q ⊗ q^-',(0,.35),42,max_width=12),
              text('Shown: a left, navigation-frame rotation error.',(0,-.60),25,MUTED),
              text('After injection, reset the error coordinates and transform covariance.',(0,-1.60),23,GNSS))
    await stage.say('Attitude correction is composition on rotations—not addition to four quaternion coefficients.',hold=8)
    await stage.finish('Choose the error convention first. Jacobian signs, injection order, and reset must agree.')
