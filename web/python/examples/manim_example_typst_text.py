from noon import *


class TypstTextReferenceExample(Scene):
    def construct(self):
        text = Typst(
            r"*Hello* from _Typst!_",
            color=YELLOW,
            font_size=72,
        )
        self.add(text)
