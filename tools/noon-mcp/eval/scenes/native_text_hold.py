from noon import *


class NativeTextHold(Scene):
    def construct(self):
        self.add(Text("Hello world").scale(3))
        self.wait(1)
