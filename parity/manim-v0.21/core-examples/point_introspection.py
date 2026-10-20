from manim import *


class PointIntrospection(Scene):
    def construct(self):
        rectangle = Rectangle(width=2, height=1, color=WHITE)
        circle = Circle(radius=0.8, color=WHITE)
        line = Line(LEFT, RIGHT)

        empty = Mobject()
        group = VGroup(Circle())
        image = ImageMobject([[0]])
        text = Text("A")

        assert empty.get_num_points() == 0
        assert not empty.has_points()
        assert empty.has_no_points()
        assert group.get_num_points() == 0
        assert not group.has_points()
        assert group.has_no_points()
        assert rectangle.get_num_points() == 16
        assert rectangle.has_points()
        assert not rectangle.has_no_points()
        assert circle.get_num_points() == 32
        assert circle.has_points()
        assert line.get_num_points() == 4
        assert image.get_num_points() == 4
        assert image.has_points()
        assert text.get_num_points() == 0
        assert text.has_no_points()

        self.add(rectangle, circle)
        self.wait(0.2)
