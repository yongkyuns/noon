"""Signed bars, retained names, axis numbers, and value labels."""
from manim import *


class BarChartExample(Scene):
    def construct(self):
        chart = BarChart(
            [-2, 0, 3, -1, 2], bar_names=["A", "B", "C", "D", "E"],
            y_range=[-4, 4, 1], x_length=8, y_length=5,
        )
        self.add(chart, chart.get_bar_labels())
        self.wait(0.2)
