from noon import *


class BezierPaths(Scene):
    def construct(self):
        anchors = [4 * LEFT + 0.8 * DOWN, 4 * RIGHT + 0.8 * UP]
        controls = [2 * LEFT + 2.0 * UP, 2 * RIGHT + 2.0 * DOWN]
        bent_controls = [1.5 * LEFT + 1.7 * DOWN, 1.5 * RIGHT + 1.7 * UP]
        points = [anchors[0], *controls, anchors[1]]
        bent_points = [anchors[0], *bent_controls, anchors[1]]

        def curve(vertices):
            path = VMobject(stroke_color=BLUE, stroke_width=6, fill_opacity=0)
            path.start_new_path(vertices[0])
            path.add_cubic_bezier_curve_to(*vertices[1:])
            return path

        def guide(vertices):
            return VMobject(stroke_color=GRAY, stroke_width=1.5, fill_opacity=0).set_points_as_corners(vertices)

        path, control_polygon = curve(points), guide(points)
        dots = [Dot(point, radius=0.08, color=WHITE if i in (0, 3) else YELLOW) for i, point in enumerate(points)]
        title = Text("A curve from four points", font_size=30).shift(3.2 * UP)
        first = Text("White anchors fix the ends. Yellow handles set the tangents.", font_size=19).shift(3.0 * DOWN)
        second = Text("Move the handles; the curve bends continuously.", font_size=21).shift(3.0 * DOWN)
        final = Text("One vector path, ready to animate.", font_size=23).shift(3.0 * DOWN)

        self.play(FadeIn(title), FadeIn(first), run_time=0.6)
        self.play(Create(control_polygon), *[FadeIn(dot) for dot in dots], run_time=0.8)
        self.play(Create(path), run_time=2.0, rate_func=smooth)
        self.wait(0.7)
        self.play(FadeOut(first), FadeIn(second), run_time=0.5)
        self.play(
            Transform(path, curve(bent_points)),
            Transform(control_polygon, guide(bent_points)),
            dots[1].animate.move_to(bent_controls[0]),
            dots[2].animate.move_to(bent_controls[1]),
            run_time=2.4, rate_func=smooth,
        )
        self.wait(0.7)
        self.play(
            FadeOut(control_polygon), FadeOut(dots[1]), FadeOut(dots[2]),
            FadeOut(second), FadeIn(final), run_time=0.6,
        )
        self.wait(1.2)
