"""A complete scalar correction with all numbers derived from one example."""
from math import exp,sqrt,pi
from visuals import Stage,Plot,text,equation,INS,GNSS,FUSED,Transform,linear


async def lesson_scalar(scene):
    stage=Stage(scene,6,'How much should we trust a fix?','Combine a prior and a measurement without choosing one blindly.')
    prior,prior_variance,measurement,measurement_variance=10.,9.,14.,4.
    gain=prior_variance/(prior_variance+measurement_variance)
    posterior=prior+gain*(measurement-prior)
    posterior_variance=(1-gain)*prior_variance
    plot=Plot((0,24),(0,.27),'Position / m','Probability density / (1/m)',
              xticks=(0,6,12,18,24),yticks=(0,.1,.2))
    def gaussian(mean,variance,color):
        return plot.curve([(x,exp(-.5*(x-mean)**2/variance)/sqrt(2*pi*variance))
                           for x in (i/10 for i in range(241))],color)
    stage.add(*plot.objects,gaussian(prior,prior_variance,INS),gaussian(measurement,measurement_variance,GNSS),
              text('Prior',(-4.7,-2.90),20,INS),text('Measurement',(-2.3,-2.90),20,GNSS))
    await stage.reveal(equation(r'K = P^- /(P^- + R)',(3.58,1.45)),
                       text(f'K = {prior_variance:g}/({prior_variance:g}+{measurement_variance:g}) = {gain:.3f}',(3.58,.60),24))
    await stage.say('The narrower measurement distribution receives more weight.',hold=6)
    await stage.reveal(gaussian(posterior,posterior_variance,FUSED),
                       equation(r'hat(p)^+ = hat(p)^- + K(z-hat(p)^-)',(3.58,-.30),28),
                       text(f'Posterior = {posterior:.3f} m',(3.58,-1.2),24,FUSED),
                       text(f'Standard deviation = {sqrt(posterior_variance):.3f} m',(3.58,-1.90),21,FUSED))
    await stage.say('This result assumes independent Gaussian errors and a correct measurement model.',hold=7)
    await stage.finish('A larger measurement variance R reduces its weight; variance is not standard deviation.')
