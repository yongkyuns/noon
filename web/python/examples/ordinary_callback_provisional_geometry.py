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
                if len(created) == 2 and _manim_updaters._canonical_callback_time(mobject) == 0.25:
                    terminal = Circle(radius=0.1).shift((4.0, -1.0, 0.0))
                    self.add(terminal)
                    created.append(terminal)
                return
            assert _manim_updaters._canonical_callback_time(mobject) == 0.0
            candidate = (
                Circle(radius=0.25)
                .shift((2.0, -1.0, 0.0))
                .set_fill(Color(0.1, 0.8, 0.4), opacity=0.6)
            )
            assert candidate.get_center() == (2.0, -1.0)
            second = Circle(radius=0.15).shift((3.0, -1.0, 0.0))
            # Separate source-level adds share one pending callback collector;
            # their delayed wrapper bindings must receive distinct IDs.
            self.add(candidate)
            self.add(second)
            assert self.mobjects == [anchor, candidate, second]
            mobject.shift((0.5, 0.0, 0.0))
            created.extend([candidate, second])

        anchor.add_updater(construct_once)
        await self.wait(0.25)
        # The final phase also creates an object: its wrapper must finalize
        # before the endpoint returns the player lease at segment completion.
        assert len(created) == 3
        assert all(candidate._scene is self for candidate in created)
        assert created[0].get_center() == (2.0, -1.0)
        assert created[1].get_center() == (3.0, -1.0)
        assert created[2].get_center() == (4.0, -1.0)
        assert anchor.get_center() == (0.5, 0.0)
        assert self.mobjects == [anchor, *created]
