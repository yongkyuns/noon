"""Syntax-highlighted Code blocks are retained scene families, not HTML overlays.

The only host-language work here is choosing text and timing. Noon's shared Rust
lexer, source layout, line numbers, opacity and renderer own the displayed code.
"""
from noon import *


PYTHON_SOURCE = "def squares(values):\n    return [\n        x * x for x in values\n    ]"
CUDA_SOURCE = (
    "__global__ void transpose(float *out) {\n"
    "    __shared__ float tile[8][9];\n"
    "    // Padding keeps the bank stride coprime.\n"
    "    out[threadIdx.x] = tile[0][threadIdx.x];\n"
    "}"
)


class NativeCodeBlocks(Scene):
    def construct(self):
        heading = Text("Code is part of the scene", font_size=32).move_to(UP * 3.25)
        python = Code(
            code_string=PYTHON_SOURCE,
            language="python",
            formatter_style="monokai",
            background="window",
            paragraph_config={"font_size": 20},
        ).shift(UP * 1.25)
        cuda = Code(
            code_string=CUDA_SOURCE,
            language="cuda",
            formatter_style="emacs",
            background="rectangle",
            paragraph_config={"font_size": 20},
        ).shift(DOWN * 1.5)

        self.play(FadeIn(heading), run_time=0.8)
        self.play(FadeIn(python), run_time=1.2)
        self.wait(0.5)
        self.play(FadeIn(cuda), run_time=1.2)
        self.wait(0.5)
        # Move both durable Code families without retokenizing their contents.
        self.play(
            python.animate.shift(0.22 * LEFT),
            cuda.animate.shift(0.22 * RIGHT),
            run_time=1.0, rate_func=smooth,
        )
        self.wait(0.8)
