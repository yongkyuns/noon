"""One Python updater replaces effective geometry without authored revisions."""

from noon import BLUE, Circle, Scene


class OrdinaryEffectiveCircleUpdater(Scene):
    async def construct(self):
        pulse = Circle(radius=0.35).set_fill(BLUE, opacity=1.0)
        self.add(pulse)
        samples = 0

        def redraw(mobject, _dt):
            nonlocal samples
            samples += 1
            mobject.set_effective_circle(0.35 + min(samples, 20) * 0.025)

        pulse.add_updater(redraw)
        await self.wait(0.25)
        assert samples >= 2
