from noon import *


class AgentTemporalTranslation(Scene):
    """Simple monotonic scene for preview/MCP temporal qualification."""

    def construct(self):
        marker = Square(side_length=1.0)
        marker.set_fill(BLUE, opacity=1.0)
        marker.set_stroke(BLUE, width=0)
        marker.shift(LEFT * 3)
        self.add(marker)
        self.play(marker.animate.shift(RIGHT * 6), run_time=3)
        self.wait(0.5)
