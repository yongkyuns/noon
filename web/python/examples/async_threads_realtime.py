"""Async, threads, and the deadline — an original, 13-chapter Noon tutorial.

Run this file in Noon's Python editor. The complete lesson is 522 authored
seconds. Set START_CHAPTER and END_CHAPTER to the same number to study one
chapter; SPEED changes presentation speed, never the illustrated microseconds.

This is educational animation, not a controller implementation or a benchmark.
All three response-time examples use the same hypothetical single-core model.
Domain numbers are computed below; Noon owns animation, time, and rendering.
No frame callbacks, replacement renderer, or external assets are required.

Primary technical references and a chapter guide accompany this file in
async_threads_realtime.md. In particular, async state representation does not
determine scheduling policy; immediately available work need not yield; and
memory, resource, timing and system guarantees require different contracts.
"""

from math import sqrt

from noon import *


START_CHAPTER = 1
END_CHAPTER = 13
SPEED = 1.0

# Title, authored duration at SPEED=1, and scene method. Each chapter is complete
# on its own. The final hold reads Scene.time from the shared engine; this table
# is an authoring cue sheet, not a second animation clock.
CHAPTERS = (
    ("Three programs, one execution stream", 32, "choices"),
    ("Paused work keeps its place", 34, "memory"),
    ("Both runtimes can release the CPU for I/O", 52, "waiting"),
    ("Available work can keep the CPU busy", 40, "await_paths"),
    ("Selection priority cannot interrupt a poll", 40, "long_poll"),
    ("Bounded cooperative work can succeed", 34, "bounded_poll"),
    ("Preempt, preserve, resume", 44, "preemption"),
    ("Three architectures, still one CPU", 42, "architectures"),
    ("Which performance metric improves?", 30, "performance"),
    ("The highest priority can still be blocked", 46, "inversion"),
    ("Timeout and cancellation need contracts", 42, "timeouts"),
    ("Safety needs explicit guarantees", 32, "assurance"),
    ("Follow the sample all the way to actuation", 54, "control_boundary"),
)

PERIOD_US = 1000
DEADLINE_US = 250
CONTROL_US = 80
DISPATCH_US = 5
AXIS_US = 650
SCENARIOS = (("long", 500), ("bounded", 50), ("preemptive", 10))


def response_example(blocking_us):
    """Arithmetic for ONE release, with no other interference or I/O waits.

    The bounded cooperative example additionally assumes control is selected
    next. The preemptive example's bound includes all applicable lower-priority
    nonpreemptible/resource blocking. These are given bounds, not measured WCET.
    """
    start = blocking_us + DISPATCH_US
    finish = start + CONTROL_US
    return {
        "blocking": (0, blocking_us),
        "dispatch": (blocking_us, start),
        "control": (start, finish),
        "finish": finish,
        "margin": DEADLINE_US - finish,
        "met": finish <= DEADLINE_US,
    }


BG = color_from_hex("#0B1020")
PANEL = color_from_hex("#141F33")
RAISED = color_from_hex("#1B2B44")
EDGE = color_from_hex("#40516D")
INK = color_from_hex("#E6EEF9")
MUTED = color_from_hex("#9AACC5")
CONTROL = color_from_hex("#63C7FF")
ESTIMATE = color_from_hex("#F7BD67")
LOGGER = color_from_hex("#BAA7FF")
GOOD = color_from_hex("#6FE3B5")
BAD = color_from_hex("#FF7885")
ROLES = (("C", "CONTROL", CONTROL), ("E", "ESTIMATE", ESTIMATE),
         ("L", "LOG", LOGGER))

# Presentation dimensions, not object positions. Native groups and measured
# anchors below place every component inside the available scene regions.
PAGE_MARGIN = 0.64
PAGE_GAP = 0.12
PANEL_PADDING = 0.20
COLUMN_GAP = 0.40
CONTENT_WIDTH = DEFAULT_FRAME_WIDTH - 2 * PAGE_MARGIN


def words(source, *, size=20, color=INK, width=CONTENT_WIDTH):
    obj = Text(str(source), font="DejaVu Sans Mono", font_size=size, color=color)
    if obj.width > width:
        obj.scale(width / obj.width)
    return obj


def panel(width, height, fill=PANEL, edge=EDGE, opacity=1, stroke=1):
    return Rectangle(width=width, height=height, color=edge,
                     fill_opacity=opacity, stroke_width=stroke).set_fill(
                         color=fill, opacity=opacity)


def rule(a, b, color=EDGE, width=1.5):
    return Line((*a[:2], 0), (*b[:2], 0), color=color, stroke_width=width)


def connector(points, color=MUTED, width=2.2):
    """Explicit diagram waypoints; the shared engine owns path geometry/drawing."""
    for a, b in zip(points, points[1:]):
        if a == b or (a[0] != b[0] and a[1] != b[1]):
            raise ValueError("Diagram routes must have nonzero orthogonal segments.")
    a, b = points[-2:]
    length = sqrt((b[0] - a[0]) ** 2 + (b[1] - a[1]) ** 2)
    tail = (b[0] - (b[0] - a[0]) / length * 0.23,
            b[1] - (b[1] - a[1]) / length * 0.23)
    shaft = VMobject(stroke_color=color, stroke_width=width,
                     fill_opacity=0).set_points_as_corners(points)
    tip = Arrow(tail, b, buff=0, color=color, stroke_width=width, tip_length=0.14)
    return shaft, tip


def card(title, detail, *, width=3.5, height=1.0, color=CONTROL):
    body = panel(width, height)
    accent = panel(0.05, height, color, color, stroke=0).match_x(body, LEFT)
    labels = [words(title, size=19, color=color, width=width - 2 * PANEL_PADDING)]
    if detail:
        labels.append(words(detail, size=13, color=MUTED, width=width - 2 * PANEL_PADDING))
    VGroup(*labels).arrange(DOWN, buff=0.15).move_to(body)
    return body, accent, *labels


def badge(text, color=CONTROL, width=None):
    label = words(text, size=13, color=color,
                  width=CONTENT_WIDTH if width is None else width - 2 * PANEL_PADDING)
    body = panel(label.width + 2 * PANEL_PADDING if width is None else width,
                 label.height + 2 * PAGE_GAP, RAISED, color)
    return body, label.move_to(body)


def station(title, *, width=3.0, height=1.45, color=INK):
    body = panel(width, height)
    label = words(title, size=13, color=color, width=width - 2 * PANEL_PADDING)
    label.next_to(body.get_top(), DOWN, buff=0.13)
    content = Rectangle(width=body.width - 2 * PANEL_PADDING,
                        height=body.height - label.height - 0.13 - 2 * PAGE_GAP,
                        stroke_width=0, fill_opacity=0).next_to(label, DOWN, buff=PAGE_GAP)
    return body, label, content


def job(letter, color, completed=0):
    """A native family: task identity plus qualitative completed-work markers.

    Markers travel with the task and stop changing outside the CPU. They are
    not percentages or instruction counts; the microsecond axis carries time.
    """
    body = panel(0.98, 0.70, RAISED, color, stroke=2)
    name = words(letter, size=21, color=color, width=0.7).next_to(body.get_top(), DOWN, buff=0.10)
    marks = VGroup(*(panel(0.125, 0.105, GOOD if i < completed else EDGE,
                           EDGE, stroke=0) for i in range(4)))
    marks.arrange(RIGHT, buff=0.06).next_to(body.get_bottom(), UP, buff=0.10)
    return VGroup(body, name, *marks.submobjects)


def packet(color=ESTIMATE):
    """Transport is a glowing circular signal, distinct from program progress."""
    dot = Circle(radius=0.075, color=color, stroke_width=0).set_fill(
        color=color, opacity=1)
    dot.set_glow(color=color, radius=Pixels(5), intensity=0.8)
    return dot


def authored_objects(*items):
    # Expand only authored tuples/lists. Native Noon families (for example jobs)
    # pass through unchanged; their membership and layout remain Rust-owned.
    result = []
    for item in items:
        if isinstance(item, (tuple, list)):
            result.extend(authored_objects(*item))
        else:
            result.append(item)
    return result


class AsyncThreadsRealtime(Scene):
    """One retained Noon scene, thirteen independently runnable chapters."""

    async def play_for(self, *animations, seconds=0.5, rate_func=smooth):
        await self.play(*animations, run_time=seconds / SPEED, rate_func=rate_func)

    async def pause(self, seconds=1.0):
        await self.wait(seconds / SPEED)

    async def show(self, *items, seconds=0.55):
        objects = authored_objects(*items)
        self.content.extend(objects)
        animations = [Write(obj) if isinstance(obj, Text) else
                      Create(obj) if isinstance(obj, Line) else FadeIn(obj)
                      for obj in objects]
        await self.play_for(*animations, seconds=seconds)

    async def draw_route(self, points, color=MUTED, seconds=0.7):
        shaft, tip = connector(points, color)
        self.content.extend((shaft, tip))
        await self.play_for(Create(shaft), seconds=seconds, rate_func=linear)
        await self.play_for(GrowArrow(tip), seconds=0.18)
        return shaft, tip

    async def move_job(self, token, *targets, seconds=1.1):
        # These are ordinary native family transforms, one per activation.
        # Python supplies authored destinations, never per-frame interpolation.
        for target in targets:
            await self.play_for(token.animate.move_to(target),
                                seconds=seconds / len(targets))

    async def compute(self, token, begin=0, end=4, seconds=2.0):
        for index in range(begin, end):
            await self.play_for(token.submobjects[index + 2].animate.set_fill(GOOD, opacity=1),
                                seconds=seconds / (end - begin), rate_func=linear)

    async def pulse_data(self, points, color=ESTIMATE, seconds=1.2):
        dot = packet(color).move_to(points[0])
        await self.show(dot, seconds=0.12)
        lengths = [sqrt((b[0] - a[0]) ** 2 + (b[1] - a[1]) ** 2)
                   for a, b in zip(points, points[1:])]
        total = sum(lengths)
        for point, length in zip(points[1:], lengths):
            # Distances size the authored segments. Native smooth interpolation
            # eases into each orthogonal turn; there is no per-frame callback.
            await self.play_for(dot.animate.move_to(point),
                                seconds=seconds * length / total, rate_func=smooth)
        await self.hide(dot, seconds=0.12)

    async def hide(self, *items, seconds=0.3):
        objects = authored_objects(*items)
        # Promote the selection to one native scene root before fading. This
        # preserves layout families while removing all selected root members,
        # including leaves that still have detached layout-family parents.
        fading = VGroup(*objects)
        self.add(fading)
        await self.play_for(FadeOut(fading), seconds=seconds)
        removed = {id(obj) for obj in objects}
        self.content = [obj for obj in self.content if id(obj) not in removed]

    async def caption(self, main, detail="", color=INK, phase="EXPLAIN"):
        phase_color = {"EXPLAIN": MUTED, "WATCH": ESTIMATE, "NOTICE": GOOD}[phase]
        phase_label = words(phase, size=9, color=phase_color).move_to(
            self.caption_region, aligned_edge=UL)
        main_label = words(main, size=20, color=color)
        detail_label = words(detail or " ", size=13, color=MUTED)
        VGroup(main_label, detail_label).arrange(DOWN, buff=0.18).next_to(
            phase_label, DOWN, buff=0.08).match_x(self.caption_region)
        new = [main_label, detail_label, phase_label]
        # Remove the old sentence before writing the next, so no transition
        # frame superimposes two different explanations in the same space.
        if self.captions:
            await self.hide(self.captions, seconds=0.13)
        else:
            await self.pause(0.13)
        await self.play_for(Write(new[0]), FadeIn(new[1]), FadeIn(new[2]), seconds=0.47)
        self.captions = new

    async def explain(self, main, detail="", color=INK, hold=2.2):
        await self.caption(main, detail, color, phase="EXPLAIN")
        await self.pause(hold)

    async def watch(self, main, detail="", color=INK, hold=0.7):
        await self.caption(main, detail, color, phase="WATCH")
        await self.pause(hold)

    async def notice(self, main, detail="", color=INK, hold=2.6):
        await self.caption(main, detail, color, phase="NOTICE")
        await self.pause(hold)

    async def clear_focus(self):
        if self.focus is not None:
            await self.hide(self.focus, seconds=0.25)
            self.focus = None

    async def focus_on(self, target, color=CONTROL, buff=0.055):
        await self.clear_focus()
        # An attention cue, independent of every program's execution pointer.
        # SurroundingRectangle measures the current native bounds; the next cue
        # is constructed after any layout change, rather than following a callback.
        self.focus = SurroundingRectangle(target, buff=buff, corner_radius=0.07,
                color=color, stroke_width=1.5).set_fill(color, opacity=0.055).set_stroke(
                    color, opacity=0.8)
        await self.show(self.focus, seconds=0.35)

    async def loop_board(self, subtitle, line_indices=(0, 0, 0),
                         states=("WAITING", "READY", "READY"), owner=None):
        """Arrange three native program families; retain named handles for cues.

        The returned dictionary contains authored handles and hypothetical CPU
        ownership only. It does not mirror Noon state, interpolate frames or
        implement an animation clock. All transitions use the shared engine.
        """
        programs = (
            ("wait for release", "acquire sample", "read state snapshot",
             "compute command", "send command"),
            ("wait for measurement", "update estimate", "publish snapshot"),
            ("receive packet", "encode + buffer", "if due: flush output"),
        )
        board = {"owner": owner}
        families, objects = [], []
        width = (self.stage.width - 2 * COLUMN_GAP) / len(ROLES)
        padding, indent = PANEL_PADDING, PANEL_PADDING
        listing_width = width - 2 * padding - indent
        listings = [VGroup(*(words(line, size=13, width=listing_width) for line in program))
                    .arrange(DOWN, buff=0.10, aligned_edge=LEFT) for program in programs]
        listing_height = max(listing.height for listing in listings)
        state_template = words("PREEMPTED", size=10)
        subtitle_label = words(subtitle, size=11, color=MUTED).next_to(
            self.stage.get_top(), DOWN, buff=0.25)
        for (letter, title, color), listing, line_index, state in zip(
                ROLES, listings, line_indices, states):
            rows = list(listing.submobjects)
            heading = words(letter + " / " + title, size=13, color=color,
                            width=width - state_template.width - 3 * padding - PAGE_GAP)
            repeat = words("repeat:", size=12, color=MUTED)
            marks = [panel(0.42, 0.065, EDGE, EDGE, stroke=0) for _ in range(4)]
            work_marks = VGroup(*marks).arrange(RIGHT, buff=0.10)
            work_label = words("work", size=10, color=MUTED)
            work_band = VGroup(work_label, work_marks).arrange(RIGHT, buff=0.34)
            # Equal-height cards reserve the tallest actual listing, rather
            # than encoding a second row-height formula or a fixed panel height.
            height = (heading.height + repeat.height + listing_height + work_band.height
                      + 2 * padding + 3 * PAGE_GAP + PANEL_PADDING)
            body = panel(width, height)
            heading.move_to(body, aligned_edge=UL).shift(padding * (RIGHT + DOWN))
            # Reserve a stable status slot so READY -> RUNNING -> PREEMPTED
            # changes the label without resizing the card or shifting its title.
            state_slot = Rectangle(width=state_template.width + PAGE_GAP,
                                   height=state_template.height, stroke_width=0, fill_opacity=0)
            state_slot.move_to(body, aligned_edge=UR).shift(LEFT * padding).match_y(heading)
            state_label = words(state, size=10, color=color, width=state_slot.width).move_to(state_slot)
            header = VGroup(heading, state_slot)
            repeat.next_to(header, DOWN, buff=2 * PAGE_GAP, aligned_edge=LEFT)
            listing.next_to(repeat, DOWN, buff=PAGE_GAP, aligned_edge=LEFT).shift(RIGHT * indent)
            # The execution strip spans the code area; the separate attention
            # outline below hugs the actual line being explained.
            highlight = RoundedRectangle(width=listing_width + 0.14,
                    height=max(row.height for row in rows) + 0.08, corner_radius=0.045,
                    color=color, stroke_width=0.8).set_fill(color, opacity=0.07).set_stroke(
                        color, opacity=0.50)
            highlight.match_x(listing, LEFT).shift(LEFT * 0.07).match_y(rows[line_index])
            pointer = Circle(radius=0.038, color=color, stroke_width=0).set_fill(color, opacity=1)
            pointer.next_to(rows[line_index], LEFT, buff=0.18)
            pointer.set_glow(color=color, radius=Pixels(3.5), intensity=0.65)
            work_band.next_to(body.get_bottom(), UP, buff=padding)
            leaves = [body, heading, state_label, repeat,
                      highlight, *rows, pointer, work_label, *marks]
            families.append(VGroup(*leaves, state_slot))
            objects.extend(leaves)
            board[letter] = {"body": body, "header": header, "color": color, "rows": rows, "listing": listing,
                             "state": state_label, "state_slot": state_slot, "pointer": pointer,
                             "highlight": highlight, "marks": marks}
        program_row = VGroup(*families).arrange(RIGHT, buff=COLUMN_GAP).next_to(
            subtitle_label, DOWN, buff=PANEL_PADDING)
        # Decorative strokes are created after family placement. Native Create
        # leaf targets must start without a family parent in this revision.
        for letter, _, _ in ROLES:
            item = board[letter]
            divider = Line(LEFT, RIGHT, color=EDGE, stroke_width=1.5).stretch_to_fit_width(
                item["body"].width - 2 * padding).next_to(item["header"], DOWN, buff=PAGE_GAP)
            objects.append(divider.match_x(item["body"]))
        cpu_title = words("ONE CPU", size=11, color=MUTED)
        cpu_body = panel(width, cpu_title.height + 2 * PANEL_PADDING, RAISED).next_to(
            program_row, DOWN, buff=PAGE_GAP)
        cpu_title.next_to(cpu_body.get_left(), RIGHT, buff=PANEL_PADDING)
        cpu_slot = Rectangle(width=cpu_body.width - cpu_title.width - 3 * PANEL_PADDING,
                             height=cpu_title.height, stroke_width=0, fill_opacity=0)
        cpu_slot.next_to(cpu_title, RIGHT, buff=PANEL_PADDING)
        cpu_label = words((owner + " RUNNING") if owner else "NO TASK SELECTED", size=13,
                          color=dict((r[0], r[2]) for r in ROLES).get(owner, MUTED),
                          width=cpu_slot.width).move_to(cpu_slot)
        objects += [subtitle_label, cpu_body, cpu_title, cpu_label]
        board.update(programs=program_row, cpu=cpu_label, cpu_body=cpu_body, cpu_slot=cpu_slot)
        await self.show(objects, seconds=0.75)
        return board

    async def set_state(self, board, role, state):
        item = board[role]
        await self.hide(item["state"], seconds=0.12)
        item["state"] = words(state, size=10, color=item["color"],
                              width=item["state_slot"].width).move_to(item["state_slot"])
        await self.show(item["state"], seconds=0.22)

    async def select(self, board, role):
        # The captioned hypothetical scheduler selects one program. Selection
        # alone never advances that program's pointer or completed-work marks.
        await self.hide(board["cpu"], seconds=0.12)
        label = role + " RUNNING" if role else "DISPATCH"
        color = board[role]["color"] if role else MUTED
        board["cpu"] = words(label, size=13, color=color,
                              width=board["cpu_slot"].width).move_to(board["cpu_slot"])
        board["owner"] = role
        await self.show(board["cpu"], seconds=0.22)
        if role:
            await self.set_state(board, role, "RUNNING")

    async def at_line(self, board, role, index, seconds=0.30):
        if board["owner"] != role:
            raise ValueError("Only the CPU-selected program can advance.")
        item = board[role]
        row = item["rows"][index]
        await self.play_for(item["pointer"].animate.next_to(row, LEFT, buff=0.18),
                            item["highlight"].animate.match_y(row),
                            *[mark.animate.set_fill(EDGE, opacity=1) for mark in item["marks"]],
                            seconds=seconds)

    async def work(self, board, role, begin=0, end=4, seconds=1.4):
        if board["owner"] != role:
            raise ValueError("Only the CPU-selected program can compute.")
        for index in range(begin, end):
            await self.play_for(board[role]["marks"][index].animate.set_fill(GOOD, opacity=1),
                                seconds=seconds / (end - begin), rate_func=linear)

    async def focus_loop(self, board, role, index=None):
        item = board[role]
        await self.play_for(*[row.animate.set_opacity(1 if letter == role else 0.50)
                             for letter, _, _ in ROLES for row in board[letter]["rows"]],
                            seconds=0.25)
        if index is None:
            await self.focus_on(item["body"], item["color"])
        else:
            await self.focus_on(item["rows"][index], item["color"], buff=0.055)

    async def restore_board(self, board):
        await self.clear_focus()
        await self.play_for(*[row.animate.set_opacity(1)
                             for letter, _, _ in ROLES for row in board[letter]["rows"]],
                            seconds=0.25)

    async def begin_chapter(self, number, title):
        old = self.content + self.captions + self.heading
        self.content, self.captions = [], []
        self.focus = None
        self.heading = [
            words(title, size=28, width=self.header_region.width).move_to(
                self.header_region, aligned_edge=DL).shift(UP * PAGE_GAP),
            words("%02d / 13" % number, size=12, color=MUTED).move_to(
                self.header_region, aligned_edge=UR).match_y(self.kicker),
        ]
        if old:
            await self.hide(old, seconds=0.25)
        else:
            await self.pause(0.25)
        await self.play_for(*(Write(obj) for obj in self.heading), seconds=0.40)

    async def construct(self):
        if (type(START_CHAPTER) is not int or type(END_CHAPTER) is not int
                or not 1 <= START_CHAPTER <= END_CHAPTER <= len(CHAPTERS)):
            raise ValueError("Choose chapter numbers from 1 through 13 in order.")
        if isinstance(SPEED, bool) or not isinstance(SPEED, (int, float)) or not 0 < SPEED < 20:
            raise ValueError("SPEED must be a finite number between 0 and 20.")
        self.content, self.captions, self.heading = [], [], []
        self.focus = None
        self.canvas = panel(DEFAULT_FRAME_WIDTH, DEFAULT_FRAME_HEIGHT, BG, BG, stroke=0)
        footer = words(
            "ONE CORE PER DESIGN  /  work marks are qualitative  /  timing bounds are illustrative",
            size=10, color=MUTED)
        self.header_region = Rectangle(width=CONTENT_WIDTH, height=1.0)
        self.caption_region = Rectangle(width=CONTENT_WIDTH, height=1.0)
        self.stage = Rectangle(width=CONTENT_WIDTH, height=self.canvas.height
            - self.header_region.height - self.caption_region.height - footer.height
            - 3 * PAGE_GAP - 0.16)
        # These native, unpainted regions define the page. Every chapter roots
        # its composition in stage; there are no independent chapter x/y anchors.
        VGroup(self.header_region, self.stage, self.caption_region, footer).arrange(
            DOWN, buff=PAGE_GAP).move_to(self.canvas)
        self.kicker = words("REAL-TIME SYSTEMS / VISUAL FIELD GUIDE", size=10,
                            color=MUTED).move_to(self.header_region, aligned_edge=UL).shift(
                                DOWN * PAGE_GAP)
        self.add(self.canvas, self.kicker, footer,
                 rule(self.header_region.get_corner(DL), self.header_region.get_corner(DR)),
                 rule(self.stage.get_corner(DL), self.stage.get_corner(DR)))
        for index in range(START_CHAPTER - 1, END_CHAPTER):
            title, duration, method = CHAPTERS[index]
            start = self.time
            self.next_section("%02d %s" % (index + 1, title))
            await self.begin_chapter(index + 1, title)
            await getattr(self, method)()
            # Scene.time is the engine's logical time. Holds make the cue sheet
            # stable while letting the native scene own every frame and seek.
            remaining = start + duration / SPEED - self.time
            if remaining < -0.001 / SPEED:
                raise ValueError("Chapter %d exceeds its authored cue by %.3f seconds" %
                                 (index + 1, -remaining * SPEED))
            if remaining > 0.000001:
                await self.wait(remaining)

    async def choices(self):
        board = await self.loop_board("SAME THREE PROGRAMS / language-neutral pseudocode / one shared CPU",
                                      states=("WAITING", "READY", "READY"))
        await self.explain("Three concurrent programs keep three independent positions.",
                           "A small pointer marks each current operation. The four work marks are qualitative progress.")
        await self.focus_loop(board, "E", 1)
        await self.watch("Select estimation. Only E's computation now advances.",
                         "The attention outline tells you where to look; the CPU strip tells you who runs.")
        await self.select(board, "E")
        await self.at_line(board, "E", 1, seconds=0.3)
        await self.work(board, "E", seconds=1.8)
        await self.notice("C and L still retain their positions.",
                          "Concurrency keeps several programs in progress. This single core executes one at a time.")
        await self.clear_focus()
        await self.at_line(board, "E", 2, seconds=0.3)
        await self.work(board, "E", 0, 2, seconds=0.9)
        await self.restore_board(board)
        choices = VGroup(
            VGroup(*card("STATE REPRESENTATION", "Thread stacks / suspended async state",
                         width=(self.stage.width - COLUMN_GAP) / 2, height=1.06, color=CONTROL)),
            VGroup(*card("SCHEDULING POLICY", "Cooperative / preemptive execution boundaries",
                         width=(self.stage.width - COLUMN_GAP) / 2, height=1.06, color=ESTIMATE)),
        ).arrange(RIGHT, buff=COLUMN_GAP).next_to(board["cpu_body"], DOWN, buff=PANEL_PADDING)
        await self.show(*(tuple(choice.submobjects) for choice in choices))
        await self.explain("State representation and scheduling are separate choices.",
                           "Async describes suspension and saved state. The surrounding runtime determines preemption.")
        await self.focus_on(choices[1][0], ESTIMATE)
        await self.notice("Keep these same three loops in view as the runtime changes.",
                          "Waiting, readiness, CPU ownership and resource ownership will remain separate facts.")


    async def memory(self):
        await self.explain(
            "Where does C keep its place while acquiring a sample?",
            "The operation is unfinished. Another program can use the CPU while C waits.",
            hold=2.0)
        operation = words("C'S CURRENT OPERATION: acquire sample", size=15,
                          color=CONTROL).next_to(self.stage.get_top(), DOWN, buff=PANEL_PADDING)
        column_gap = 2 * COLUMN_GAP
        column_width = (self.stage.width - 2 * PANEL_PADDING - column_gap) / 2
        slot_width = (column_width - 2 * COLUMN_GAP) / len(ROLES)
        storage_band_height = 1.44
        sides = {}
        for mode, heading_text, storage_height, color, cpu_title in (
                ("thread", "THREAD STACKS", storage_band_height, CONTROL,
                 "CPU USES THE SELECTED THREAD STACK"),
                ("async", "ASYNC STATE", 0.99, LOGGER,
                 "CPU USES THE ACTIVE EXECUTION STACK")):
            heading = words(heading_text, size=18, color=color)
            cells, leaves, slots = [], [], {}
            for initial, _, role_color in ROLES:
                body = panel(slot_width, storage_height, edge=role_color)
                name = words(initial + (" stack" if mode == "thread" else " state"),
                             size=13, color=role_color, width=slot_width - PANEL_PADDING)
                if mode == "thread":
                    name.next_to(body.get_top(), DOWN, buff=PAGE_GAP)
                    reserved = words("reserved", size=10, color=MUTED).next_to(
                        body.get_bottom(), UP, buff=PAGE_GAP)
                    cell = VGroup(body, name, reserved)
                else:
                    name.next_to(body, UP, buff=PAGE_GAP / 2)
                    cell = VGroup(body, name)
                cells.append(cell)
                leaves.extend(cell.submobjects)
                slots[initial] = body
            row = VGroup(*cells).arrange(RIGHT, buff=COLUMN_GAP)
            # An equal storage band aligns both CPU rows while the schematic
            # stack/state boxes keep their deliberately different heights.
            band = panel(column_width, storage_band_height, opacity=0, stroke=0)
            row.move_to(band)
            storage = VGroup(band, row)
            cpu = VGroup(*station(cpu_title, width=column_width, height=1.18))
            column = VGroup(heading, storage, cpu).arrange(
                DOWN, buff=PANEL_PADDING + PAGE_GAP)
            sides[mode] = dict(column=column, heading=heading, storage=storage,
                               slots=slots, leaves=leaves, cpu=cpu)
        thread, asynchronous = sides["thread"], sides["async"]
        comparison = VGroup(thread["column"], asynchronous["column"]).arrange(
            RIGHT, buff=column_gap).next_to(operation, DOWN, buff=2 * PAGE_GAP)
        # Create this stroke after layout so its draw animation has no parent.
        divider = Line(ORIGIN, DOWN * comparison.height, color=EDGE, stroke_width=1.5)
        divider.next_to(operation, DOWN, buff=2 * PAGE_GAP)
        await self.show(operation, thread["heading"], asynchronous["heading"],
                        divider, seconds=0.5)
        await self.show(thread["leaves"], asynchronous["leaves"],
                        tuple(thread["cpu"].submobjects),
                        tuple(asynchronous["cpu"].submobjects), seconds=0.55)

        tc, tl = job("C", CONTROL), job("L", LOGGER)
        ac, al = job("C", CONTROL), job("L", LOGGER)
        thread_run, async_run = thread["cpu"][2], asynchronous["cpu"][2]
        tc.move_to(thread_run)
        tl.move_to(thread["slots"]["L"])
        ac.move_to(async_run)
        al.move_to(asynchronous["slots"]["L"])
        # Native slot anchors describe the two orthogonal legs of each handoff.
        # Their positions follow the measured storage and execution regions.
        thread_c_turn = thread_run.copy().match_x(thread["slots"]["C"])
        thread_l_turn = thread_run.copy().match_x(thread["slots"]["L"])
        async_c_turn = async_run.copy().match_x(asynchronous["slots"]["C"])
        async_l_turn = async_run.copy().match_x(asynchronous["slots"]["L"])
        await self.show(tc, tl, ac, al, seconds=0.45)

        await self.focus_on(VGroup(thread["storage"], thread["cpu"]), color=CONTROL, buff=0.15)
        await self.watch(
            "First, C is a thread. Watch its unfinished operation stay intact.",
            "The reserved stack retains the call and the saved execution position.",
            hold=0.9)
        await self.compute(tc, 0, 1, seconds=0.6)
        await self.move_job(tc, thread_c_turn, thread["slots"]["C"], seconds=1.0)
        await self.move_job(tl, thread_l_turn, thread_run, seconds=1.0)
        await self.compute(tl, 0, 2, seconds=1.0)
        await self.notice(
            "L advances. C's saved acquire-sample operation does not.",
            "A waiting program keeps its place even while another program runs.",
            hold=2.1)

        await self.focus_on(VGroup(asynchronous["storage"], asynchronous["cpu"]),
                            color=LOGGER, buff=0.15)
        await self.explain(
            "Now perform the same handoff with async state.",
            "C retains its continuation and live data; the active stack can execute L.",
            hold=1.7)
        await self.compute(ac, 0, 1, seconds=0.6)
        await self.move_job(ac, async_c_turn, asynchronous["slots"]["C"], seconds=1.0)
        await self.move_job(al, async_l_turn, async_run, seconds=1.0)
        await self.compute(al, 0, 2, seconds=1.0)
        await self.notice(
            "The saved operation is separate from the active execution stack.",
            "Suspended futures still need storage, and the running program still uses a stack.",
            hold=2.1)

        await self.watch(
            "When C is selected again, it continues the saved operation.",
            "Follow the existing work mark; C does not restart from the beginning.",
            hold=0.8)
        await self.move_job(al, async_l_turn, asynchronous["slots"]["L"], seconds=0.95)
        await self.move_job(ac, async_c_turn, async_run, seconds=0.95)
        await self.compute(ac, 1, 2, seconds=0.7)
        await self.clear_focus()
        summary = VGroup(
            words("Same unfinished operation. Different storage organization.",
                  size=18, color=GOOD),
            words("Schematic sizes / count buffers and stack nesting / static allocation in both",
                  size=11, color=MUTED)).arrange(DOWN, buff=PAGE_GAP).next_to(
                      self.stage.get_bottom(), UP, buff=PANEL_PADDING)
        await self.show(*summary.submobjects, seconds=0.4)
        await self.notice(
            "Many waiting programs can use less reserved stack memory.",
            "Compare the complete memory budget; these boxes establish no byte ratio.",
            color=GOOD, hold=1.8)


    async def waiting(self):
        for pass_number, mode in enumerate(("BLOCKING THREADS", "ASYNC TASKS")):
            if pass_number:
                await self.hide(list(self.content), seconds=0.3)
                self.focus = None
            board = await self.loop_board(mode + " / C waits for the same sensor transfer",
                    line_indices=(1, 0, 1), states=("RUNNING", "WAITING", "READY"), owner="C")
            device = VGroup(*station("SPI / DMA", width=board["C"]["body"].width - PANEL_PADDING,
                                     height=1.0, color=ESTIMATE)).next_to(
                board["cpu_body"], DOWN, buff=PANEL_PADDING).match_x(board["C"]["body"])
            track = Line(LEFT, RIGHT, color=EDGE, stroke_width=3).stretch_to_fit_width(
                device[2].width).move_to(device[2])
            await self.show(tuple(device.submobjects), track,
                            words("DEVICE PROGRESS", size=10, color=MUTED).next_to(device, DOWN, buff=0.07))
            control_port = board["C"]["body"].get_bottom() + DOWN * 0.03
            device_port = device[0].get_top() + UP * 0.04
            issue = (control_port + LEFT * 0.70, device_port + LEFT * 0.70)
            ready = (device_port + RIGHT * 0.70, control_port + RIGHT * 0.70)
            await self.draw_route(issue, CONTROL, seconds=0.3)
            await self.draw_route(ready, ESTIMATE, seconds=0.3)
            await self.focus_loop(board, "C", 1)
            await self.explain("C starts the transfer, then releases the CPU while waiting.",
                    "A blocked thread retains its stack." if not pass_number else
                    "A suspended async task retains its continuation and live data.", hold=1.8)
            await self.pulse_data(issue, CONTROL, seconds=0.65)
            await self.set_state(board, "C", "WAITING")
            await self.select(board, "L")
            await self.clear_focus()
            await self.watch("L computes while DMA progresses independently.",
                             "C's acquire-sample pointer stays frozen. The device does not use this CPU work track.", hold=0.5)
            dma = packet(ESTIMATE).move_to(track.get_start())
            await self.show(dma, seconds=0.12)
            for i in range(3):
                await self.play_for(board["L"]["marks"][i].animate.set_fill(GOOD, opacity=1),
                        dma.animate.move_to(track.point_from_proportion((i + 1) / 3)),
                        seconds=0.5, rate_func=linear)
            await self.focus_loop(board, "C", 1)
            await self.notice("Waiting saves C's place; useful work continues elsewhere.",
                              "Both models can overlap CPU computation with independent device activity.", hold=1.8)
            await self.hide(dma, seconds=0.12)
            await self.watch("The transfer completes and sends a readiness event to C.",
                             "Follow the amber circle upward, then inspect C's state label.", hold=0.5)
            await self.pulse_data(ready, ESTIMATE, seconds=0.65)
            await self.set_state(board, "C", "READY")
            await self.notice("The completion makes C ready. L still owns the CPU.",
                              "Readiness is a scheduling input; it is not itself execution.", hold=1.8)
            await self.watch("Select C; its saved acquire-sample operation can now continue.",
                             "CPU ownership changes before C's pointer advances to the next operation.", hold=0.5)
            await self.set_state(board, "L", "READY")
            await self.select(board, "C")
            await self.clear_focus()
            await self.at_line(board, "C", 2, seconds=0.3)
            await self.work(board, "C", 0, 2, seconds=0.5)
            await self.at_line(board, "C", 3, seconds=0.3)
            await self.work(board, "C", seconds=0.6)
            await self.at_line(board, "C", 4, seconds=0.3)
            await self.notice("Only after selection does C advance beyond acquire sample.",
                              "The waiting mechanism differs; the useful CPU / device overlap is the same.", hold=1.8)
        await self.restore_board(board)


    async def await_paths(self):
        board = await self.loop_board("ONE COOPERATIVE EXECUTOR / C is ready / flush is not due in this example",
                    line_indices=(3, 0, 0), states=("READY", "WAITING", "RUNNING"), owner="L")
        queue = VGroup(*station("INPUT QUEUE", width=board["L"]["body"].width - PANEL_PADDING,
                                height=1.03, color=LOGGER)).next_to(
            board["cpu_body"], DOWN, buff=PANEL_PADDING).match_x(board["L"]["body"])
        queue_label = words("3 PACKETS READY", size=14, color=LOGGER,
                            width=queue[2].width).move_to(queue[2])
        await self.show(tuple(queue.submobjects), queue_label)
        feed = (queue[0].get_top() + UP * 0.035, board["L"]["body"].get_bottom() + DOWN * 0.03)
        await self.draw_route(feed, LOGGER, seconds=0.4)
        await self.explain("An asynchronous receive can complete immediately.",
                           "Already available input lets L continue inside the same poll; await syntax alone is not a yield.")
        await self.focus_loop(board, "L", 0)
        await self.watch("Follow L through receive, encode and the next loop iteration.",
                         "The conditional flush is skipped here because no flush is due.")
        for remaining in (2, 1, 0):
            await self.pulse_data(feed, LOGGER, seconds=0.65)
            await self.hide(queue_label, seconds=0.1)
            queue_label = words((str(remaining) + " PACKETS READY") if remaining else "EMPTY",
                                size=14, color=LOGGER if remaining else MUTED,
                                width=queue[2].width).move_to(queue[2])
            await self.show(queue_label, seconds=0.2)
            await self.clear_focus()
            await self.at_line(board, "L", 1, seconds=0.3)
            await self.work(board, "L", seconds=0.85)
            await self.at_line(board, "L", 2, seconds=0.3)
            await self.at_line(board, "L", 0, seconds=0.3)
        await self.focus_loop(board, "C", 3)
        await self.notice("C stayed ready, but its compute-command pointer did not advance.",
                          "A finite sequence of immediately ready operations can make one poll unexpectedly long.")
        await self.focus_loop(board, "L", 0)
        await self.explain("Now the empty receive must wait for new input.",
                           "Assume the wait registers a wakeup and the task returns Pending to its executor.")
        await self.set_state(board, "L", "WAITING")
        await self.select(board, "C")
        await self.focus_loop(board, "C", 3)
        await self.work(board, "C", seconds=1.4)
        await self.notice("Returning control creates an opportunity to select C.",
                          "The duration of a whole poll matters; counting await expressions does not bound it.", GOOD)


    async def trace_shell(self, subtitle):
        board = await self.loop_board(subtitle, line_indices=(3, 0, 1),
                    states=("READY", "WAITING", "RUNNING"), owner="L")
        await self.work(board, "L", 0, 1, seconds=0.2)
        # One native coordinate frame owns every quantitative position. Author
        # ticks, deadlines and intervals from n2p after the axis is laid out.
        self.trace_axis = NumberLine([0, AXIS_US, 100], length=self.stage.width - 4 * COLUMN_GAP,
                include_ticks=False, color=EDGE, stroke_width=1.5).next_to(
                    board["cpu_body"], DOWN, buff=0.535)
        # Live coordinate queries read the native frame after scene attachment.
        # Create below still owns the reveal, in this same authored cue.
        self.add(self.trace_axis)
        ticks, labels = [], {}
        for tick in (0, 100, 200, DEADLINE_US, 300, 400, 500, 600, AXIS_US):
            color = BAD if tick == DEADLINE_US else MUTED
            point = self.trace_axis.n2p(tick)
            mark = Line(point, point + 0.10 * DOWN, color=color, stroke_width=1.5)
            ticks.append(mark)
            labels[tick] = words(str(tick), size=10, color=color).next_to(
                mark, DOWN, buff=0.10)
        deadline = Line(ORIGIN, 0.47 * UP, color=BAD, stroke_width=2).next_to(
            self.trace_axis.n2p(DEADLINE_US), UP, buff=0.01)
        deadline_label = words("DEADLINE", size=9, color=BAD).next_to(
            labels[DEADLINE_US], DOWN, buff=0.12)
        units = words("µs after C release", size=9, color=MUTED).match_y(
            deadline_label).match_x(self.trace_axis, RIGHT)
        self.trace_footer = VGroup(deadline_label, units)
        cursor = Line(ORIGIN, 0.44 * UP, color=INK, stroke_width=2.7).next_to(
            self.trace_axis.n2p(0), UP, buff=0.01)
        # Keep the axis reveal and written labels at the original cue duration.
        objects = [self.trace_axis, *ticks, *labels.values(), deadline,
                   deadline_label, units, cursor]
        self.content.extend(objects)
        self.remove(self.trace_axis)
        await self.play_for(Create(self.trace_axis),
                *(Create(mark) for mark in ticks),
                *(Write(label) for label in labels.values()),
                Create(deadline), Write(deadline_label), Write(units), Create(cursor),
                seconds=0.5)
        return board, cursor


    async def trace_segment(self, cursor, begin_us, end_us, color, markers=(), seconds=None):
        start, end = self.trace_axis.n2p(begin_us), self.trace_axis.n2p(end_us)
        bar = panel(0.006, 0.28, color, color, opacity=0.87, stroke=0).next_to(
            start, UP, buff=0.09).set_x(start.x, LEFT)
        self.content.append(bar)
        self.add(bar)
        animations = [bar.animate.stretch_to_fit_width(end.x - start.x, about_edge=LEFT),
                      cursor.animate.set_x(end.x)]
        animations += [mark.animate.set_fill(GOOD, opacity=1) for mark in markers]
        await self.play_for(*animations,
                seconds=(end_us - begin_us) * 0.016 if seconds is None else seconds,
                rate_func=linear)


    async def control_execution(self, board, cursor, start):
        if board["owner"] != "C":
            raise ValueError("C must own the CPU during its response-time computation.")
        marks = board["C"]["marks"]
        step_us = CONTROL_US / len(marks)
        for i, mark in enumerate(marks):
            await self.trace_segment(cursor, start + i * step_us, start + (i + 1) * step_us,
                                     CONTROL, (mark,))
        label = words("C", size=11, color=BG, width=0.5).move_to(cursor).set_x(
            self.trace_axis.n2p(start + CONTROL_US / 2).x)
        await self.show(label, seconds=0.2)


    async def trace_result(self, example):
        blocking = example["blocking"][1]
        color = GOOD if example["met"] else BAD
        equation = "%d + %d + %d = %d µs" % (
            blocking, DISPATCH_US, CONTROL_US, example["finish"])
        outcome = ("MET · %d µs margin" % example["margin"] if example["met"]
                   else "MISSED by %d µs" % -example["margin"])
        result = VGroup(words(equation, size=18, color=color),
                        words(outcome, size=14, color=color)).arrange(
                            RIGHT, buff=0.8).next_to(self.trace_footer, DOWN, buff=0.15)
        result.match_x(self.trace_axis)
        await self.show(*result.submobjects, seconds=0.5)


    async def long_poll(self):
        board, cursor = await self.trace_shell("C: period 1,000 µs / deadline 250 µs / compute 80 µs / inputs already ready")
        await self.explain("A higher selection priority cannot interrupt a running poll.",
                           "One cooperative executor. C is released now; E is waiting. Dispatch costs 5 µs.")
        await self.focus_loop(board, "L", 1)
        await self.watch("L keeps computing. Watch the deadline and C's frozen pointer.",
                         "The remaining synchronous work lasts 500 µs; no other interference or I/O waits are included.")
        await self.trace_segment(cursor, 0, 250, LOGGER, (board["L"]["marks"][1],))
        await self.focus_loop(board, "C", 3)
        await self.notice("The deadline arrives while C has made no progress.",
                          "C is ready. L still owns the CPU. This is a blocking interval, despite low average load.", BAD)
        await self.clear_focus()
        await self.trace_segment(cursor, 250, 500, LOGGER, board["L"]["marks"][2:])
        await self.watch("L returns control. The executor can finally dispatch C.",
                         "The thin gray interval is 5 µs; the cyan computation remains 80 µs.")
        await self.set_state(board, "L", "READY")
        await self.select(board, None)
        await self.trace_segment(cursor, 500, 505, MUTED)
        await self.select(board, "C")
        await self.restore_board(board)
        await self.control_execution(board, cursor, 505)
        await self.trace_result(response_example(500))
        await self.notice("C finishes at 585 µs, missing its deadline by 335 µs.",
                          "Async may reduce overhead, yet an unbounded poll can dominate response time.", BAD)


    async def bounded_poll(self):
        board, cursor = await self.trace_shell("SAME C / same 250 µs deadline / same 80 µs computation / 5 µs dispatch")
        await self.explain("Bound the cooperative work, then account for task selection.",
                           "Assume at most 50 µs of remaining relevant poll work, and that C is selected next.")
        await self.focus_loop(board, "L", 1)
        await self.watch("L completes a bounded chunk and yields with work unfinished.",
                         "Its pointer and partial work marks retain the continuation point.")
        await self.trace_segment(cursor, 0, 50, LOGGER, (board["L"]["marks"][1],), seconds=1.5)
        await self.set_state(board, "L", "READY")
        await self.select(board, None)
        await self.notice("L is ready to resume later. C can now be selected.",
                          "A saved continuation does not have to mean waiting for a device.")
        await self.trace_segment(cursor, 50, 55, MUTED, seconds=0.45)
        await self.select(board, "C")
        await self.focus_loop(board, "C", 3)
        await self.control_execution(board, cursor, 55)
        await self.trace_result(response_example(50))
        await self.notice("C finishes at 135 µs, with 115 µs of margin.",
                          "The useful control arithmetic is unchanged. It starts sooner because blocking is bounded.", GOOD)
        await self.restore_board(board)
        await self.explain("The next-selection assumption is part of this result.",
                           "Other ready work ahead of C would add interference. Frequent yielding also adds coordination cost.")
        await self.notice("Cooperative execution can satisfy a deadline under complete bounds.",
                          "Bound poll length, wakeup latency, queueing, resources and all higher-priority interference.", GOOD)


    async def preemption(self):
        board, cursor = await self.trace_shell("PREEMPTIVE PRIORITIES / same workload / L can be interrupted inside encode + buffer")
        await self.explain("Keep the long computation; allow a higher-priority boundary to preempt it.",
                           "Here 10 µs bounds all applicable lower-priority non-preemptible and resource blocking.")
        await self.focus_loop(board, "L", 1)
        await self.watch("L is stopped partway through the current operation.",
                         "A pseudocode line is not atomic and is not a fixed unit of execution time.")
        await self.trace_segment(cursor, 0, 10, LOGGER, seconds=1.2)
        await self.set_state(board, "L", "PREEMPTED")
        await self.select(board, None)
        await self.notice("L keeps the same line and the same partial progress.",
                          "It did not need to finish the operation, yield, or reach an asynchronous suspension.")
        await self.trace_segment(cursor, 10, 15, MUTED, seconds=0.45)
        await self.select(board, "C")
        await self.focus_loop(board, "C", 3)
        await self.control_execution(board, cursor, 15)
        await self.trace_result(response_example(10))
        await self.notice("C finishes at 95 µs, with 155 µs of margin.",
                          "These given bounds illustrate response time; they are not measured worst-case execution times.", GOOD)
        await self.set_state(board, "C", "COMPLETE")
        await self.select(board, "L")
        await self.focus_loop(board, "L", 1)
        await self.watch("Resume L from its saved point inside encode + buffer.",
                         "The response-time trace ends at C's finish. This resumption is a qualitative illustration.")
        await self.work(board, "L", 1, 4, seconds=2.8)
        await self.notice("L continues the original operation; it does not restart it.",
                          "Preemptive threads and priority-separated async executors can both provide this boundary.")
        await self.restore_board(board)
        await self.explain("Preemption reduces some blocking; resource dependencies still matter.",
                           "Interrupt masking, device transactions and higher-priority execution still require explicit bounds.")


    async def architectures(self):
        modes = (
            ("ONE COOPERATIVE EXECUTOR", "One executor boundary contains C, E and L.",
             "L must return control before the executor can select C.", False),
            ("ASYNC AT DISTINCT PREEMPTIVE PRIORITIES", "Separate execution boundaries can preempt one another.",
             "For example, distinct Cortex-M interrupt executors can give async work hardware priorities.", True),
            ("RTOS THREADS + BACKGROUND ASYNC", "An RTOS can schedule threads around an async executor.",
             "C and E use priority threads; L represents background async work inside a lower-priority thread.", True),
        )
        for index, (title, explanation, detail, preemptive) in enumerate(modes):
            if index:
                await self.hide(list(self.content), seconds=0.3)
                self.focus = None
            board = await self.loop_board(title, line_indices=(3, 1, 1),
                    states=("READY", "READY", "RUNNING"), owner="L")
            frames = []
            if not preemptive:
                frames.append(SurroundingRectangle(board["programs"], buff=0.11,
                              color=GOOD, stroke_width=1.7, fill_opacity=0))
            else:
                for letter, _, color in ROLES:
                    frames.append(SurroundingRectangle(board[letter]["body"], buff=0.11,
                                  color=color, stroke_width=1.7, fill_opacity=0))
            labels = (("task", "task", "task") if not preemptive else
                      ("high-priority executor", "medium-priority executor", "background executor")
                      if index == 1 else ("high-priority thread", "medium-priority thread", "thread: async executor"))
            await self.show(frames, *[words(label, size=12, color=color,
                    width=board[letter]["body"].width).next_to(
                    board["cpu_body"], DOWN, buff=PANEL_PADDING).match_x(board[letter]["body"])
                    for label, (letter, _, color) in zip(labels, ROLES)],
                    words("ONE CORE / scheduling changes ownership, never the number of CPUs",
                          size=12, color=MUTED).next_to(self.stage.get_bottom(), UP,
                                                       buff=COLUMN_GAP), seconds=0.5)
            await self.explain(explanation, detail, hold=1.8)
            await self.focus_loop(board, "L", 1)
            await self.work(board, "L", 0, 2 if preemptive else 4, seconds=1.3)
            await self.watch("Preempt L inside its operation." if preemptive else
                             "Let L return control after its current poll.",
                             "C has the highest illustrated priority." if preemptive else
                             "Higher selection priority alone cannot interrupt this poll.", hold=0.5)
            await self.set_state(board, "L", "PREEMPTED" if preemptive else "READY")
            await self.select(board, "C")
            await self.focus_loop(board, "C", 3)
            await self.work(board, "C", 0, 2, seconds=1.0)
            await self.notice("The same C loop runs under a different scheduling boundary.",
                              "E remains ready, with its position saved; one shared CPU still has only one owner.", hold=1.8)


    async def performance(self):
        def metric(title, claim, qualifier, color):
            body = panel((self.stage.width - COLUMN_GAP) / 2, 1.25)
            content_width = body.width - 2 * PANEL_PADDING
            labels = VGroup(words(title, size=13, color=color, width=content_width),
                            words(claim, size=17, width=content_width),
                            words(qualifier, size=11, color=MUTED, width=content_width))
            labels.arrange(DOWN, buff=0.15).move_to(body)
            return VGroup(body, *labels.submobjects)

        operation = VGroup(*card("E / update estimate", "The same numerical algorithm in both designs",
                                 width=self.stage.width, height=0.82, color=ESTIMATE))
        metrics = VGroup(
            metric("RAM", "Saved state + stacks + buffers",
                   "Possible reduction; count the complete budget", CONTROL),
            metric("COORDINATION", "Wait / wake / dispatch",
                   "Measure the selected runtime and drivers", LOGGER),
            metric("COMPUTATION", "The same estimate update",
                   "Async does not remove the arithmetic", ESTIMATE),
            metric("DEADLINE RESPONSE", "When the result becomes usable",
                   "Bound blocking and interference", GOOD),
        ).arrange_in_grid(rows=2, cols=2, buff=(COLUMN_GAP, 0.29))
        summary = words("Equivalent work, drivers, allocation rules and timing obligations.",
                        size=14, color=GOOD)
        VGroup(operation, metrics, summary).arrange(DOWN, buff=COLUMN_GAP).move_to(self.stage)

        await self.explain(
            "Which performance cost could this workload improve?",
            "Keep E's update-estimate operation and the useful workload the same.",
            hold=2.3)
        await self.show(tuple(operation.submobjects), seconds=0.55)

        await self.show(tuple(metrics[0].submobjects), seconds=0.45)
        await self.focus_on(metrics[0][0], color=CONTROL)
        await self.notice(
            "RAM can improve when many programs mostly wait.",
            "Count retained state, active stacks, buffers and queues in both designs.",
            hold=2.2)

        await self.explain(
            "Coordination has a separate implementation cost.",
            "Waiting, waking and selecting a program all consume CPU work.",
            hold=1.2)
        await self.show(tuple(metrics[1].submobjects), seconds=0.45)
        await self.focus_on(metrics[1][0], color=LOGGER)
        await self.notice(
            "A smaller coordination path can help; more yields also add work.",
            "The runtime, driver and synchronization choices determine the actual cost.",
            hold=1.9)

        await self.focus_on(operation[0], color=ESTIMATE)
        await self.watch(
            "Keep E's numerical operation unchanged.",
            "Changing its execution wrapper does not remove the estimate calculation.",
            hold=0.8)
        await self.show(tuple(metrics[2].submobjects), seconds=0.45)
        await self.pause(1.4)

        await self.explain(
            "Now ask when C's result becomes usable.",
            "Response includes blocking, dispatch, execution and the dependencies that exist.",
            hold=1.4)
        await self.show(tuple(metrics[3].submobjects), seconds=0.45)
        await self.focus_on(metrics[3][0], color=GOOD)
        await self.notice(
            "Earlier access to the CPU can improve deadline response.",
            "The controller's opportunity to execute can change while its arithmetic stays the same.",
            hold=1.9)
        await self.clear_focus()
        await self.show(summary, seconds=0.4)
        await self.notice(
            "State the metric and compare equally capable designs.",
            "Measure RAM, throughput, energy and jitter; justify the required timing bounds.",
            color=GOOD, hold=2.5)



    async def inversion(self):
        board = await self.loop_board("PRIORITIES: C HIGH / E MEDIUM / L LOW / L initially owns the shared SPI resource",
                    line_indices=(1, 1, 2), states=("READY", "READY", "RUNNING"), owner="L")
        spi = VGroup(*station("SHARED SPI", width=board["L"]["body"].width - PANEL_PADDING,
                              height=1.05, color=LOGGER)).next_to(
            board["cpu_body"], DOWN, buff=PANEL_PADDING).match_x(board["L"]["body"])
        owner_label = words("OWNER: L", size=18, color=LOGGER,
                            width=spi[2].width).move_to(spi[2])
        await self.show(tuple(spi.submobjects), owner_label)
        start = board["C"]["body"].get_bottom() + DOWN * 0.03
        end = spi[0].get_left() + LEFT * 0.06
        request = (start, (start.x, end.y), end)
        await self.draw_route(request, CONTROL, seconds=0.6)
        await self.show(rule(board["L"]["body"].get_bottom() + DOWN * 0.03,
                             spi[0].get_top() + UP * 0.015, LOGGER, 1.6),
                        words("resource ownership", size=11, color=MUTED).next_to(spi, DOWN, buff=0.08))
        await self.work(board, "L", 0, 1, seconds=0.7)
        await self.explain("C can preempt L, but it cannot use SPI until L releases it.",
                           "Expand L's flush-output operation: it holds this resource while doing protected work.")
        await self.set_state(board, "L", "READY")
        await self.select(board, "C")
        await self.pulse_data(request, CONTROL, seconds=1.2)
        await self.set_state(board, "C", "WAITING")
        await self.select(board, "E")
        await self.focus_loop(board, "E", 1)
        await self.watch("C blocks on SPI. E can run ahead of the low-priority owner L.",
                         "Watch E's work marks advance while L's flush operation stays frozen.")
        await self.work(board, "E", 0, 2, seconds=1.8)
        await self.focus_on(spi[0], LOGGER)
        await self.notice("CPU owner: E. SPI owner: L. C waits for L's progress.",
                          "This indirect delay is priority inversion; the highest-priority task can still be blocked.", BAD)
        boost = VGroup(*badge("L inherits C's effective priority", GOOD)).move_to(
            self.stage, aligned_edge=DL).shift(UP * PAGE_GAP)
        await self.explain("Now add priority inheritance: raise L to C's effective priority.",
                           "This is generic priority inheritance, not a property supplied by every async mutex.")
        await self.show(tuple(boost.submobjects), seconds=0.35)
        await self.set_state(board, "E", "READY")
        await self.select(board, "L")
        await self.focus_loop(board, "L", 2)
        await self.work(board, "L", 1, 4, seconds=2.0)
        await self.hide(owner_label, tuple(boost.submobjects), seconds=0.25)
        free_label = words("FREE", size=18, color=GOOD, width=spi[2].width).move_to(spi[2])
        await self.show(free_label, seconds=0.3)
        await self.set_state(board, "C", "READY")
        await self.notice("L releases SPI. C becomes ready to acquire it.",
                          "The dependency is resolved by the resource owner's progress, not by changing C's syntax.")
        await self.set_state(board, "L", "READY")
        await self.select(board, "C")
        await self.focus_loop(board, "C", 1)
        await self.pulse_data(request, CONTROL, seconds=1.0)
        await self.hide(free_label, seconds=0.15)
        await self.show(words("OWNER: C", size=18, color=CONTROL,
                             width=spi[2].width).move_to(spi[2]), seconds=0.3)
        await self.work(board, "C", 0, 2, seconds=0.7)
        await self.notice("C now acquires SPI and continues.",
                          "Bound lock hold time, device transactions and service queues as well as CPU execution.")


    async def timeouts(self):
        board = await self.loop_board("TRIAL 1 / a cooperative timeout wrapper surrounds L's inner operation",
                    line_indices=(0, 0, 1), states=("WAITING", "WAITING", "RUNNING"), owner="L")
        timer_panel = VGroup(*station("TIMEOUT TIMER", width=board["L"]["body"].width - PANEL_PADDING,
                                      height=1.07, color=BAD)).next_to(
            board["cpu_body"], DOWN, buff=PANEL_PADDING).match_x(board["L"]["body"])
        timer = words("ARMED", size=17, color=MUTED,
                      width=timer_panel[2].width).move_to(timer_panel[2])
        await self.show(tuple(timer_panel.submobjects), timer)
        await self.explain("A timeout can become ready while its inner operation still runs.",
                           "The wrapper cannot inspect the timer while a synchronous inner call holds control.")
        await self.focus_loop(board, "L", 1)
        await self.work(board, "L", 0, 1, seconds=0.8)
        await self.hide(timer, seconds=0.15)
        await self.show(words("READY", size=17, color=BAD,
                             width=timer_panel[2].width).move_to(timer_panel[2]), seconds=0.3)
        await self.focus_on(timer_panel[0], BAD)
        await self.notice("The timer is ready. L still owns the CPU.",
                          "An async timeout does not itself enforce a CPU execution budget.", BAD, hold=2.0)
        await self.work(board, "L", 1, 4, seconds=1.7)
        await self.set_state(board, "L", "READY")
        await self.select(board, None)
        await self.notice("Only after the inner call returns can the wrapper check the timer.",
                          "A schedulable monitor or watchdog needs its own enforcement boundary and timing analysis.", hold=2.0)
        await self.hide(list(self.content), seconds=0.35)
        self.focus = None
        route_gap = 3 * COLUMN_GAP
        node_width = (self.stage.width - 2 * route_gap) / 3
        caller = VGroup(*station("CALLER", width=node_width, height=1.2, color=CONTROL))
        driver = VGroup(*station("DRIVER", width=node_width, height=1.2, color=MUTED))
        device_node = VGroup(*station("DEVICE", width=node_width, height=1.2, color=ESTIMATE))
        trial_label = words("TRIAL 2 / cancellation after an external command has started",
                            size=12, color=MUTED).next_to(self.stage.get_top(), DOWN,
                                                        buff=PANEL_PADDING)
        command_path = VGroup(caller, driver, device_node).arrange(RIGHT, buff=route_gap).next_to(
            trial_label, DOWN, buff=2 * COLUMN_GAP)
        await self.show(trial_label,
                        *(tuple(node.submobjects) for node in command_path))
        caller_state = words("REQUEST ACTIVE", size=14, color=CONTROL,
                             width=caller[2].width).move_to(caller[2])
        device_state = words("IDLE", size=14, color=MUTED,
                             width=device_node[2].width).move_to(device_node[2])
        await self.show(caller_state, device_state,
                        words("operation in flight", size=14, color=MUTED,
                              width=driver[2].width).move_to(driver[2]))
        issue = (caller[0].get_right() + RIGHT * 0.04, driver[0].get_left() + LEFT * 0.04)
        device = (driver[0].get_right() + RIGHT * 0.04, device_node[0].get_left() + LEFT * 0.04)
        await self.draw_route(issue, CONTROL, seconds=0.35)
        await self.draw_route(device, ESTIMATE, seconds=0.35)
        await self.explain("Cancellation has to account for effects already outside the caller.",
                           "Follow one command from caller to driver to the physical device.", hold=1.8)
        await self.pulse_data(issue, CONTROL, seconds=0.8)
        await self.pulse_data(device, ESTIMATE, seconds=0.8)
        await self.hide(device_state, seconds=0.15)
        await self.show(words("COMMAND ISSUED", size=14, color=ESTIMATE,
                             width=device_node[2].width).move_to(device_node[2]), seconds=0.3)
        await self.watch("The caller cancels after the driver has issued the command.")
        await self.hide(caller_state, seconds=0.15)
        await self.show(words("REQUEST CANCELLED", size=14, color=BAD,
                             width=caller[2].width).move_to(caller[2]), seconds=0.3)
        await self.focus_on(device_node[0], ESTIMATE)
        await self.notice("Cancelling the caller does not rewind the device.",
                          "The command may already be acting on the physical system.", BAD, hold=2.0)
        contract = VGroup(*card("DRIVER + PROTOCOL CONTRACT", "Specify abort, completion, cleanup and resource lifetimes",
                                width=self.stage.width - 2 * PANEL_PADDING, height=1.2,
                                color=GOOD)).next_to(command_path, DOWN, buff=2 * COLUMN_GAP)
        await self.show(tuple(contract.submobjects))
        await self.notice("Define the actual cancellation contract before relying on it.",
                          "Immediate abort, deferred cleanup and completion have different semantics and timing costs.")


    async def assurance(self):
        await self.explain(
            "Could C finish on time and still use an invalid input?",
            "Consider valid snapshot storage whose timestamp violates C's input-age policy.",
            hold=2.1)
        route_gap = 5 * COLUMN_GAP
        card_width = (self.stage.width - route_gap) / 2
        snapshot = VGroup(*card("STATE SNAPSHOT", "Age: 12 ms / allowed: 5 ms",
                                width=card_width, height=1.38, color=ESTIMATE))
        consumer = VGroup(*card("C / read state snapshot", "A valid memory access can still read stale data",
                                width=card_width, height=1.38, color=CONTROL))
        policy = words("ILLUSTRATIVE INPUT-AGE POLICY", size=13, color=MUTED).next_to(
            self.stage.get_top(), DOWN, buff=2 * COLUMN_GAP)
        inputs = VGroup(snapshot, consumer).arrange(RIGHT, buff=route_gap).next_to(
            policy, DOWN, buff=COLUMN_GAP)
        example = (
            policy,
            tuple(snapshot.submobjects), tuple(consumer.submobjects),
            words("valid storage and lifetime", size=14, color=GOOD,
                  width=snapshot[0].width).next_to(snapshot, DOWN, buff=0.32),
            words("timely computation is still possible", size=14, color=MUTED,
                  width=consumer[0].width).next_to(consumer, DOWN, buff=0.32),
        )
        await self.show(example, seconds=0.55)
        await self.focus_on(snapshot[0], color=ESTIMATE)
        await self.watch(
            "Watch the snapshot's age when C reads it.",
            "Twelve milliseconds exceeds this example's five-millisecond limit.",
            hold=0.8)
        age_path = (snapshot[0].get_right() + RIGHT * 0.05, consumer[0].get_left() + LEFT * 0.05)
        age_route = await self.draw_route(age_path, ESTIMATE, seconds=0.45)
        await self.pulse_data(age_path, ESTIMATE, seconds=1.1)
        stale = VGroup(*badge("AGE LIMIT EXCEEDED", BAD)).next_to(example[-1], DOWN, buff=0.30)
        await self.show(tuple(stale.submobjects), seconds=0.35)
        await self.notice(
            "Valid memory does not establish valid control input.",
            "A timely task can still violate its required data-age contract.",
            color=BAD, hold=2.1)
        await self.clear_focus()
        await self.hide(example, age_route, tuple(stale.submobjects), seconds=0.35)

        await self.explain(
            "Name the property that the design actually guarantees.",
            "These four questions require different evidence and explicit assumptions.",
            hold=1.7)
        rows = (
            ("MEMORY", "Are accesses and lifetimes valid?",
             "Language, sound APIs and isolation", CONTROL,
             "Start with valid access and lifetime rules."),
            ("RESOURCES", "Who can block whom?",
             "Ownership and enforced resource protocols", LOGGER,
             "Then identify resource ownership and waiting dependencies."),
            ("TIMING", "Will required results arrive in time?",
             "Execution, blocking, interference and protection", ESTIMATE,
             "Establish timing bounds under the actual execution model."),
            ("SYSTEM", "Is the physical response correct?",
             "Data age, requirements, fault handling and verification", GOOD,
             "Verify the required response of the complete physical system."),
        )
        layers = []
        name_width = max(words(row[0], size=15).width for row in rows) + 2 * PANEL_PADDING
        for title, question, evidence, color, _ in rows:
            body = panel(self.stage.width, 0.93)
            accent = panel(0.07, body.height, color, color, stroke=0).match_x(body, LEFT)
            name_slot = Rectangle(width=name_width, height=body.height - 2 * PANEL_PADDING,
                                  stroke_width=0, fill_opacity=0)
            detail_slot = Rectangle(width=body.width - name_slot.width - COLUMN_GAP - 2 * PANEL_PADDING,
                                    height=name_slot.height, stroke_width=0, fill_opacity=0)
            VGroup(name_slot, detail_slot).arrange(RIGHT, buff=COLUMN_GAP).move_to(body)
            name = words(title, size=15, color=color, width=name_slot.width).move_to(
                name_slot, aligned_edge=LEFT)
            details = VGroup(words(question, size=17, width=detail_slot.width),
                             words(evidence, size=12, color=MUTED, width=detail_slot.width))
            details.arrange(DOWN, buff=0.15, aligned_edge=LEFT).move_to(detail_slot, aligned_edge=LEFT)
            layers.append(VGroup(body, accent, name, *details.submobjects))
        VGroup(*layers).arrange(DOWN, buff=PANEL_PADDING).move_to(self.stage)
        for layer, (_, _, _, color, explanation) in zip(layers, rows):
            await self.watch(explanation, hold=0.6)
            await self.show(tuple(layer.submobjects), seconds=0.45)
            await self.focus_on(layer[0], color=color)
            await self.pause(0.9)
        await self.clear_focus()
        await self.notice(
            "Each guarantee needs a stated property and assumptions.",
            "Apply these contracts to threads, async tasks or a hybrid.",
            color=GOOD, hold=1.5)


    async def control_boundary(self):
        # Arrange native node families before deriving any connections. The
        # shared grid measures their bounds; only panel sizes and gaps are style.
        route_gap = 3 * COLUMN_GAP
        node_size = {"width": (self.stage.width - 2 * route_gap) / 3, "height": 1.45}
        sensor = VGroup(*card("SENSOR CAPTURE", "Timestamp / DMA", color=ESTIMATE, **node_size))
        control = VGroup(*station("C / HIGH-PRIORITY CONTROL", color=CONTROL, **node_size))
        actuator = VGroup(*card("ACTUATOR", "Bound command latency", color=CONTROL, **node_size))
        logger = VGroup(*station("L / BACKGROUND ASYNC", color=LOGGER, **node_size))
        estimator = VGroup(*station("E / ESTIMATION", color=ESTIMATE, **node_size))
        monitor = VGroup(*card("MONITOR / PROTECTION", "Independent fault path", color=GOOD, **node_size))
        nodes = VGroup(sensor, control, actuator, logger, estimator, monitor)
        nodes.arrange_in_grid(rows=2, cols=3, buff=(route_gap, 1.35)).move_to(self.stage)
        sensor_body, control_body, actuator_body, logger_body, estimator_body, monitor_body = (
            node.submobjects[0] for node in nodes.submobjects)
        # Reveal the native leaves so headings still write on instead of fading
        # an entire family in as a single visual block.
        await self.show([tuple(node.submobjects) for node in nodes.submobjects], seconds=0.6)

        c = job("C", CONTROL).move_to(control[2])
        e = job("E", ESTIMATE).move_to(estimator[2])
        l = job("L", LOGGER)
        queue = [panel(0.33, 0.27, PANEL, LOGGER) for _ in range(3)]
        queue_cells = VGroup(*queue).arrange(RIGHT, buff=0.14)
        queue_label = words("log queue", size=10, color=MUTED, width=queue_cells.width)
        queue_contents = VGroup(queue_cells, queue_label).arrange(DOWN, buff=0.17)
        VGroup(l, queue_contents).arrange(RIGHT, buff=PANEL_PADDING).move_to(logger[2])
        await self.show(c, e, l, queue, queue_label, seconds=0.4)
        await self.explain(
            "Where does the end-to-end control deadline end?",
            "Trace one sample through shared-CPU computation to the physical actuator.",
            hold=2.3)

        # Wiring expresses this diagram's topology, not a layout engine. Ports
        # come from the arranged bodies; drawing and moving signals reuse the
        # same routes. The log copy has a reserved left-hand interrow corridor.
        port_gap = 0.05
        sample_route = (sensor_body.get_right() + RIGHT * port_gap,
                        control_body.get_left() + LEFT * port_gap)
        command_route = (control_body.get_right() + RIGHT * port_gap,
                         actuator_body.get_left() + LEFT * port_gap)
        snapshot_route = (estimator_body.get_top() + UP * port_gap,
                          control_body.get_bottom() + DOWN * port_gap)
        fault_route = (monitor_body.get_top() + UP * port_gap,
                       actuator_body.get_bottom() + DOWN * port_gap)
        log_start = control_body.get_bottom() + LEFT * (control_body.width * 0.32) + DOWN * port_gap
        log_end = logger_body.get_top() + UP * port_gap
        corridor_y = (control_body.get_bottom()[1] + logger_body.get_top()[1]) / 2
        log_route = (log_start, (log_start[0], corridor_y), (log_end[0], corridor_y), log_end)

        await self.focus_on(sensor_body, color=ESTIMATE)
        await self.watch(
            "Start with a timestamped sensor sample.",
            "Acquisition and delivery count before control can use the data.",
            hold=1.1)
        await self.show(words("sample", size=10, color=ESTIMATE, width=route_gap).next_to(
            (sample_route[0] + sample_route[1]) / 2, UP, buff=0.25), seconds=0.3)
        await self.draw_route(sample_route, ESTIMATE, seconds=0.5)
        await self.pulse_data(sample_route, ESTIMATE, seconds=1.2)
        await self.notice(
            "The sample arrived; C still needs a usable state snapshot.",
            "Arrival alone does not establish freshness or consistency.",
            hold=1.8)

        await self.focus_on(estimator_body, color=ESTIMATE)
        await self.explain(
            "E publishes its next coherent snapshot.",
            "Only E's illustrated computation advances during this CPU turn.",
            hold=1.8)
        await self.compute(e, 0, 4, seconds=1.8)
        await self.draw_route(snapshot_route, ESTIMATE, seconds=0.45)
        snapshot_labels = VGroup(
            words("coherent snapshot", size=11, color=ESTIMATE, width=estimator[2].width),
            words("timestamp + age", size=11, color=MUTED, width=estimator[2].width),
        ).arrange(DOWN, buff=0.20, aligned_edge=LEFT).next_to(
            (snapshot_route[0] + snapshot_route[1]) / 2, RIGHT, buff=0.25)
        await self.show(tuple(snapshot_labels.submobjects), seconds=0.3)
        await self.pulse_data(snapshot_route, ESTIMATE, seconds=1.0)
        await self.notice(
            "C can now read a coherent snapshot with an allowed age.",
            "Specify the age policy separately from memory ownership.",
            hold=1.4)

        await self.focus_on(control_body, color=CONTROL)
        await self.explain(
            "C gets the CPU and computes the command.",
            "E and L retain their positions while C advances.",
            hold=1.5)
        await self.compute(c, 0, 4, seconds=1.8)
        await self.show(words("command", size=10, color=CONTROL, width=route_gap).next_to(
            (command_route[0] + command_route[1]) / 2, UP, buff=0.25), seconds=0.3)
        await self.draw_route(command_route, CONTROL, seconds=0.45)
        await self.pulse_data(command_route, CONTROL, seconds=1.0)
        applied = words("command applied", size=11, color=GOOD,
                        width=actuator_body.width - 2 * PANEL_PADDING).next_to(
            actuator_body.get_bottom(), UP, buff=0.15)
        await self.show(applied, seconds=0.3)
        await self.notice(
            "The end-to-end path includes applying the actuator command.",
            "Finishing C's arithmetic is only one part of that path.",
            hold=1.6)

        await self.focus_on(logger_body, color=LOGGER)
        await self.explain(
            "A log copy enters a bounded background queue.",
            "The full-queue policy must preserve the control contract.",
            hold=1.3)
        await self.draw_route(log_route, LOGGER, seconds=0.5)
        await self.show(words("bounded log queue", size=11, color=LOGGER, width=logger_body.width).next_to(
            ((log_start[0] + log_end[0]) / 2, corridor_y), UP, buff=0.20), seconds=0.3)
        await self.pulse_data(log_route, LOGGER, seconds=1.3)
        await self.play_for(queue[1].animate.set_fill(LOGGER, opacity=1), seconds=0.3)
        await self.pause(0.8)
        await self.watch(
            "L services that queue on a later CPU turn.",
            "Its progress starts after the control work shown above has finished.",
            hold=0.5)
        await self.compute(l, 0, 2, seconds=1.0)
        await self.play_for(queue[1].animate.set_fill(PANEL, opacity=1), seconds=0.3)
        await self.notice(
            "Queue capacity and overflow behavior are explicit.",
            "Bound storage, waiting and service time.",
            hold=0.9)

        await self.focus_on(monitor_body, color=GOOD)
        await self.explain(
            "A detected fault follows a separate containment contract.",
            "The monitor needs a schedulable enforcement boundary and platform evidence.",
            hold=1.5)
        await self.draw_route(fault_route, GOOD, seconds=0.45)
        fault_labels = VGroup(
            words("detect", size=11, color=GOOD, width=monitor_body.width / 2 - PANEL_PADDING),
            words("contain", size=11, color=GOOD, width=monitor_body.width / 2 - PANEL_PADDING),
        ).arrange(DOWN, buff=0.20, aligned_edge=LEFT).next_to(
            (fault_route[0] + fault_route[1]) / 2, RIGHT, buff=0.35)
        await self.show(tuple(fault_labels.submobjects), seconds=0.3)
        await self.pulse_data(fault_route, GOOD, seconds=1.0)
        await self.hide(applied, seconds=0.2)
        await self.show(words("specified fault response", size=10, color=GOOD,
                             width=actuator_body.width - 2 * PANEL_PADDING).next_to(
            actuator_body.get_bottom(), UP, buff=0.15), seconds=0.3)
        await self.notice(
            "The monitor's fault response has its own bounds.",
            "Scheduling urgency and fault containment are separate design obligations.",
            hold=1.0)
        await self.clear_focus()
        await self.show(
            words("Choose execution boundaries to match urgency and containment needs.",
                  size=12, color=GOOD, width=self.stage.width).next_to(nodes, UP, buff=PAGE_GAP), seconds=0.4)
        await self.notice(
            "Prove execution, blocking and data-age bounds.",
            "Async, threads or a hybrid can fit when their concrete contracts support the system.",
            color=GOOD, hold=0.1)
