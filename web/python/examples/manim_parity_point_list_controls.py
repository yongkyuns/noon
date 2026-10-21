"""Same-platform public values, including the real Rust-backed tuple query."""
import noon as m
import numpy as np


class PointListInsertionControls(m.Scene):
    async def construct(self):
        assert len(POINT_LIST_CASES) == 135
        oracle = pinned_point_list_oracle(POINT_LIST_ORACLE)
        for phase in ("authored", "live"):
            for desktop_expected in POINT_LIST_CASES:
                spec = desktop_expected["spec"]
                expected = observe_point_list(oracle, spec)
                actual = observe_point_list(m, spec)
                assert_point_list_observation(actual, expected, phase + ": " + str(spec))
                # The original desktop expectations remain exact on a matching
                # integer platform. wasm32 is checked against the same CE code,
                # rather than casting caller values or editing that fixture.
                if np.dtype(int).name == "int64":
                    assert_point_list_observation(expected, desktop_expected, "pinned desktop")
            await self.wait(.25)
