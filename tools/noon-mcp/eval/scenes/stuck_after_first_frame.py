from noon import *


class StuckAfterFirstFrame(Scene):
    def construct(self):
        self.add(Circle())
        self.wait(0.1)
        while True:
            pass
