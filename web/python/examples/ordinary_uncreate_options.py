from noon import *


class OrdinaryUncreateOptions(Scene):
    def construct(self):
        first = Square(side_length=0.6, color=BLUE)
        kept = Circle(radius=0.25, color=PINK)
        forward = Square(side_length=0.4, color=GREEN)
        self.add(first, kept, forward)

        self.play(Uncreate(first), run_time=2.0, rate_func=rush_into)
        assert first not in self.mobjects
        assert kept in self.mobjects and forward in self.mobjects

        self.play(Uncreate(kept, remover=False), run_time=1.0)
        assert kept in self.mobjects
        assert forward in self.mobjects

        self.play(
            Uncreate(forward, reverse_rate_function=False),
            run_time=1.0,
        )
        assert forward not in self.mobjects
        assert self.mobjects == [kept]
