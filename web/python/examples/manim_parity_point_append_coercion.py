"""The original CE caller/XYZ/hook controls through real authored/live Rust paths."""
import noon as m
import numpy as np


class PointAppendCoercionControls(m.Scene):
    async def construct(self):
        assert len(POINT_APPEND_CASES) == 84
        assert len(POINT_APPEND_HOOK_CASES) == 28
        for phase in ("authored", "live"):
            for constructor, cases in ((m.VMobject, POINT_APPEND_CASES), (m.VGroup, POINT_APPEND_FAMILY_CASES)):
                for expected in cases:
                    actual = observe_point_append(m, expected["spec"], expected["method"], expected["prefixRows"], constructor)
                    for field in ("spec", "method", "prefixRows", "output", "numPoints", "inputUnchanged"):
                        assert actual[field] == expected[field], (phase, field, expected["method"], expected["spec"]["id"], actual[field], expected[field])
                    np.testing.assert_allclose(actual["pointsXY"], expected["pointsXY"], rtol=1e-12, atol=1e-12, err_msg=f"{phase}: {expected['method']}: {expected['spec']['id']}")
                    np.testing.assert_allclose(actual["pointsXYZ"], expected["pointsXYZ"], rtol=1e-12, atol=1e-12, err_msg=f"XYZ {phase}: {expected['method']}: {expected['spec']['id']}")
            for expected in POINT_APPEND_HOOK_CASES:
                actual = observe_point_append_hook(m, expected["spec"], expected["method"])
                assert actual == expected, (phase, expected["method"], expected["spec"]["id"], actual, expected)
            mapped = m.VMobject().set_points([[1., 2., .25], [3., 4., .75]])
            snapshot = mapped.get_all_points()
            mapped.apply_over_attr_arrays(lambda points: points + [5., -2., 1.])
            np.testing.assert_array_equal(mapped.get_all_points(), [[6., 0., 1.25], [8., 2., 1.75]])
            np.testing.assert_array_equal(snapshot, [[1., 2., .25], [3., 4., .75]])
            resized = m.VMobject().set_points([[1., 2., .25], [3., 4., .75]])
            resized.resize_points(3)
            np.testing.assert_array_equal(resized.get_all_points(), [[1., 2., .25], [3., 4., .75], [1., 2., .25]])
            await self.wait(.25)
