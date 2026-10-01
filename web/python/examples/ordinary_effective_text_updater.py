"""always_redraw selects prebuilt text through effective content."""

from noon import Scene, Text, always_redraw


class OrdinaryEffectiveTextUpdater(Scene):
    async def construct(self):
        first = Text("ready", font="DejaVu Sans Mono", font_size=36)
        second = Text("moving", font="DejaVu Sans Mono", font_size=36)
        samples = 0

        def produce():
            nonlocal samples
            samples += 1
            return first if samples % 2 else second

        label = always_redraw(produce)
        self.add(label)
        await self.wait(0.25)
        assert samples >= 2
