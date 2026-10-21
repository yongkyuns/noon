"""The browser harness injects unchanged shared observations and pinned cases."""
import noon as m
import numpy as np


class PathKeywordControls(m.Scene):
    async def construct(self):
        assert len(PATH_KEYWORD_CASES) == 5
        for phase in ("authored", "live"):
            for expected in PATH_KEYWORD_CASES:
                actual = observe_path_keyword(m, expected["method"])
                for field in ("method", "keywords", "returnsSelf"):
                    assert actual[field] == expected[field], (phase, field, actual, expected)
                for field in ("pointsXY", "centerXY"):
                    np.testing.assert_allclose(actual[field], expected[field], rtol=1e-12, atol=1e-12,
                                               err_msg=f"{phase}: {expected['method']} {field}")
            await self.wait(.25)
