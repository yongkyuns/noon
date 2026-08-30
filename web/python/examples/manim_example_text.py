from noon import *


class Example1Text(Scene):
    def construct(self):
        text = Text('Hello world').scale(3)
        self.add(text)
