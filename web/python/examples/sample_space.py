"""Probability partitions built from shared Rust SampleSpace geometry."""
from noon import BLUE_E, GREEN_E, SampleSpace, Scene, YELLOW, color_from_hex


class SampleSpaceExample(Scene):
    def construct(self):
        horizontal = SampleSpace(width=2.8, height=1.8)
        horizontal.divide_horizontally([0.25, 0.5], colors=[GREEN_E, BLUE_E])
        horizontal.shift((-2.0, 0.0))

        vertical = SampleSpace(width=2.8, height=1.8)
        vertical.divide_vertically([0.4, 0.35], colors=[color_from_hex("#EC92AB"), YELLOW])
        vertical.shift((2.0, 0.0))

        assert horizontal.horizontal_parts is horizontal.submobjects[1]
        assert vertical.vertical_parts is vertical.submobjects[1]
        assert len(horizontal.horizontal_parts.submobjects) == 3
        assert len(vertical.vertical_parts.submobjects) == 3
        assert horizontal.complete_p_list([0.25, 0.5]) == [0.25, 0.5, 0.25]

        self.add(horizontal, vertical)
