from noon import *


class OrdinaryPathQueries(Scene):
    def construct(self):
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
        start, end = rectangle.get_start(), target.get_start()
        animation = self.declare_live_transform_to(rectangle, target, run_time=1, rate_func=linear)
        live = self.live_execution()
        finish = live.play(animation)
        live.advance_to(0.5)
        check_boundary()
        midpoint = rectangle.get_start()
        assert abs(midpoint[0] - (start[0] + end[0]) * 0.5) < 2e-6
        assert abs(midpoint[1] - (start[1] + end[1]) * 0.5) < 2e-6
        live.advance_to(finish)
        live.complete()
        wait_end = live.wait(0.2)
        live.advance_to(wait_end)
        live.complete()
        self.remove(ellipse)
        self.play(Create(ellipse), run_time=1, rate_func=linear)
        observed = self.live_execution()
        observed.evaluate(1.7)
        endpoint = ellipse.get_end()
        assert abs(endpoint[0] - 1.3) < 2e-6 and abs(endpoint[1]) < 2e-6
        observed.evaluate(2.2)
