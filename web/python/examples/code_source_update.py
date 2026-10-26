"""Paired native/direct-WASM example: one source edit with stable page layout."""
from noon import *


DECLARATION = "__shared__ float tile[TILE_DIM][TILE_DIM];"
PADDED_DECLARATION = "__shared__ float tile[TILE_DIM][TILE_DIM + 1];"
SOURCE = "\n".join([
    DECLARATION,
    "int x = blockIdx.x * TILE_DIM + threadIdx.x;",
    "int y = blockIdx.y * TILE_DIM + threadIdx.y;",
    "for (int j = 0; j < TILE_DIM; j += BLOCK_ROWS) {",
    "  if (x < cols && y + j < rows) {",
    "    tile[threadIdx.y + j][threadIdx.x] =",
    "        in[static_cast<size_t>(y + j) * cols + x];",
    "  }",
    "}",
    "__syncthreads();",
])


def listing():
    code = Code(
        code_string=SOURCE,
        reserved_source=SOURCE.replace(DECLARATION, PADDED_DECLARATION, 1),
        language="cuda", formatter_style="monokai", background="window",
        syntax_colors={"foreground": WHITE, "comment": GRAY,
                       "keyword": PURPLE, "number": GOLD},
        token_colors={"in": TEAL, "tile": TEAL, "__syncthreads": PINK,
                      "TILE_DIM": GOLD, "BLOCK_ROWS": GOLD,
                      "threadIdx.x": GOLD, "threadIdx.y": GOLD,
                      "blockIdx.x": GOLD, "blockIdx.y": GOLD},
        paragraph_config={"font_size": 20},
        line_number_config={"font_size": 11, "color": GRAY, "min_digits": 2},
    ).scale(1.2).rotate(0.04)
    code.highlight_lines(0)
    return code


class CodeSourceBefore(Scene):
    def construct(self):
        self.add(listing())


class CodeSourceAfter(Scene):
    def construct(self):
        code = listing()
        code.replace_line(0, PADDED_DECLARATION)
        self.add(code)


class CodeSourceUpdate(Scene):
    def construct(self):
        code = listing()
        self.add(code)
        self.wait(0.2)
        previous, current = code.replace_line(0, PADDED_DECLARATION)
        self.add(previous)
        self.play(FadeOut(previous), Write(current), run_time=0.75, rate_func=linear)
        self.wait(0.2)
