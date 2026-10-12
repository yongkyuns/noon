"""Signed bars and labels authored through shared Rust semantics."""
from noon import *


class BarChartExample(Scene):
    async def construct(self):
        await prepare_latex()
        chart = BarChart(
            [-2, 0, 3, -1, 2], bar_names=["A", "B", "C", "D", "E"],
            y_range=[-4, 4, 1], x_length=8, y_length=5,
        )
        self.add(chart, chart.get_bar_labels())
