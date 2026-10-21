"""The browser harness injects the shared observer and pinned CE cases."""
import noon as m


class ObjectMemberControls(m.Scene):
    async def construct(self):
        assert len(OBJECT_MEMBER_CASES) == 50
        for phase in ("authored", "live"):
            for case in OBJECT_MEMBER_CASES:
                observed = observe_object_membership(m, case["spec"])
                assert observed == case, (phase, case["spec"], observed, case)
            await self.wait(.25)
