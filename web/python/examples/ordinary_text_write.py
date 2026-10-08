from noon import *


class OrdinaryTextWrite(Scene):
    def construct(self):
        # Reuse an exact embedded face; glyph shaping and residency stay in Rust.
        font = NativeFontFace.bundled("DejaVu Sans Mono")
        moving = Text("MOVE", font=font).shift(2 * LEFT + DOWN)
        writing = Text("WRITE", font=font).shift(LEFT + UP)
        self.play(
            moving.animate.shift(2 * RIGHT),
            Write(writing),
            run_time=2,
            rate_func=linear,
        )
        assert abs(moving.get_center()[0]) < 1e-6
        self.play(Unwrite(writing), run_time=1, rate_func=linear)
        self.wait(0.25)
