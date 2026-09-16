"""Cross-covariance is the bridge from a position fix to an unmeasured bias."""
from model import Experiment,simulate,display_points
from visuals import Stage,Plot,text,equation,notes,confidence_ellipse,arrow,FUSED,GNSS,INS,UNCERTAINTY,MUTED


async def lesson_covariance(scene):
    stage=Stage(scene,7,'Information travels through covariance','How can a position-only receiver help estimate an accelerometer bias?')
    stage.add(confidence_ellipse((-3,0),((1,-.72),(-.72,1)),scale_x=.9,scale_y=.65),
              arrow((-5.5,0),(-.5,0),MUTED),arrow((-3,-1.9),(-3,1.9),MUTED),
              text('position error',(-1.1,-.45),18,MUTED,max_width=2),
              text('bias error',(-3,2.1),20,MUTED),
              *notes(['Illustrative joint 95% ellipse','Negative correlation:','too much estimated bias','predicts too little motion.']))
    await stage.say('A covariance is more than three independent error bars. Cross-terms couple the estimates.',hold=8)
    await stage.clear()
    stage.add(equation(r'K_b = P_(b p)^- / (P_(p p)^- + R)',(-3,1.15),35,max_width=6.1),
              equation(r'hat(b)^+ = hat(b)^- + K_b r',(-3,-.2),35,max_width=6.1),
              *notes(['Position is directly measured.','Bias is not.','Prediction creates correlation;','the residual uses that correlation.']))
    await stage.say('The sign depends on the bias convention: this simulator subtracts physical bias from raw acceleration.',hold=8)
    await stage.clear()
    config=Experiment();samples=simulate(config)
    plot=Plot((0,120),(-.01,.06),'Simulation time / s','Estimated physical bias / (m/s²)',
              xticks=(0,30,60,90,120),yticks=(0,.02,.04,.06))
    stage.add(*plot.objects,plot.curve(display_points(samples,'bias'),FUSED),
              plot.curve([(0,config.bias),(120,config.bias)],GNSS,1.5),
              *notes(['Green: estimated bias','Gold: injected physical bias',f'Final: {samples[-1].bias:.5f} m/s²',
                      'No bias measurement was supplied.']))
    await stage.finish('GNSS informs bias indirectly through the motion model and cross-covariance.')
