"""Follow a moving point while its path remains the stable reference."""
from math import sin
from noon import *


class CameraFollowsPath(MovingCameraScene):
    async def construct(self):
        camera_frame = self.camera.frame
        camera_frame.save_state()

        axes = Axes(
            x_range=[0, 9, 1.5], y_range=[-1.5, 1.5, 0.5],
            x_length=10, y_length=3.6, color=GRAY,
        )
        path = axes.plot(lambda x: 0.9 * sin(0.85 * x), [0, 9], color=BLUE)
        start_point = axes.i2gp(path.t_min, path)
        end_point = axes.i2gp(path.t_max, path)
        start_marker = Dot(start_point, radius=0.11, color=TEAL)
        end_marker = Dot(end_point, radius=0.11, color=RED)
        moving_dot = Dot(start_point, radius=0.09, color=YELLOW)
        start_label = Text("start", font_size=17, color=TEAL).move_to(start_point + 0.34 * DOWN)
        end_label = Text("finish", font_size=17, color=RED).move_to(end_point + 0.34 * DOWN)

        title = Text("A camera that follows the path", font_size=31).shift(3.2 * UP)
        caption = Text("The curve and axes stay fixed as the view moves.", font_size=19, color=GRAY).shift(3.0 * DOWN)
        resolved = Text("The full path stays available as a reference.", font_size=22).shift(3.0 * DOWN)

        await self.play(FadeIn(title), FadeIn(caption), run_time=0.6)
        await self.play(Create(axes), run_time=0.8, rate_func=smooth)
        await self.play(
            Create(path), FadeIn(start_marker), FadeIn(end_marker), FadeIn(moving_dot),
            FadeIn(start_label), FadeIn(end_label), run_time=1.2, rate_func=smooth,
        )
        await self.wait(0.4)

        await self.play(
            FadeOut(title), FadeOut(caption),
            camera_frame.animate.scale(0.62).move_to(moving_dot),
            run_time=0.7, rate_func=smooth,
        )

        def follow_point(frame):
            frame.move_to(moving_dot.get_center())

        camera_frame.add_updater(follow_point)
        await self.play(MoveAlongPath(moving_dot, path, rate_func=linear), run_time=3.2)
        camera_frame.remove_updater(follow_point)
        await self.wait(0.4)

        await self.play(Restore(camera_frame), run_time=1.0, rate_func=smooth)
        await self.play(FadeIn(resolved), run_time=0.5)
        await self.wait(1.0)
