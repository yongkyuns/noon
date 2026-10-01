"""One Python updater swaps prebuilt text as effective content."""

from noon import Scene, Text


class OrdinaryEffectiveTextUpdater(Scene):
    async def construct(self):
        label = Text("starting", font="DejaVu Sans Mono", font_size=36)
        first = Text("ready", font="DejaVu Sans Mono", font_size=36)
        second = Text("moving", font="DejaVu Sans Mono", font_size=36)
        self.add(label)
        samples = 0

        def redraw(mobject, _dt):
            nonlocal samples
            samples += 1
            mobject.set_effective_text(first if samples % 2 else second)

        label.add_updater(redraw)
        await self.wait(0.25)
        assert samples >= 2
