"""The harness injects the same public observer and original CE expectations."""
import noon as m
import numpy as np


class ScalarPathPrecisionControls(m.Scene):
    async def construct(self):
        assert len(SCALAR_PATH_PRECISION_CASES) == 10
        for phase in ('authored', 'live'):
            for expected in SCALAR_PATH_PRECISION_CASES:
                actual = observe_scalar_path_precision(m, expected['method'])
                assert actual['method'] == expected['method']
                assert actual['returnsSelf'] is expected['returnsSelf']
                np.testing.assert_allclose(actual['pointsXY'], expected['pointsXY'],
                                           rtol=1e-12, atol=1e-12,
                                           err_msg=phase + ': ' + expected['method'])
            assert len(MUTABLE_STRIDE_CLOSING_CASES) == 2
            for expected in MUTABLE_STRIDE_CLOSING_CASES:
                actual = observe_mutable_stride_closing(m, expected['pointCount'])
                for field in ('pointCount', 'stride', 'returnsSelf'):
                    assert actual[field] == expected[field]
                for field in ('samples', 'inputPointsXY', 'lastSubpathStartXY', 'pointsXY'):
                    np.testing.assert_allclose(actual[field], expected[field],
                                               rtol=1e-12, atol=1e-12,
                                               err_msg=phase + ': closing stride: ' + field)
            await self.wait(.25)
