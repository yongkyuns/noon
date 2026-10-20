from manim import *
import numpy as np


class OrdinaryPathQueries(Scene):
    def construct(self):
        precise = 1.0 + 2.0 ** -40
        for count in range(10):
            value = VMobject().set_points([(precise, 0.0, 0.0)] * count)
            assert value.width == 0.0
            if count != 1 and count % 4 != 0:
                for query in (value.get_anchors, value.get_center):
                    try:
                        query()
                    except ValueError:
                        pass
                    else:
                        raise AssertionError(("unmatched anchors", count))
            else:
                assert float(value.get_center()[0]) == (0.0 if count == 0 else precise)
                anchors = value.get_anchors()
                assert isinstance(anchors, np.ndarray) == (count == 1)
                assert len(anchors) == (1 if count == 1 else count // 2)

        rectangle = Rectangle(width=2, height=1, color=WHITE)
        rectangle.stretch_to_fit_width(4).rotate(0.25).shift(LEFT * 2.5)
        precise_points = [[float(p[0]), float(p[1]), 0.0]
                          for p in rectangle.get_all_points()]
        precise_points[0][0] += 2.0 ** -40
        rectangle.set_points(precise_points)
        assert [[float(p[0]), float(p[1])] for p in rectangle.get_all_points()] == [p[:2] for p in precise_points]
        assert float(rectangle.get_start()[0]) == precise_points[0][0]
        assert float(rectangle.point_from_proportion(0.0)[0]) == precise_points[0][0]

        ellipse = Circle(radius=0.8, color=WHITE)
        ellipse.stretch_to_fit_width(2.4).shift(RIGHT * 2.5)
        family = VGroup(VGroup(rectangle, ellipse), rectangle)

        def check_boundary():
            actual = [[float(p[0]), float(p[1])]
                      for p in family.get_points_defining_boundary()]
            expected = [[float(p[0]), float(p[1])]
                        for shape in (ellipse, rectangle) for p in shape.get_anchors()]
            assert actual == expected

        check_boundary()
        assert Mobject().get_num_points() == 0
        assert Mobject().has_no_points()
        assert VGroup(Circle()).get_num_points() == 0
        assert VGroup(Circle()).has_no_points()
        assert Line(LEFT, RIGHT).get_num_points() == 4
        assert ImageMobject([[0]]).get_num_points() == 4
        assert ImageMobject([[0]]).has_points()
        assert Text("A").get_num_points() == 0
        assert Text("A").has_no_points()
        for shape, expected_points in ((rectangle, 16), (ellipse, 32)):
            assert shape.get_num_points() == expected_points
            assert shape.has_points()
            assert not shape.has_no_points()
            self.add(shape)
            assert shape.get_arc_length() > 0
            pieces = shape.get_nth_curve_length_pieces(0)
            assert len(pieces) == 9
            assert abs(shape.get_nth_curve_length(0) - sum(pieces)) < 1e-10
            custom_pieces = shape.get_nth_curve_length_pieces(0, sample_points=4)
            assert len(custom_pieces) == 3
            assert abs(shape.get_nth_curve_length(0, sample_points=4) - sum(custom_pieces)) < 1e-10
            assert len(shape.get_nth_curve_length_pieces(0, sample_points=1)) == 0
            assert shape.get_nth_curve_length(0, sample_points=1) == 0
            for alpha in (0.125, 0.375, 0.625, 0.875):
                point = shape.point_from_proportion(alpha)
                assert abs(shape.proportion_from_point(point) - alpha) < 1e-5
                self.add(Dot(point, color="#ffcc44"))
        try:
            rectangle.proportion_from_point(ORIGIN)
        except ValueError:
            pass
        else:
            raise AssertionError("an off-path point must be rejected")
        loop = CubicBezier(ORIGIN, (1, 1, 0), (-1, 1, 0), ORIGIN)
        assert loop.proportion_from_point(ORIGIN) == 1.0
        endpoint_path = CubicBezier((-3, -1, 0), (-1, 2, 0), (1, -2, 0), (3, 1, 0)).rotate(0.1)
        assert endpoint_path.proportion_from_point(endpoint_path.get_end()) == 1.0
        target = rectangle.copy().stretch(1.2, 0)
        self.play(Transform(rectangle, target), run_time=1, rate_func=linear)
        check_boundary()
        self.wait(0.2)
        self.remove(ellipse)
        self.play(Create(ellipse), run_time=1, rate_func=linear)
