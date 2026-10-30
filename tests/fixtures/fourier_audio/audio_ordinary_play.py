from noon import *

class AudioWithOrdinaryPlay(Scene):
    def construct(self):
        self.audio(
            AudioSource.small_discovery(),
            duration=3.0,
            gain=0.5,
            fade_in=0.5,
            fade_out=0.5,
        )
        self.play(Create(Circle(radius=0.5)), run_time=1.0)
        self.wait(2.0)
