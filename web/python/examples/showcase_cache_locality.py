"""Compare loop orders in a model with one cache slot and four values per line."""
from noon import *

RIGHT_ORDER = "column"  # Try "row" to give both panels the same traversal.

SIDE = 4
WORD = 0.57
PAPER = color_from_hex("#f0ece2")
CARD = color_from_hex("#fffdf7")
INK = color_from_hex("#182d33")
MUTED = color_from_hex("#4e6062")
EDGE = color_from_hex("#c4cfca")
HIT = color_from_hex("#006c61")
MISS = color_from_hex("#a84421")
LOADED = color_from_hex("#d5e9e2")


def cache_reads(order):
    """Return each read's coordinates, resident cache line, and cumulative misses."""
    if order not in ("row", "column"):
        raise ValueError('Use "row" or "column" for RIGHT_ORDER.')
    resident = None
    misses = 0
    reads = []
    for outer in range(SIDE):
        for inner in range(SIDE):
            row, column = (outer, inner) if order == "row" else (inner, outer)
            address = row * SIDE + column
            line = address // SIDE
            misses += line != resident
            resident = line
            reads.append((row, column, line, misses))
    return reads


def label(text, size, x, y, color=INK):
    return Text(text, font_size=size, color=color).move_to((x, y, 0))


class CacheLocality(Scene):
    class TraversalPanel:
        def __init__(self, scene, x, order):
            self.scene = scene
            self.reads = cache_reads(order)
            self.misses = 0
            self.last_miss = None
            outer, inner = ("r", "c") if order == "row" else ("c", "r")
            code = Code(
                code_string=f"for {outer} in range({SIDE}):\n    for {inner} in range({SIDE}):\n        read(a[r][c])",
                language="python", formatter_style="emacs", background="rectangle",
                add_line_numbers=False, paragraph_config={"font_size": 24},
            ).move_to((x, 0.92, 0))
            read_line = code.code_lines[2]
            self.read_highlight = SurroundingRectangle(read_line, color=HIT, buff=0.11, stroke_width=2)
            self.read_highlight.set_stroke(opacity=0)

            scene.add(
                Rectangle(width=6.83, height=5.73, fill=CARD, fill_opacity=1,
                          stroke=EDGE, stroke_width=1.5).move_to((x, -0.24, 0)),
                label("Row first" if order == "row" else "Column first", 35, x, 2.19),
                code, self.read_highlight,
                label("Addresses", 24, x - 1.50, -0.23, MUTED),
                label("Misses", 24, x + 1.68, -0.23, MUTED),
            )
            self.count_position = (x + 1.68, -1.28, 0)
            self.count = Integer(0, font_size=68, color=INK).move_to(self.count_position)
            self.lines = [-1.52 - (row - 1.5) * WORD for row in range(SIDE)]
            self.columns = [x - 1.50 + (column - 1.5) * WORD for column in range(SIDE)]
            self.rows = []
            for y in self.lines:
                line = Rectangle(width=SIDE * WORD, height=WORD - 0.06,
                                 fill=CARD, fill_opacity=1, stroke=EDGE, stroke_width=1)
                self.rows.append(line.move_to((x - 1.50, y, 0)))
            self.cursor = Rectangle(width=WORD - 0.05, height=WORD - 0.06,
                                    fill=MUTED, fill_opacity=0, stroke=MUTED, stroke_width=2)
            self.cursor.set_stroke(opacity=0).move_to(self.position(0, 0))
            self.result_position = (x + 1.68, -2.31)
            self.result = label("Empty cache", 22, *self.result_position, MUTED)
            scene.add(*self.rows, self.cursor, self.count, self.result)
            for row, y in enumerate(self.lines):
                for column, cx in enumerate(self.columns):
                    scene.add(label(str(row * SIDE + column), 22, cx, y))

        def position(self, row, column):
            return (self.columns[column], self.lines[row], 0)

        def approach(self, index):
            row, column, _, _ = self.reads[index]
            last_row, last_column = self.reads[index - 1][:2] if index else (row, column)
            cursor = self.cursor.animate.set_fill(MUTED, opacity=0)
            if abs(row - last_row) + abs(column - last_column) > 1:
                # A loop wrap skips cells, so hide the cursor during the jump.
                cursor.set_stroke(opacity=0)
            else:
                cursor.move_to(self.position(row, column)).set_stroke(MUTED, opacity=0.6)
            return [cursor, self.read_highlight.animate.set_stroke(opacity=0.25)]

        def read(self, index):
            row, column, line, misses = self.reads[index]
            miss = misses != self.misses
            color = MISS if miss else HIT
            # The result starts only after both cursors reach their destinations.
            self.cursor.move_to(self.position(row, column))
            animations = [
                self.cursor.animate.set_fill(color, opacity=0.38)
                .set_stroke(color, width=2.5, opacity=1),
                self.read_highlight.animate.set_stroke(color, width=2.5, opacity=1),
            ]
            if miss:
                if index:
                    previous = self.reads[index - 1][2]
                    animations.append(self.rows[previous].animate.set_fill(CARD).set_stroke(EDGE, width=1))
                animations.append(self.rows[line].animate.set_fill(LOADED).set_stroke(HIT, width=2))
                self.count.set_value(misses).move_to(self.count_position)
            if miss != self.last_miss:
                self.scene.remove(self.result)
                self.result = label("MISS" if miss else "HIT", 25, *self.result_position, color)
                animations.append(FadeIn(self.result))
                self.last_miss = miss
            self.misses = misses
            return animations

        def settle(self):
            return [
                self.cursor.animate.set_fill(opacity=0.24).set_stroke(width=1.6),
                self.read_highlight.animate.set_stroke(width=1.5, opacity=0.72),
            ]

    def construct(self):
        self.add(
            Rectangle(width=16, height=9, fill=PAPER, fill_opacity=1, stroke=None),
            label("Cache misses", 37, 0, 3.28),
            label("1 cache slot · 4 values per line", 24, 0, 2.82, MUTED),
        )
        left = self.TraversalPanel(self, -3.58, "row")
        right = self.TraversalPanel(self, 3.58, RIGHT_ORDER)
        self.wait(0.8)
        for index in range(len(left.reads)):
            self.play(*left.approach(index), *right.approach(index), run_time=0.3, rate_func=smooth)
            self.play(*left.read(index), *right.read(index), run_time=0.4, rate_func=smooth)
            self.play(*left.settle(), *right.settle(), run_time=0.2, rate_func=smooth)
        takeaway = ("Same order, same number of misses."
                    if left.misses == right.misses
                    else "Row-first reuses each line before replacing it.")
        self.play(FadeIn(label(takeaway, 25, 0, -3.65)), run_time=0.6)
        self.wait(2.2)
