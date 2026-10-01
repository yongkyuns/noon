"""One stable target takes callback-local geometry, transform, and paint."""

from noon import BLUE, Path, Rectangle, Scene, VectorPath, always_redraw


class OrdinaryAlwaysRedraw(Scene):
    async def construct(self):
        path_samples = 0

        def produce_path():
            nonlocal path_samples
            path_samples += 1
            width = 0.3 + min(path_samples, 20) * 0.015
            return Path(
                VectorPath()
                .move_to((-width, -0.25))
                .cubic_to((-width, 0.25), (width, 0.25), (width, -0.25)),
                color=BLUE,
                stroke_width=2.0,
            ).shift((0.0, -0.7, 0.0))

        path = always_redraw(produce_path)
        rectangle = Rectangle(width=0.4, height=0.35).shift((0.5, 0.25, 0.0))
        rectangle.set_fill(BLUE, opacity=0.8)
        self.add(rectangle)
        self.add(path)
        path_identity = path.id
        await self.wait(0.25)
        assert path_samples >= 2
        assert path.id == path_identity
