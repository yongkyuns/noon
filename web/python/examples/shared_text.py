"""Typed Python counterpart to noon-native's shared_text example.

Both objects are ordinary members of one Rust semantic Scene. Native Text is
shaped into that same store and the browser boundary transfers only the mixed
resource bundle and retained execution deltas to the renderer.
"""

from noon import Circle, RIGHT, LEFT, Scene, Text


class SharedText(Scene):
    def construct(self):
        circle = Circle(radius=0.5).shift(LEFT * 2)
        label = Text("Noon", font_size=48).shift(RIGHT)
        self.add(circle, label)
