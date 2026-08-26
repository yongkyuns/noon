from noon import *


class UsingIndicate(Scene):
    def construct(self):
        tex = Tex("Indicate").scale(3)
        self.play(Indicate(tex))
        self.wait()
