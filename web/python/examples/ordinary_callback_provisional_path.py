"""Callback-local retained path construction through the shared resource scope.

The Path stays phase-local until `Scene.add` and the callback's one final
semantic/effective publication. Its vector payload is never installed in a store
or rendered before that commit succeeds.
"""

from noon import Circle, Color, Path, Scene, VectorPath


class OrdinaryCallbackProvisionalPath(Scene):
    async def construct(self):
        anchor = Circle(radius=0.2)
        self.add(anchor)
        created = []

        def add_path_once(mobject, _dt):
            if created:
                return
            path = (
                Path(
                    VectorPath()
                    .move_to((-0.5, 0.0))
                    .line_to((0.5, 0.0))
                    .line_to((0.0, 0.6))
                    .close(),
                    fill=Color(0.1, 0.8, 0.4),
                    stroke=None,
                )
                .shift((1.5, 0.5, 0.0))
                .set_fill(Color(0.2, 0.6, 1.0), opacity=0.7)
            )
            assert path.get_center() == (1.5, 0.8)
            self.add(path)
            assert self.mobjects == [anchor, path]
            created.append(path)

        anchor.add_updater(add_path_once)
        await self.wait(0.25)
        assert len(created) == 1
        assert created[0]._scene is self
        assert created[0].get_center() == (1.5, 0.8)
