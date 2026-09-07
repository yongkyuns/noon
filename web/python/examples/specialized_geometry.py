"""Pair of the direct Rust specialized_geometry example."""

from noon import (
    AnnularSector, Annulus, DashedLine, Dot, Elbow, PI, RoundedRectangle,
    Scene, Sector, Triangle, Underline,
)


class SpecializedGeometry(Scene):
    def construct(self):
        rectangle = RoundedRectangle(width=2.0, height=1.0, corner_radius=0.2).shift((-4, 0, 0))
        objects = [
            Dot((-4, 2, 0), radius=0.3),
            Triangle().shift((0, 2, 0)),
            Elbow(width=0.8, angle=0.3).shift((4, 2, 0)),
            rectangle,
            AnnularSector(inner_radius=0.3, outer_radius=0.9, angle=PI, num_components=8),
            Sector(radius=0.9, angle=PI / 2, start_angle=PI / 4,
                   num_components=8, arc_center=(4, 0, 0)),
            Annulus(inner_radius=0.5, outer_radius=0.9,
                    num_components=8, arc_center=(-4, -2, 0)),
            DashedLine((-1, -2, 0), (1, -2, 0), dash_length=0.2, dashed_ratio=0.5),
            Underline(rectangle, buff=0.2).shift((8, -1.3, 0)),
        ]
        for object in objects:
            object.set_fill("#0000ff", opacity=0.35)
            object.set_stroke("#ffffff", width=2.0)
        self.add(*objects)
        self.wait(1.0)
