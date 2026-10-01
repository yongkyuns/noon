"""One stable target takes callback-local geometry, transform, and paint."""

from noon import BLUE, Rectangle, Scene, always_redraw


class OrdinaryAlwaysRedraw(Scene):
    async def construct(self):
        samples = 0

        def produce():
            nonlocal samples
            samples += 1
            return (
                Rectangle(width=0.4 + min(samples, 20) * 0.02, height=0.35)
                .shift((0.5, 0.25, 0.0))
                .set_fill(BLUE, opacity=0.8)
            )

        shape = always_redraw(produce)
        self.add(shape)
        identity = shape.id
        await self.wait(0.25)
        assert samples >= 2
        assert shape.id == identity
