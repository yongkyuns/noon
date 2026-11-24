# Copyright 2019–2023 Observable, Inc.
# SPDX-License-Identifier: ISC
# Mike Bostock, D3 Bar Chart Race, Observable notebook revision 3087:
# https://observablehq.com/d/3ff9fa2c6593d814@3087
# Source data: the notebook's category-brands.csv attachment, credited to Interbrand.
# https://static.observableusercontent.com/files/aec3792837253d4c6168f9bbecdf495140a5f9bb1cdb12c7c8113cec26332634a71ad29b446a1e8236e0a45732ea5d0b4e86d9d1568ff5791412f093ec06f4f1
# See THIRD_PARTY_NOTICES.md for the notebook, exact attachment, and license.
"""The value of the top global brands, 2000–2019, in millions of dollars."""

from math import floor, log10, sqrt

from noon import *


WIDTH = 1064  # The original notebook scales the chart to its column width.
HEIGHT = 598
PIXELS_PER_UNIT = 74.8125
PLOT_WIDTH = WIDTH - 6
ROW_COUNT = 12
ROW_STEP = 48
BAR_HEIGHT = 43
TRANSITION_TICKS = 15  # The notebook transitions for 250 ms at each keyframe.
TICKS_PER_SECOND = 60
FRAMES_PER_YEAR = 10
FIRST_YEAR = 2000
LAST_YEAR = 2019
DURATION_TICKS = (FRAMES_PER_YEAR * (LAST_YEAR - FIRST_YEAR) + 1) * TRANSITION_TICKS
FONT = "Arimo"

# Interbrand values in millions of dollars, one column per year from 2000 to 2019.
# The 21 series below are every brand that reaches the top 12 in the source.
# Rows retain the notebook’s input order for ties. An absent value is zero.
BRANDS = (
    ('Coca-Cola', '#4e79a7', (72537, 68945, 69637, 70453, 67394, 67525, 67000, 65324, 66667, 68734, 70452, 71861, 77839, 79213, 81563, 78423, 73102, 69733, 66341, 63365)),
    ('Microsoft', '#f28e2c', (70196, 65068, 64091, 65174, 61372, 59941, 56926, 58709, 59007, 56647, 60895, 59087, 57853, 59546, 61154, 67670, 72795, 79999, 92715, 108847)),
    ('IBM', '#e15759', (53183, 52752, 51188, 51767, 53791, 53376, 56201, 57090, 59031, 60211, 64727, 69905, 75532, 78808, 72244, 65095, 52500, 46829, 42972, 40381)),
    ('Intel', '#f28e2c', (39048, 34665, 30861, 31112, 33499, 35588, 32319, 30954, 31261, 30636, 32015, 35217, 39385, 37257, 34153, 35415, 36952, 39459, 43293, 40197)),
    ('Nokia', '#f28e2c', (38528, 35035, 29970, 29440, 24041, 26452, 30131, 33696, 35942, 34864, 29495, 25071, 21009, 7444, 4138, 0, 0, 0, 0, 0)),
    ('GE', '#76b7b2', (38127, 42396, 41311, 42340, 44111, 46996, 48907, 51569, 53086, 47777, 42808, 42808, 43682, 46947, 45480, 42267, 43130, 44208, 32757, 25566)),
    ('Ford', '#59a14f', (36368, 30092, 20403, 17066, 14475, 13159, 11056, 8982, 7896, 7005, 7195, 7483, 7958, 9181, 10876, 11578, 12962, 13643, 13995, 14325)),
    ('Disney', '#edc949', (33553, 32591, 29256, 28036, 27113, 26441, 27848, 29210, 29251, 28447, 28731, 29018, 27438, 28147, 32223, 36514, 38790, 40772, 39874, 44352)),
    ("McDonald's", '#af7aa1', (27859, 25289, 26375, 24699, 25001, 26014, 27501, 29398, 31049, 32275, 33578, 35593, 40062, 41992, 42254, 39809, 39381, 41533, 43417, 45362)),
    ('AT&T', '#ff9da7', (25548, 22828, 16059, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0)),
    ('Marlboro', '#9c755f', (22110, 22053, 24151, 22183, 22128, 21189, 21350, 21282, 21300, 19010, 19961, 0, 0, 0, 0, 0, 0, 0, 0, 0)),
    ('Mercedes-Benz', '#59a14f', (21104, 21728, 21010, 21371, 21331, 20006, 21795, 23568, 25577, 23867, 25179, 27445, 30097, 31904, 34338, 36711, 43490, 47829, 48601, 50832)),
    ('HP', '#bab0ab', (20572, 17983, 16776, 19860, 20978, 18866, 20458, 22197, 23509, 24096, 26867, 28479, 26087, 25843, 23758, 23056, 10386, 9541, 10433, 10891)),
    ('Toyota', '#59a14f', (18823, 18578, 19448, 20784, 22673, 24837, 27941, 32070, 34050, 31330, 26192, 27764, 30280, 35346, 42392, 49048, 53580, 50291, 53404, 56246)),
    ('Citi', '#4e79a7', (18809, 19005, 18066, 18571, 19971, 19967, 21458, 23442, 20174, 10254, 8887, 8620, 7570, 7973, 8737, 9784, 10276, 10599, 11577, 12697)),
    ('BMW', '#59a14f', (12969, 13858, 14425, 15106, 15886, 17126, 19617, 21612, 23298, 21671, 22322, 24554, 29052, 31839, 34214, 37212, 41535, 41521, 41006, 41440)),
    ('Apple', '#f28e2c', (6594, 5464, 5316, 5554, 6871, 7985, 9130, 11037, 13724, 15433, 21143, 33492, 76568, 98316, 118863, 170276, 178119, 184154, 214480, 234241)),
    ('Samsung', '#f28e2c', (5223, 6374, 8310, 10846, 12553, 14956, 16169, 16853, 17689, 17518, 19491, 23430, 32893, 39610, 45462, 45297, 51808, 56249, 59890, 61098)),
    ('Amazon', '#f28e2c', (4528, 3130, 3175, 3403, 4156, 4248, 4707, 5411, 6434, 7858, 9665, 12758, 18625, 23620, 29478, 37948, 50338, 64796, 100764, 125263)),
    ('Google', '#f28e2c', (0, 0, 0, 0, 0, 8461, 12376, 17837, 25590, 31980, 43557, 55317, 69726, 93291, 107439, 120314, 133252, 141703, 155506, 167713)),
    ('Facebook', '#f28e2c', (0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 5421, 7732, 14349, 22029, 32593, 48188, 45168, 39857)),
)


def screen(x, y):
    return ((x - WIDTH / 2) / PIXELS_PER_UNIT, (HEIGHT / 2 - y) / PIXELS_PER_UNIT)


def row_center(rank):
    return 21 + ROW_STEP * rank + BAR_HEIGHT / 2


def rounded(value):
    return floor(value + 0.5)


def chart_keys():
    """Reproduce the notebook's ten linear value samples per year and stable ranking."""
    frames = []
    for index in range((LAST_YEAR - FIRST_YEAR) * FRAMES_PER_YEAR + 1):
        year = index // FRAMES_PER_YEAR
        fraction = (index % FRAMES_PER_YEAR) / FRAMES_PER_YEAR
        values = [history[year] * (1 - fraction) + history[min(year + 1, LAST_YEAR - FIRST_YEAR)] * fraction
                  for _, _, history in BRANDS]
        order = sorted(range(len(values)), key=lambda brand: -values[brand])
        ranks = [ROW_COUNT for _ in values]
        for rank, brand in enumerate(order[:ROW_COUNT]):
            ranks[brand] = rank
        frames.append((values, ranks, values[order[0]]))
    return [frames[0]] + frames


def tick_values(high):
    """The positive ticks of d3.scaleLinear().ticks(width / 160)."""
    target = high / (WIDTH / 160)
    unit = 10 ** floor(log10(target))
    error = target / unit
    factor = 10 if error >= sqrt(50) else 5 if error >= sqrt(10) else 2 if error >= sqrt(2) else 1
    step = unit * factor
    return range(step, int(high) + 1, step)


def held_keys(values, step=1):
    return [(i * step, value) for i, value in enumerate(values)
            if i == 0 or value != values[i - 1]]


def digit_visibility(marks):
    """Collect only the visibility changes for each digit in this column."""
    tracks = {}
    previous = " "
    for tick, char in enumerate(marks):
        if char == previous:
            continue
        if previous != " ":
            tracks[previous].append((tick, 0))
        if char != " ":
            if char not in tracks:
                tracks[char] = [(0, 0)] if tick else []
            tracks[char].append((tick, 0.7))
        previous = char
    return tracks


class BrandValueRace(Scene):
    def ink(self, text, pixels, bold=False):
        if bold:
            return MarkupText("<b>" + text.replace("&", "&amp;") + "</b>", font=FONT,
                              font_size=pixels * 72 / PIXELS_PER_UNIT, color=BLACK)
        return Text(text, font=FONT, font_size=pixels * 72 / PIXELS_PER_UNIT, color=BLACK)

    def label(self, mob, clock, poses, opacity, *, alignment=1, below_baseline=0):
        # alignment is 1 for right-aligned text and 0 for centered tick labels.
        dx = alignment * mob.width * PIXELS_PER_UNIT / 2
        dy = below_baseline - mob.height * PIXELS_PER_UNIT / 2
        self.add(mob)
        self.bind_sampled(mob, clock, "position",
                          [(t, *screen(x - dx, y + dy)) for t, x, y in poses])
        self.bind_sampled(mob, clock, "opacity", opacity, linear=False)

    def numbers(self, clock, poses, values, visible):
        # The source right-aligns a tabular, comma-separated number on every bar.
        # Reuse the digit outlines and store only the changes in their visibility.
        marks = []
        for tick in range(DURATION_TICKS + 1):
            segment = min(tick // TRANSITION_TICKS, len(values) - 2)
            phase = tick / TRANSITION_TICKS - segment
            value = values[segment] * (1 - phase) + values[segment + 1] * phase
            marks.append(f"{rounded(value):>7,}"[::-1] if visible[segment] else " " * 7)
        advance = 12 * 1139 / 2048
        comma = 12 * 569 / 2048
        right = 0
        for column in range(7):
            slot_width = comma if column == 3 else advance
            center = right + slot_width / 2
            for char, opacity in digit_visibility([value[column] for value in marks]).items():
                glyph = self.ink(char, 12)
                self.label(glyph, clock, [(t, x - center, y) for t, x, y in poses], opacity,
                           alignment=0, below_baseline=1.8 if char == "," else 0)
            right += slot_width

    def axis(self, clock, frames):
        domains = [high for _, _, high in frames]
        ticks = [set(tick_values(high)) for high in domains]
        origin = Rectangle(width=1 / PIXELS_PER_UNIT, height=581 / PIXELS_PER_UNIT,
                           fill=BLACK, fill_opacity=1, stroke=None)
        origin.move_to((*screen(0.5, 306.5), 0))
        self.add(origin)
        self.bind_sampled(origin, clock, "opacity", [(0, 0), (TRANSITION_TICKS, 1)])
        values = sorted(set(value for frame in ticks for value in frame))
        for value in values:
            line = Rectangle(width=1 / PIXELS_PER_UNIT, height=581 / PIXELS_PER_UNIT,
                             fill=WHITE, fill_opacity=1, stroke=None)
            text = self.ink(f"{value:,}", 10)
            self.add(line, text)
            poses = [(index * TRANSITION_TICKS, PLOT_WIDTH * value / high + 0.5)
                     for index, high in enumerate(domains)]
            opacity = [(index * TRANSITION_TICKS, float(index > 0 and value in present))
                       for index, present in enumerate(ticks)]
            self.bind_sampled(line, clock, "position", [(t, *screen(x, 306.5)) for t, x in poses])
            self.bind_sampled(line, clock, "opacity", opacity)
            self.bind_sampled(text, clock, "position",
                              [(t, *screen(x, 13 - text.height * PIXELS_PER_UNIT / 2))
                               for t, x in poses])
            self.bind_sampled(text, clock, "opacity", opacity)

    def construct(self):
        paper = Rectangle(width=16, height=9, fill=WHITE, fill_opacity=1, stroke=None)
        self.add(paper.set_z_index(-ROW_COUNT - 2))
        frames = chart_keys()
        clock = self.value_tracker(0)
        bars, label_tracks = [], []
        for brand, (name, color, _) in enumerate(BRANDS):
            values = [values[brand] for values, _, _ in frames]
            ranks = [ranks[brand] for _, ranks, _ in frames]
            highs = [high for _, _, high in frames]
            # On entry D3 places the preceding value on the new axis scale.
            widths = [PLOT_WIDTH * value / (highs[i + 1]
                      if i + 1 < len(ranks) and ranks[i] == ROW_COUNT and ranks[i + 1] < ROW_COUNT
                      else highs[i]) for i, value in enumerate(values)]
            # A brand glides in from the first off-screen row and leaves by the same route.
            visible = [min(a, b) < ROW_COUNT for a, b in zip(ranks, ranks[1:])]
            visible.append(ranks[-1] < ROW_COUNT)
            bar = Rectangle(width=1, height=BAR_HEIGHT / PIXELS_PER_UNIT, fill=color,
                            fill_opacity=0.6, stroke=None)
            self.add(bar.set_z_index(ranks[0] - ROW_COUNT - 1))
            bars.append(bar)
            self.bind_sampled(bar, clock, "position",
                              [(i * TRANSITION_TICKS, *screen(width / 2, row_center(rank)))
                               for i, (width, rank) in enumerate(zip(widths, ranks))])
            self.bind_sampled(bar, clock, "scale",
                              [(i * TRANSITION_TICKS, max(0.0001, width / PIXELS_PER_UNIT), 1)
                               for i, width in enumerate(widths)])
            opacity = held_keys([int(show) for show in visible], TRANSITION_TICKS)
            self.bind_sampled(bar, clock, "opacity", opacity, linear=False)
            labels = [(i * TRANSITION_TICKS, width - 6, row_center(rank))
                      for i, (width, rank) in enumerate(zip(widths, ranks))]
            label_tracks.append((name, labels, values, visible, opacity))

        self.axis(clock, frames)
        for name, labels, values, visible, opacity in label_tracks:
            self.label(self.ink(name, 12, True), clock, [(t, x, y - 3) for t, x, y in labels],
                       opacity, below_baseline=2.4 if any(char in name for char in "gjpqy") else 0)
            self.numbers(clock, [(t, x, y + 10.8) for t, x, y in labels], values, visible)

        for year in range(FIRST_YEAR, LAST_YEAR + 1):
            start = 0 if year == FIRST_YEAR else ((year - FIRST_YEAR) * FRAMES_PER_YEAR + 1) * TRANSITION_TICKS
            stop = ((year - FIRST_YEAR + 1) * FRAMES_PER_YEAR + 1) * TRANSITION_TICKS
            ticks = [(0, 0), (start, 1)] if start else [(0, 1)]
            if year != LAST_YEAR:
                ticks.append((stop, 0))
            ticker = self.ink(str(year), ROW_STEP, True).stretch(1.19, 0)
            self.label(ticker, clock, [(0, WIDTH - 6, 585.76)], ticks)

        # D3 orders the bars by the destination rank before a crossing.
        # Use authored boundaries only when that paint order changes.
        previous_tick = 0
        for index, (before, after) in enumerate(zip(frames, frames[1:])):
            if before[1] == after[1]:
                continue
            tick = index * TRANSITION_TICKS
            self.play(clock.animate.set_value(tick),
                      run_time=(tick - previous_tick) / TICKS_PER_SECOND, rate_func=linear)
            for bar, old, rank in zip(bars, before[1], after[1]):
                if old != rank:
                    bar.set_z_index(rank - ROW_COUNT - 1)
            previous_tick = tick
        self.play(clock.animate.set_value(DURATION_TICKS),
                  run_time=(DURATION_TICKS - previous_tick) / TICKS_PER_SECOND, rate_func=linear)
