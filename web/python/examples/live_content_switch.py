"""Paired Python proof for shared live Geometry ↔ Text replacement."""

from noon import Circle, Scene, Square, Text, WHITE


class LiveContentSwitch(Scene):
    def construct(self):
        target = Circle(0.75).set_fill(WHITE, opacity=1.0)
        unaffected = Square(1.0).set_color("#0000ff")
        replacement_text = Text("one runtime")
        replacement_geometry = Circle(1.5)
        self.add(target, unaffected)

        self.live_execution()
        self.set_translation(target, 2.0, -1.0)
        self.replace_content(target, replacement_text)
        assert self.effective_translation(target) == (2.0, -1.0)
        assert self.effective_translation(unaffected) == (0.0, 0.0)

        self.replace_content(target, replacement_geometry)
        assert self.effective_translation(target) == (2.0, -1.0)
        assert self.effective_translation(unaffected) == (0.0, 0.0)

        # Content replacement preserves presentation. Finish with the same mixed
        # geometry/text frame as the Rust example, using text's point scale.
        self.replace_content(target, replacement_text)
        self.set_translation(target, -2.0, 1.0)
        self.set_scale(target, 1.0 / 96.0, 1.0 / 96.0)
