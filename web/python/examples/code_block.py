"""Paired with Noon's retained native/direct-WASM Code example."""
from noon import *


PYTHON_SOURCE = "def squares(values):\n    return [\n        x * x for x in values\n    ]"
CUDA_SOURCE = "__global__ void transpose(float *out) {\n    __shared__ float tile[8][9];\n    // Padding keeps the bank stride coprime.\n    out[threadIdx.x] = tile[0][threadIdx.x];\n}"


class CodeBlocks(Scene):
    def construct(self):
        title = Text("Code is part of the scene", font_size=32).move_to(UP * 3.25)
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
            paragraph_config={"font_size": 20},
        ).shift(DOWN * 1.5)
        self.add(title, python, cuda)
