"""One extra column: a non-interactive CUDA shared-memory lesson.

Paste this whole file into the Noon Playground and Run. The only import change
needed for the ManimCE version is ``from noon import *`` -> ``from manim import *``.
No CUDA GPU, external assets, Python callbacks, or per-frame updaters are used.

Concept: ourarash/cuda-algorithm-lab, matrix_transpose/03_padded/
03_transpose_bank_conflicts_visualization.html and 03_transpose_padded.cu.
https://github.com/ourarash/cuda-algorithm-lab/tree/main/matrix_transpose/03_padded
Reference: https://developer.nvidia.com/blog/efficient-matrix-transpose-cuda-cc/

The first memory diagram is explicitly a TOY eight-bank, eight-lane model.
The final section uses 32 banks, 32 lanes, and distinct 32-bit word addresses.
Service rounds illustrate conflict serialization, NOT clock cycles or a measured
kernel speedup. Identical-address reads are broadcast (covered by model tests).
The CUDA snippets omit block-coordinate setup, loops, and boundary predicates;
the source kernel uses 32 x 8 threads and four elements per thread.
"""

from noon import *


BG = "#101722"
INK = "#EDF3FA"
MUTED = "#9BAABF"
EDGE = "#35445A"
ACCENT = "#63D5EE"
GOOD = "#7BE0AE"
WARN = "#FF837B"
BANK_COLORS = (
    "#FF837B", "#FFB66B", "#E8D777", "#7BE0AE",
    "#63D5EE", "#80A7FF", "#B89AF5", "#E99BD0",
)
CUDA_STRIDES = (32, 33, 34, 36, 48, 33)
ROUND_TIME = 0.32


def column_addresses(stride, banks, column=0):
    """One distinct 32-bit word per lane, reading one column of a full tile."""
    if any(isinstance(v, bool) or not isinstance(v, int)
           for v in (stride, banks, column)):
        raise TypeError("stride, banks, and column must be integers")
    if stride <= 0 or banks <= 0 or not 0 <= column < stride:
        raise ValueError("positive stride/bank count and 0 <= column < stride required")
    return tuple(lane * stride + column for lane in range(banks))


def read_service_batches(addresses, banks):
    """Schematic read rounds: one distinct word per bank per round.

    Lanes reading the same word share a broadcast; they are NOT a conflict.
    This is an educational address model, not a GPU timing simulator.
    """
    if isinstance(banks, bool) or not isinstance(banks, int) or banks <= 0:
        raise ValueError("banks must be a positive integer")
    per_bank = [{} for _ in range(banks)]
    for lane, address in enumerate(addresses):
        if isinstance(address, bool) or not isinstance(address, int) or address < 0:
            raise ValueError("word addresses must be non-negative integers")
        per_bank[address % banks].setdefault(address, []).append(lane)
    queues = [list(words.values()) for words in per_bank]
    count = max((len(queue) for queue in queues), default=0)
    return tuple(
        tuple(lane for queue in queues if step < len(queue) for lane in queue[step])
        for step in range(count)
    )


def bank_loads(stride, banks, column=0):
    """Distinct requested words per bank for the column-access experiment."""
    addresses = column_addresses(stride, banks, column)
    return tuple(len({a for a in addresses if a % banks == bank})
                 for bank in range(banks))


def text(value, x=0, y=0, size=22, color=INK):
    return Text(str(value), font="DejaVu Sans Mono", font_size=size,
                color=color).move_to((x, y, 0))


def rect(x, y, width, height, color=EDGE, opacity=0.18, stroke=1):
    return Rectangle(width=width, height=height, color=color,
                     stroke_width=stroke, fill_opacity=opacity).move_to((x, y, 0))


def matrix(n, x, y, step, numbers=False):
    cells, labels, centers = [], [], []
    for row in range(n):
        cell_row, center_row = [], []
        for col in range(n):
            p = (x + (col - (n - 1) / 2) * step,
                 y + ((n - 1) / 2 - row) * step, 0)
            cell_row.append(rect(p[0], p[1], step - 0.035, step - 0.035))
            center_row.append(p)
            if numbers:
                labels.append(text(row * n + col, p[0], p[1], 20))
        cells.append(cell_row)
        centers.append(center_row)
    return VGroup(*(cell for row in cells for cell in row), *labels), cells, labels, centers


class CudaTransposeBanks(Scene):
    """Three connected acts: transpose, contention, then a stride experiment."""

    def change_text(self, old, value, x=0, y=-3.4, size=20, color=INK, duration=0.3):
        new = text(value, x, y, size, color)
        self.play(FadeOut(old), FadeIn(new), run_time=duration)
        return new

    def wipe(self, *objects):
        self.play(*(FadeOut(obj) for obj in objects), run_time=0.5)

    def construct(self):
        # A normal semantic background rectangle; no renderer-specific settings.
        background = rect(0, 0, 14.23, 8.01, BG, 1, 0)
        self.add(background)
        self.transpose_intro()
        self.eight_bank_lesson()
        self.cuda_stride_experiment()

    def transpose_intro(self):
        title = text("ONE EXTRA COLUMN", y=3.38, size=38)
        subtitle = text("A tiny code edit. A different memory-access pattern.",
                        y=2.84, size=18, color=MUTED)
        self.play(Write(title), FadeIn(subtitle), run_time=1.1)
        left, cells_a, values, _ = matrix(4, -3.65, 0.45, 0.67, True)
        right, cells_b, _, pb = matrix(4, 3.65, 0.45, 0.67)
        labels = VGroup(text("INPUT", -3.65, 2.12, 18, ACCENT),
                        text("TRANSPOSE", 3.65, 2.12, 18, GOOD))
        arrow = Arrow((-1.65, 0.45, 0), (1.65, 0.45, 0), color=ACCENT)
        code = text("out[c][r] = in[r][c];", y=-1.64, size=26, color=ACCENT)
        caption = text("One input column becomes one output row.", y=-2.62, size=22)
        self.play(FadeIn(left), FadeIn(right), FadeIn(labels), Create(arrow),
                  Write(code), run_time=1)
        self.play(*(cells_a[r][0].animate.set_fill(ACCENT, opacity=0.45)
                    for r in range(4)), FadeIn(caption), run_time=0.5)
        transfers = [value.copy() for value in values]
        first_column = [transfers[r * 4] for r in range(4)]
        self.add(*first_column)
        self.play(*(transfers[r * 4].animate.move_to(pb[0][r]) for r in range(4)),
                  run_time=1.2)
        self.play(*(cells_b[0][r].animate.set_fill(GOOD, opacity=0.35)
                    for r in range(4)), run_time=0.3)
        self.wait(0.8)
        rest = [transfers[r * 4 + c] for r in range(4) for c in range(1, 4)]
        self.add(*rest)
        self.play(*(transfers[r * 4 + c].animate.move_to(pb[c][r])
                    for r in range(4) for c in range(1, 4)), run_time=0.8)
        caption = self.change_text(caption, "Same values. Different addresses.",
                                   y=-2.62, size=22, color=GOOD)
        self.wait(1)
        self.wipe(title, subtitle, left, right, labels, arrow, code, caption, *transfers)

    def eight_bank_lesson(self):
        title = text("WHY A COLUMN READ GETS STUCK", y=3.55, size=28)
        subtitle = text("Toy model: 8 lanes / 8 banks / 32-bit words. CUDA uses 32 banks.",
                        y=3.12, size=14, color=MUTED)
        tile, cells, _, positions = matrix(8, 3.55, 1.25, 0.42)
        data_labels = VGroup(*(text("a" + str(r), *positions[r][0][:2], 13)
                              for r in range(8)))
        tile_label = text("SHARED TILE  [8][8]", 3.55, 2.99, 15, ACCENT)
        code_y = (2.36, 1.88, 1.40, 0.92)
        code_strings = (
            "__shared__ float tile[8][8];",
            "tile[ty][tx] = input[...];",
            "__syncthreads();",
            "output[...] = tile[tx][ty];",
        )
        code_lines = [text(s, -3.43, y, 17, ACCENT if i == 0 else INK)
                      for i, (s, y) in enumerate(zip(code_strings, code_y))]
        focus = rect(-3.43, code_y[0], 6.12, 0.39, ACCENT, 0.12, 1)
        formula = text("word = r * stride + c", -2.86, 0.12, 19)
        bank_formula = text("bank = word % 8", -2.86, -0.35, 19, MUTED)
        bank_x = [-5.48 + 1.565 * i for i in range(8)]
        bank_boxes = [rect(x, -2.05, 1.26, 0.58, BANK_COLORS[i], 0.26, 2)
                      for i, x in enumerate(bank_x)]
        banks_group = VGroup(*bank_boxes,
                             *(text("bank " + str(i), x, -2.05, 14)
                               for i, x in enumerate(bank_x)))
        legend = text("Cell color = memory bank.  a0 ... a7 = unchanged logical values.",
                      y=-2.69, size=14, color=MUTED)
        caption = text("Write a row: neighboring lanes use neighboring banks.",
                       y=-3.35, size=19)
        self.play(FadeIn(title), FadeIn(subtitle), FadeIn(tile), FadeIn(tile_label),
                  FadeIn(banks_group), FadeIn(legend), run_time=0.8)
        self.add(focus)
        self.play(*(Write(line) for line in code_lines), FadeIn(formula),
                  FadeIn(bank_formula), FadeIn(caption), run_time=1)
        self.play(focus.animate.move_to((-3.43, code_y[1], 0)), run_time=0.3)
        # Deliberate pacing: inspect the first row slowly, fill later rows faster.
        for r in range(8):
            self.play(*(cells[r][c].animate.set_fill(BANK_COLORS[c], opacity=0.42)
                        for c in range(8)), run_time=0.65 if r == 0 else 0.12)
        self.play(FadeIn(data_labels), focus.animate.move_to((-3.43, code_y[2], 0)),
                  run_time=0.35)
        caption = self.change_text(caption, "Synchronize the block before reading another lane's data.",
                                   size=18)
        self.play(Indicate(code_lines[2], color=GOOD), run_time=0.65)
        self.wait(0.7)
        self.play(focus.animate.move_to((-3.43, code_y[3], 0)),
                  *(cells[r][0].animate.set_stroke(INK, width=3) for r in range(8)),
                  run_time=0.4)
        formula = self.change_text(formula, "column 0: word = r * 8", -2.86, 0.12, 19)
        bank_formula = self.change_text(bank_formula, "(r * 8) % 8 = 0", -2.86, -0.35,
                                        19, WARN)
        caption = self.change_text(caption, "Same bank. Different words. These reads cannot broadcast.",
                                   size=18, color=WARN)
        counter = text("READ SERVICE ROUNDS", 0, -1.04, 16, MUTED)
        self.play(FadeIn(counter), run_time=0.2)
        counter = self.transfer_column(8, positions, bank_x, counter)
        self.wait(1)

        # The code edit and physical row pitch change together, not via a UI toggle.
        self.play(focus.animate.move_to((-3.43, code_y[0], 0)), run_time=0.35)
        new_line = text("__shared__ float tile[8][8 + 1];", -3.43, code_y[0], 17, GOOD)
        extra = text("+1", 5.84, 1.25, 29, GOOD)
        padding = VGroup(*(rect(positions[r][-1][0] + 0.42, positions[r][-1][1],
                                0.385, 0.385, GOOD, 0.10, 1) for r in range(8)))
        caption = self.change_text(caption, "Add one unused word to every row. Keep the same values.",
                                   size=18, color=GOOD)
        self.play(FadeOut(code_lines[0]), FadeIn(new_line), FadeIn(padding),
                  Write(extra), run_time=0.8)
        code_lines[0] = new_line
        tile_label = self.change_text(tile_label, "SHARED TILE  [8][9]", 3.55, 2.99,
                                     15, GOOD)
        self.play(*(cells[r][c].animate.set_fill(BANK_COLORS[(r * 9 + c) % 8],
                                               opacity=0.42)
                    for r in range(8) for c in range(8)), run_time=1)
        formula = self.change_text(formula, "column 0: word = r * 9", -2.86, 0.12, 19)
        bank_formula = self.change_text(bank_formula, "(r * 9) % 8 = r", -2.86, -0.35,
                                        19, GOOD)
        self.wait(0.7)
        self.play(focus.animate.move_to((-3.43, code_y[3], 0)), run_time=0.35)
        caption = self.change_text(caption, "Every lane now reaches a different bank: one read round.",
                                   size=18, color=GOOD)
        counter = self.transfer_column(9, positions, bank_x, counter)
        self.wait(1.2)
        caption = self.change_text(caption, "Animation time is illustrative, not measured GPU time.",
                                   size=18, color=MUTED)
        self.wait(1.2)
        self.wipe(title, subtitle, tile, data_labels, tile_label, *code_lines, focus,
                  formula, bank_formula, banks_group, legend, caption, counter, padding, extra)

    def transfer_column(self, stride, positions, bank_x, counter):
        addresses = column_addresses(stride, 8)
        batches = read_service_batches(addresses, 8)
        requests = []
        occupied = [0] * 8
        destinations = []
        for lane, address in enumerate(addresses):
            bank = address % 8
            x, y, _ = positions[lane][0]
            packet = VGroup(rect(x, y, 0.36, 0.24, BANK_COLORS[bank], 1, 1),
                            text(str(lane), x, y, 12, BG))
            requests.append(packet)
            destinations.append((bank_x[bank], -1.46 + occupied[bank] * 0.255, 0))
            occupied[bank] += 1
        self.play(*(FadeIn(packet) for packet in requests), run_time=0.25)
        self.play(*(packet.animate.move_to(p) for packet, p in zip(requests, destinations)),
                  run_time=1.1)
        counter = self.change_text(counter, "0 / %d read rounds" % len(batches),
                                   0, -1.04, 18, MUTED)
        self.wait(0.6)
        # Equal duration per service round in BOTH cases. Only contention changes.
        for step, batch in enumerate(batches, 1):
            self.play(*(requests[lane].animate.move_to((bank_x[addresses[lane] % 8], -2.05, 0))
                        for lane in batch), run_time=ROUND_TIME, rate_func=linear)
            self.play(*(FadeOut(requests[lane]) for lane in batch), run_time=0.10)
            counter = self.change_text(counter, "%d / %d read rounds" % (step, len(batches)),
                                       0, -1.04, 18, GOOD if len(batches) == 1 else WARN,
                                       duration=0.10)
        return counter

    def cuda_stride_experiment(self):
        title = text("NOW USE CUDA'S 32 BANKS", y=3.55, size=29)
        subtitle = text("32 lanes / distinct 32-bit words / a full interior tile column",
                        y=3.10, size=16, color=MUTED)
        formula = text("bank = (lane * stride + column) % 32", y=2.54, size=23, color=ACCENT)
        code = text("__shared__ float tile[32][32];", y=1.89, size=23)
        axis = Line((-5.75, -1.7, 0), (5.75, -1.7, 0), color=EDGE)
        bank_x = [-5.58 + bank * 0.36 for bank in range(32)]
        labels = VGroup(*(text(str(bank), bank_x[bank], -1.98, 13, MUTED)
                          for bank in (0, 4, 8, 12, 16, 20, 24, 28, 31)))
        axis_title = text("SHARED-MEMORY BANK", y=-2.30, size=14, color=MUTED)
        key = text("Each small square = one distinct requested word", y=1.36, size=15, color=MUTED)
        metric = text("stride = 32", y=-2.84, size=23, color=WARN)
        caption = text("All 32 words land in bank 0.", y=-3.38, size=18)
        self.play(FadeIn(title), FadeIn(subtitle), Write(formula), Write(code),
                  Create(axis), FadeIn(labels), FadeIn(axis_title), FadeIn(key),
                  FadeIn(metric), FadeIn(caption), run_time=1)
        # Exactly 32 persistent request objects, not per-frame reconstruction.
        initial = column_addresses(32, 32)
        queues = [0] * 32
        packets = []
        for address in initial:
            bank = address % 32
            packets.append(rect(bank_x[bank], -1.58 + queues[bank] * 0.080,
                                0.26, 0.064, WARN, 1, 0))
            queues[bank] += 1
        self.play(*(FadeIn(packet) for packet in packets), run_time=0.5)
        for stride in CUDA_STRIDES:
            addresses = column_addresses(stride, 32)
            loads = bank_loads(stride, 32)
            rounds = len(read_service_batches(addresses, 32))
            used = sum(load > 0 for load in loads)
            queues = [0] * 32
            animations = []
            color = GOOD if rounds == 1 else WARN
            for packet, address in zip(packets, addresses):
                bank = address % 32
                p = (bank_x[bank], -1.58 + queues[bank] * 0.080, 0)
                queues[bank] += 1
                animations.append(packet.animate.move_to(p).set_fill(color, opacity=1))
            code = self.change_text(code, "__shared__ float tile[32][%d];" % stride,
                                    0, 1.89, 23, color)
            self.play(*animations, run_time=0.85)
            metric = self.change_text(metric, "%d banks used  /  %d read round%s" %
                                      (used, rounds, "" if rounds == 1 else "s"),
                                      0, -2.84, 23, color)
            message = {
                32: "Stride 32: every lane hits the same bank.",
                33: "Stride 33: every bank receives exactly one word.",
                34: "Stride 34: only 16 banks are used; two words per bank.",
                36: "Stride 36: only 8 banks are used; four words per bank.",
                48: "Stride 48: only 2 banks are used; sixteen words per bank.",
            }[stride]
            caption = self.change_text(caption, message, size=17)
            self.wait(1.15 if stride in (32, 33) else 0.8)
        self.wipe(*packets, axis, labels, axis_title, key, metric, caption)
        payoff = text("32-way conflict  ->  conflict-free", y=0.5, size=29, color=GOOD)
        cost = text("32 extra floats = 128 bytes per tile (+3.125%)", y=-0.27, size=20)
        reason = text("Change the layout, not the answer.", y=-1.15, size=28)
        warning = text("Fewer bank-conflict rounds do not imply a 32x kernel speedup.",
                       y=-2.23, size=16, color=MUTED)
        attribution = text("Inspired by CUDA Algorithm Lab / matrix_transpose / 03_padded",
                           y=-3.42, size=13, color=MUTED)
        self.play(Write(payoff), FadeIn(cost), run_time=0.8)
        self.play(Write(reason), FadeIn(warning), FadeIn(attribution), run_time=0.8)
        self.wait(2.5)
