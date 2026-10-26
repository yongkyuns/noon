"""Paired with Noon's native/direct-WASM logical Code row example."""
from noon import *


SOURCE = "__global__ void copy(float *out) {\n    int x = threadIdx.x;\n\n    out[x] = x;\n}"


class CodeLineHighlights(Scene):
    def construct(self):
        code = Code(
            code_string=SOURCE,
            language="cuda",
            formatter_style="monokai",
            background="window",
            line_numbers_from=9,
            paragraph_config={"font_size": 20},
        ).scale(1.2).rotate(0.08)
        code.highlight_lines(2)
        code.highlight_lines(3, 5, include_line_numbers=True)
        self.add(code)
