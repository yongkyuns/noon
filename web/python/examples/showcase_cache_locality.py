"""Follow one cache line, then read the same matrix in two loop orders."""
from noon import *

RIGHT_ORDER = "column"  # Try "row" to give both panels the same traversal.

SIDE = 4
WORD = 0.76
PAPER = color_from_hex("#f0ece2")
CARD = color_from_hex("#fffdf7")
INK = color_from_hex("#182d33")
MUTED = color_from_hex("#4e6062")
EDGE = color_from_hex("#aabcb5")
HIT = color_from_hex("#006657")
MISS = color_from_hex("#9a3f23")
LINE_COLORS = tuple(color_from_hex(hex_color) for hex_color in (
    "#c8e6dc", "#d7e0f6", "#f6debb", "#e9d8ed",
))


def cache_reads(order):
    """Track the line and cumulative misses in a one-slot, row-major cache."""
    if order not in ("row", "column"):
        raise ValueError('Use "row" or "column" for RIGHT_ORDER.')
    resident = None
    misses = 0
    reads = []
    for outer in range(SIDE):
        for inner in range(SIDE):
            row, column = (outer, inner) if order == "row" else (inner, outer)
            line = row  # Our aligned rows are exactly one cache line wide.
            misses += line != resident
            resident = line
            reads.append((row, column, line, misses))
    return reads


def label(text, size, x, y, color=INK):
    return Text(text, font_size=size, color=color).move_to((x, y, 0))


def box(width, height, x, y, fill=CARD, stroke=EDGE, stroke_width=1.4):
    return Rectangle(width=width, height=height, fill=fill, fill_opacity=1,
                     stroke=stroke, stroke_width=stroke_width).move_to((x, y, 0))


def word_x(x, column, word):
    return x + (column - (SIDE - 1) / 2) * word


def line_words(line, x, y, word, size):
    tiles = []
    for column in range(SIDE):
        cx = word_x(x, column, word)
        tiles.extend([
            box(word - 0.04, word - 0.08, cx, y, LINE_COLORS[line], stroke=None),
            label(str(line * SIDE + column), size, cx, y),
        ])
    return VGroup(*tiles)


def selection(word, x, y, color=INK):
    return Rectangle(width=word - 0.04, height=word - 0.08, fill_opacity=0,
                     stroke=color, stroke_width=3).move_to((x, y, 0))


class Traversal:
    def __init__(self, x, order):
        self.reads = cache_reads(order)
        self.word = 0.59
        self.grid_x = x - 1.59
        self.slot_x = x + 1.23
        self.slot_y = -1.04
        self.resident = None
        self.result = None
        self.last_miss = None
        self.misses = 0
        self.cursor = selection(self.word, *self.position(0, 0))
        self.cursor.set_stroke(opacity=0)
        self.counter = Integer(0, font_size=46, color=INK)
        self.counter.move_to((self.slot_x, -2.18, 0))
        title = "By row" if order == "row" else "By column"
        # Show the first four addresses so viewers can predict which lines they need.
        addresses = " → ".join(str(row * SIDE + col) for row, col, _, _ in self.reads[:SIDE])
        self.objects = [
            box(6.58, 4.45, x, -1.01),
            label(title, 35, x, 0.68),
            label(addresses, 24, x, 0.17, MUTED),
            label("Cache · 1 slot", 24, self.slot_x, -0.47, MUTED),
            box(SIDE * self.word + 0.14, self.word + 0.1, self.slot_x, self.slot_y),
            self.counter, label("misses", 24, self.slot_x, -2.76, MUTED),
        ]
        for row in range(SIDE):
            self.objects.append(
                line_words(row, self.grid_x, self.position(row, 0)[1], self.word, 23)
            )
        self.objects.append(self.cursor)

    def position(self, row, column):
        return word_x(self.grid_x, column, self.word), -0.38 - row * self.word

    def approach(self, index):
        row, column, _, _ = self.reads[index]
        old_row, old_column = self.reads[index - 1][:2] if index else (row, column)
        cursor = self.cursor.animate.set_stroke(INK, width=2, opacity=0.85)
        if abs(row - old_row) + abs(column - old_column) > 1:
            return cursor.set_stroke(opacity=0)
        return cursor.move_to((*self.position(row, column), 0))

    def retire(self, index):
        row, column, _, misses = self.reads[index]
        miss = misses != self.misses
        self.cursor.move_to((*self.position(row, column), 0))
        animations = [self.cursor.animate.set_stroke(INK, width=2, opacity=1)]
        if miss and self.resident is not None:
            animations.append(FadeOut(self.resident))
        if miss != self.last_miss and self.result is not None:
            animations.append(FadeOut(self.result))
        return animations

    def resolve(self, index):
        _, _, line, misses = self.reads[index]
        miss = misses != self.misses
        color = MISS if miss else HIT
        animations = [self.cursor.animate.set_stroke(color, width=3, opacity=1)]
        if miss:
            self.resident = line_words(line, self.slot_x, self.slot_y, self.word, 23)
            animations.append(FadeIn(self.resident))
            self.counter.set_value(misses).move_to((self.slot_x, -2.18, 0))
        if miss != self.last_miss:
            self.result = label("MISS" if miss else "HIT", 25, self.slot_x, -1.61, color)
            animations.append(FadeIn(self.result))
            self.last_miss = miss
        self.misses = misses
        return animations


class CacheLocality(Scene):
    def construct(self):
        strip_x = [(line - 1.5) * (SIDE * WORD + 0.20) for line in range(SIDE)]
        grid_y = [1.58 - line * WORD for line in range(SIDE)]
        strip_y = 1.27
        slot_y = -1.36
        rows = [line_words(line, 0, grid_y[line], WORD, 28) for line in range(SIDE)]
        title = label("Which loop fetches more from memory?", 42, 0, 2.95)
        across = Arrow((-1.48, 2.15, 0), (1.48, 2.15, 0), buff=0, color=INK)
        down = Arrow((-1.87, 1.89, 0), (-1.87, -1.02, 0), buff=0, color=INK)
        hook_labels = [label("By row", 31, 2.50, 2.15), label("By column", 31, -3.35, 0.44)]
        same_reads = label("Same matrix · 16 reads", 30, 0, -1.75, MUTED)
        self.add(box(16, 9, 0, 0, PAPER, stroke=None), title, *rows)
        self.next_section("Same reads, different order")
        self.play(GrowArrow(across), Write(hook_labels[0]), run_time=0.6)
        self.play(GrowArrow(down), Write(hook_labels[1]), FadeIn(same_reads), run_time=0.6)
        self.wait(1.5)

        self.next_section("How memory stores the matrix")
        layout_title = label("Rows sit end to end in memory", 42, 0, 2.95)
        self.play(FadeOut(title), FadeOut(across), FadeOut(down),
                  FadeOut(same_reads), *(FadeOut(item) for item in hook_labels), run_time=0.4)
        self.play(
            FadeIn(layout_title),
            *(row.animate.shift((x, 0, 0)) for row, x in zip(rows, strip_x)),
            run_time=1,
        )
        memory = label("Memory · 4 adjacent values per line", 30, 0, 2.08, MUTED)
        cache = label("Cache · 1 slot", 29, 3.52, slot_y, MUTED)
        slot = box(SIDE * WORD + 0.16, WORD + 0.12, 0, slot_y)
        caption = label("The cache keeps a copy close to the CPU", 29, 0, -2.91, MUTED)
        self.play(
            *(row.animate.shift((0, strip_y - y, 0)) for row, y in zip(rows, grid_y)),
            FadeIn(memory), FadeIn(cache), FadeIn(slot), FadeIn(caption), run_time=0.8,
        )
        self.wait(1.4)

        self.next_section("What one read fetches")
        read_title = label("Read address 0", 42, 0, 2.95)
        requested = selection(WORD, word_x(strip_x[0], 0, WORD), strip_y)
        self.play(FadeOut(layout_title), FadeOut(caption), run_time=0.3)
        self.play(Write(read_title), FadeIn(requested), run_time=0.6)

        first_line = line_words(0, strip_x[0], strip_y, WORD, 28)
        miss = label("MISS · fetch addresses 0–3", 34, 0, -2.91, MISS)
        self.add(first_line)
        self.bring_to_front(requested)
        self.play(
            first_line.animate.shift((-strip_x[0], slot_y - strip_y, 0)),
            FadeIn(miss), run_time=1.1,
        )
        self.wait(1.5)

        next_read = label("Read address 1", 42, 0, 2.95)
        self.play(FadeOut(read_title), FadeOut(miss), run_time=0.3)
        self.play(
            FadeIn(next_read),
            requested.animate.move_to((word_x(strip_x[0], 1, WORD), strip_y, 0)),
            run_time=0.5,
        )
        retained = selection(WORD, word_x(0, 1, WORD), slot_y, HIT)
        hit = label("HIT · address 1 is already here", 34, 0, -2.91, HIT)
        self.play(
            FadeIn(hit), FadeIn(retained), requested.animate.set_stroke(HIT),
            run_time=0.6,
        )
        self.wait(1.5)

        jump_read = label("Read address 4", 42, 0, 2.95)
        self.play(FadeOut(next_read), FadeOut(hit), FadeOut(requested), run_time=0.3)
        requested = selection(WORD, word_x(strip_x[1], 0, WORD), strip_y)
        self.play(Write(jump_read), FadeIn(requested), run_time=0.6)
        second_line = line_words(1, strip_x[1], strip_y, WORD, 28)
        replace = label("MISS · addresses 4–7 replace 0–3", 34, 0, -2.91, MISS)
        self.add(second_line)
        self.bring_to_front(requested)
        self.play(
            second_line.animate.shift((-strip_x[1], slot_y - strip_y, 0)),
            FadeOut(first_line), FadeOut(retained), FadeIn(replace),
            run_time=1.1,
        )
        self.wait(1.6)

        self.next_section("Compare both loop orders")
        left = Traversal(-3.50, "row")
        right = Traversal(3.50, RIGHT_ORDER)
        compare = label("Which loop reuses its cache line?", 42, 0, 2.95)
        self.play(
            *(row.animate.shift((0, 0.65, 0)) for row in rows),
            *(FadeOut(item) for item in [jump_read, replace, memory, cache, slot, requested, second_line]),
            run_time=0.7,
        )
        self.play(FadeIn(compare), *(FadeIn(item) for item in left.objects + right.objects),
                  run_time=0.8)
        self.wait(2)
        for index in range(len(left.reads)):
            # Keep the first divergence readable, then accelerate the repeated reads.
            step = max(0.6, 1.25 - 0.15 * max(0, index - 3))
            self.play(left.approach(index), right.approach(index), run_time=step * 0.45)
            self.play(*left.retire(index), *right.retire(index), run_time=step * 0.18)
            self.play(*left.resolve(index), *right.resolve(index), run_time=step * 0.37)
        same_order = left.misses == right.misses
        result_title = (f"{left.misses} misses in each loop" if same_order
                        else f"{left.misses} vs {right.misses} cache misses")
        takeaway = ("Both loops get four reads from each line." if same_order
                    else "Rows finish a line. Columns jump between lines.")
        self.play(FadeOut(compare), run_time=0.3)
        self.play(Write(label(result_title, 42, 0, 2.95)), run_time=0.9)
        self.play(FadeIn(label(takeaway, 32, 0, -3.65)), run_time=0.5)
        self.wait(3.8)
