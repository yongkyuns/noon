"""Residual, whitening, gain, Joseph form and injection, in that order."""
from visuals import Stage,text,equation,notes,FUSED,GNSS,MUTED


async def lesson_update(scene):
    stage=Stage(scene,10,'One complete GNSS update','What happens between receiving a fix and publishing a corrected solution?')
    rows=[(r'r = z - h(hat(x)^-)','Innovation: measurement minus prediction'),
          (r'S = H P^- H^T + R','Innovation covariance'),
          (r'K = P^- H^T S^(-1)','Gain distributes the innovation'),
          (r'delta hat(x) = K r','Estimated local error')]
    for i,(formula,explanation) in enumerate(rows):
        y=1.65-i*.95
        await stage.reveal(equation(formula,(-3.1,y),32,max_width=6.1),
                           text(explanation,(3.35,y),21,MUTED,max_width=5.9),hold=4)
    await stage.say('The reference uses receiver position in ECEF. It does not fuse raw satellite ranges here.',hold=7)
    await stage.clear()
    stage.add(equation(r'P^+ = (I-K H) P^- (I-K H)^T + K R K^T',(0,1.35),37,max_width=12.5),
              text('Joseph form retains both covariance contributions.',(0,.25),25,FUSED),
              equation(r'R = L L^T quad r_w = L^(-1) r quad H_w = L^(-1) H',(0,-.9),34,max_width=12.5))
    await stage.say('Whitening decorrelates observations before scalar updates. Apply the same transform to residual and H.',hold=8)
    await stage.finish('Gate the innovation, correct the nominal state, and reset the local error consistently.')
