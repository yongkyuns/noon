"""Later-slice callback-local analytic construction over the shared transaction.

A Circle stays phase-local while chained affine/style edits and its center read
operate on the pending semantic declaration. `Scene.add(anchor, created)` then
uses the collector's ordered prepared membership view; Python receives the
ordinary handle only after the callback's one semantic/effective publication.
"""

import _manim_updaters
from noon import Circle, Color, Scene


class OrdinaryCallbackProvisionalGeometry(Scene):
    async def construct(self):
        anchor = Circle(radius=0.3).set_fill(Color(0.2, 0.4, 1.0), opacity=1.0)
        self.add(anchor)
        created: list[Circle] = []

        def construct_once(mobject, _dt):
            if created:
                return
            assert _manim_updaters._canonical_callback_time(mobject) == 0.0
            candidate = (
                Circle(radius=0.25)
                .shift((2.0, -1.0, 0.0))
                .set_fill(Color(0.1, 0.8, 0.4), opacity=0.6)
            )
            assert candidate.get_center() == (2.0, -1.0)
            self.add(anchor, candidate)
            assert self.mobjects == [anchor, candidate]
            mobject.shift((0.5, 0.0, 0.0))
            created.append(candidate)

        anchor.add_updater(construct_once)
        await self.wait(0.25)
        assert len(created) == 1
        assert created[0]._scene is self
        assert created[0].get_center() == (2.0, -1.0)
        assert anchor.get_center() == (0.5, 0.0)
        assert self.mobjects == [anchor, created[0]]
