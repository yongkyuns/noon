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


def words(source, x=0, y=0, size=20, color=INK, width=12.7, left=False):
    obj = Text(str(source), font="DejaVu Sans Mono", font_size=size, color=color)
    if obj.width > width:
        obj.scale(width / obj.width)
    obj.move_to((x + obj.width / 2 if left else x, y, 0))
    return obj


def panel(x, y, width, height, fill=PANEL, edge=EDGE, opacity=1, stroke=1):
    return Rectangle(width=width, height=height, color=edge,
                     fill_opacity=opacity, stroke_width=stroke).set_fill(
                         color=fill, opacity=opacity).move_to((x, y, 0))


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


def card(title, detail, x, y, width=3.5, height=1.0, color=CONTROL):
    objects = [panel(x, y, width, height),
               panel(x - width / 2 + 0.025, y, 0.05, height, color, color, stroke=0)]
    if detail:
        objects += [words(title, x, y + 0.16, 19, color, width - 0.3),
                    words(detail, x, y - 0.22, 13, MUTED, width - 0.3)]
    else:
        objects.append(words(title, x, y, 19, color, width - 0.3))
    return tuple(objects)


def badge(text, x, y, color=CONTROL, width=1.7):
    return (panel(x, y, width, 0.42, RAISED, color),
            words(text, x, y, 13, color, width - 0.14))


def station(title, x, y, width=3.0, height=1.45, color=INK):
    return (panel(x, y, width, height),
            words(title, x, y + height / 2 - 0.21, 13, color, width - 0.25))


def job(letter, x, y, color, completed=0):
    """A native family: task identity plus qualitative completed-work markers.

    Markers travel with the task and stop changing outside the CPU. They are
    not percentages or instruction counts; the microsecond axis carries time.
    """
    body = panel(x, y, 0.98, 0.70, RAISED, color, stroke=2)
    name = words(letter, x, y + 0.11, 21, color, 0.7)
    marks = [panel(x + (i - 1.5) * 0.185, y - 0.205, 0.125, 0.105,
                   GOOD if i < completed else EDGE, EDGE, stroke=0)
             for i in range(4)]
    return VGroup(body, name, *marks)


def packet(x, y, color=ESTIMATE):
    """Transport is a glowing circular signal, distinct from program progress."""
    dot = Circle(radius=0.075, color=color, stroke_width=0).set_fill(
        color=color, opacity=1).move_to((x, y, 0))
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

    async def move_job(self, token, *points, seconds=1.1):
        # These are ordinary native family transforms, one per activation.
        # Python supplies authored destinations, never per-frame interpolation.
        for point in points:
            await self.play_for(token.animate.move_to((*point, 0)),
                                seconds=seconds / len(points))

    async def compute(self, token, begin=0, end=4, seconds=2.0):
        for index in range(begin, end):
            await self.play_for(token.submobjects[index + 2].animate.set_fill(GOOD, opacity=1),
                                seconds=seconds / (end - begin), rate_func=linear)

    async def pulse_data(self, points, color=ESTIMATE, seconds=1.2):
        dot = packet(*points[0], color)
        await self.show(dot, seconds=0.12)
        lengths = [sqrt((b[0] - a[0]) ** 2 + (b[1] - a[1]) ** 2)
                   for a, b in zip(points, points[1:])]
        total = sum(lengths)
        for point, length in zip(points[1:], lengths):
            # Distances size the authored segments. Native smooth interpolation
            # eases into each orthogonal turn; there is no per-frame callback.
            await self.play_for(dot.animate.move_to((*point, 0)),
                                seconds=seconds * length / total, rate_func=smooth)
        await self.hide(dot, seconds=0.12)

    async def hide(self, *items, seconds=0.3):
        objects = authored_objects(*items)
        await self.play_for(*(FadeOut(obj) for obj in objects), seconds=seconds)
        removed = {id(obj) for obj in objects}
        self.content = [obj for obj in self.content if id(obj) not in removed]

    async def caption(self, main, detail="", color=INK, phase="EXPLAIN"):
        phase_color = {"EXPLAIN": MUTED, "WATCH": ESTIMATE, "NOTICE": GOOD}[phase]
        new = [words(main, y=-3.08, size=20, color=color),
               words(detail or " ", y=-3.48, size=13, color=MUTED),
               words(phase, -6.47, -2.80, 9, phase_color, left=True)]
        # Remove the old sentence before writing the next, so no transition
        # frame superimposes two different explanations in the same space.
        if self.captions:
            await self.play_for(*(FadeOut(obj) for obj in self.captions), seconds=0.13)
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

    async def focus_rect(self, x, y, width, height, color=CONTROL):
        await self.clear_focus()
        # An attention cue, independent of every program's execution pointer.
        self.focus = RoundedRectangle(width=width, height=height, corner_radius=0.07,
                color=color, stroke_width=1.5).set_fill(color, opacity=0.055).set_stroke(
                    color, opacity=0.8).move_to((x, y, 0))
        await self.show(self.focus, seconds=0.35)

    async def loop_board(self, subtitle, positions=(0, 0, 0),
                         states=("WAITING", "READY", "READY"), owner=None):
        """Author three stationary programs using ordinary native Noon leaves.

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
        objects = [words(subtitle, y=2.43, size=11, color=MUTED)]
        for x, (letter, title, color), program, position, state in zip(
                (-4.5, 0, 4.5), ROLES, programs, positions, states):
            rows = [words(line, x - 1.47, 1.16 - i * 0.28, 13, INK,
                          3.26, left=True) for i, line in enumerate(program)]
            pc_y = 1.16 - position * 0.28
            highlight = RoundedRectangle(width=3.50, height=0.25, corner_radius=0.045,
                    color=color, stroke_width=0.8).set_fill(color, opacity=0.07).set_stroke(
                        color, opacity=0.50).move_to((x + 0.10, pc_y, 0))
            pointer = Circle(radius=0.038, color=color, stroke_width=0).set_fill(
                    color, opacity=1).move_to((x - 1.82, pc_y, 0))
            pointer.set_glow(color=color, radius=Pixels(3.5), intensity=0.65)
            state_label = words(state, x + 1.15, 1.87, 10, color, 1.47)
            marks = [panel(x - 0.30 + i * 0.52, -0.24, 0.42, 0.065,
                           EDGE, EDGE, stroke=0) for i in range(4)]
            objects += [panel(x, 0.84, 4.07, 2.62),
                        words(letter + " / " + title, x - 1.80, 1.87, 13,
                              color, 2.22, left=True),
                        state_label, rule((x - 1.79, 1.65), (x + 1.79, 1.65)),
                        words("repeat:", x - 1.67, 1.44, 12, MUTED, left=True),
                        highlight, *rows, pointer,
                        words("work", x - 1.40, -0.24, 10, MUTED), *marks]
            board[letter] = {"x": x, "color": color, "rows": rows,
                             "state": state_label, "pointer": pointer,
                             "highlight": highlight, "marks": marks}
        cpu_label = words((owner + " RUNNING") if owner else "NO TASK SELECTED",
                          0.75, -0.86, 13, dict((r[0], r[2]) for r in ROLES).get(owner, MUTED), 2.9)
        objects += [panel(0, -0.86, 4.0, 0.51, RAISED),
                    words("ONE CPU", -1.33, -0.86, 11, MUTED, 1.1), cpu_label]
        board["cpu"] = cpu_label
        await self.show(objects, seconds=0.75)
        return board

    async def set_state(self, board, role, state):
        item = board[role]
        await self.hide(item["state"], seconds=0.12)
        item["state"] = words(state, item["x"] + 1.15, 1.87, 10,
                              item["color"], 1.47)
        await self.show(item["state"], seconds=0.22)

    async def select(self, board, role):
        # The captioned hypothetical scheduler selects one program. Selection
        # alone never advances that program's pointer or completed-work marks.
        await self.hide(board["cpu"], seconds=0.12)
        label = role + " RUNNING" if role else "DISPATCH"
        color = board[role]["color"] if role else MUTED
        board["cpu"] = words(label, 0.75, -0.86, 13, color, 2.9)
        board["owner"] = role
        await self.show(board["cpu"], seconds=0.22)
        if role:
            await self.set_state(board, role, "RUNNING")

    async def at_line(self, board, role, index, seconds=0.30):
        if board["owner"] != role:
            raise ValueError("Only the CPU-selected program can advance.")
        item = board[role]
        y = 1.16 - index * 0.28
        await self.play_for(item["pointer"].animate.move_to((item["x"] - 1.82, y, 0)),
                            item["highlight"].animate.move_to((item["x"] + 0.10, y, 0)),
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
            await self.focus_rect(item["x"], 0.84, 4.18, 2.73, item["color"])
        else:
            await self.focus_rect(item["x"] + 0.10, 1.16 - index * 0.28,
                                  3.57, 0.28, item["color"])

    async def restore_board(self, board):
        await self.clear_focus()
        await self.play_for(*[row.animate.set_opacity(1)
                             for letter, _, _ in ROLES for row in board[letter]["rows"]],
                            seconds=0.25)

    async def begin_chapter(self, number, title):
        old = self.content + self.captions + self.heading
        self.content, self.captions = [], []
        self.focus = None
        self.heading = [words(title, -6.48, 3.20, 28, width=12.6, left=True),
                        words("%02d / 13" % number, 5.93, 3.72, 12, MUTED)]
        if old:
            await self.play_for(*(FadeOut(obj) for obj in old), seconds=0.25)
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
        self.add(panel(0, 0, 14.24, 8.02, BG, BG, stroke=0),
                 words("REAL-TIME SYSTEMS / VISUAL FIELD GUIDE", -6.48, 3.72,
                       10, MUTED, left=True),
                 rule((-6.48, 2.83), (6.48, 2.83)),
                 rule((-6.48, -2.61), (6.48, -2.61)),
                 words("ONE CORE PER DESIGN  /  work marks are qualitative  /  timing bounds are illustrative",
                       y=-3.83, size=10, color=MUTED))
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
        await self.show(card("STATE REPRESENTATION", "Thread stacks / suspended async state", -3.25,
                             -1.82, 6.08, 1.06, CONTROL),
                        card("SCHEDULING POLICY", "Cooperative / preemptive execution boundaries", 3.25,
                             -1.82, 6.08, 1.06, ESTIMATE))
        await self.explain("State representation and scheduling are separate choices.",
                           "Async describes suspension and saved state. The surrounding runtime determines preemption.")
        await self.focus_rect(3.25, -1.82, 6.15, 1.13, ESTIMATE)
        await self.notice("Keep these same three loops in view as the runtime changes.",
                          "Waiting, readiness, CPU ownership and resource ownership will remain separate facts.")


    async def memory(self):
        await self.explain(
            "Where does C keep its place while acquiring a sample?",
            "The operation is unfinished. Another program can use the CPU while C waits.",
            hold=2.0)
        await self.show(
            words("C'S CURRENT OPERATION: acquire sample", y=2.20, size=15,
                  color=CONTROL),
            words("THREAD STACKS", -3.5, 1.80, 18, CONTROL),
            words("ASYNC STATE", 3.5, 1.80, 18, LOGGER),
            rule((0, 1.96), (0, -1.68)), seconds=0.5)

        stacks, states = [], []
        for x, (initial, _, color) in zip((-5.35, -3.55, -1.75), ROLES):
            stacks.extend((panel(x, 0.65, 1.45, 1.44, PANEL, color),
                           words(initial + " stack", x, 1.15, 13, color, 1.27),
                           words("reserved", x, 0.08, 10, MUTED, 1.27)))
        for x, (initial, _, color) in zip((1.6, 3.5, 5.4), ROLES):
            states.extend((panel(x, 0.67, 1.45, 0.99, PANEL, color),
                           words(initial + " state", x, 1.32, 13, color, 1.40)))
        await self.show(
            stacks, states,
            station("CPU USES THE SELECTED THREAD STACK", -3.55, -0.99,
                    5.4, 1.18),
            station("CPU USES THE ACTIVE EXECUTION STACK", 3.5, -0.99,
                    5.4, 1.18), seconds=0.55)

        tc = job("C", -3.55, -1.11, CONTROL)
        tl = job("L", -1.75, 0.63, LOGGER)
        ac = job("C", 3.5, -1.11, CONTROL)
        al = job("L", 5.4, 0.63, LOGGER)
        await self.show(tc, tl, ac, al, seconds=0.45)

        await self.focus_rect(-3.55, 0, 5.70, 3.26, color=CONTROL)
        await self.watch(
            "First, C is a thread. Watch its unfinished operation stay intact.",
            "The reserved stack retains the call and the saved execution position.",
            hold=0.9)
        await self.compute(tc, 0, 1, seconds=0.6)
        await self.move_job(tc, (-5.35, -1.11), (-5.35, 0.63), seconds=1.0)
        await self.move_job(tl, (-1.75, -1.11), (-3.55, -1.11), seconds=1.0)
        await self.compute(tl, 0, 2, seconds=1.0)
        await self.notice(
            "L advances. C's saved acquire-sample operation does not.",
            "A waiting program keeps its place even while another program runs.",
            hold=2.1)

        await self.focus_rect(3.5, 0, 5.70, 3.26, color=LOGGER)
        await self.explain(
            "Now perform the same handoff with async state.",
            "C retains its continuation and live data; the active stack can execute L.",
            hold=1.7)
        await self.compute(ac, 0, 1, seconds=0.6)
        await self.move_job(ac, (1.6, -1.11), (1.6, 0.63), seconds=1.0)
        await self.move_job(al, (5.4, -1.11), (3.5, -1.11), seconds=1.0)
        await self.compute(al, 0, 2, seconds=1.0)
        await self.notice(
            "The saved operation is separate from the active execution stack.",
            "Suspended futures still need storage, and the running program still uses a stack.",
            hold=2.1)

        await self.watch(
            "When C is selected again, it continues the saved operation.",
            "Follow the existing work mark; C does not restart from the beginning.",
            hold=0.8)
        await self.move_job(al, (5.4, -1.11), (5.4, 0.63), seconds=0.95)
        await self.move_job(ac, (1.6, -1.11), (3.5, -1.11), seconds=0.95)
        await self.compute(ac, 1, 2, seconds=0.7)
        await self.clear_focus()
        await self.show(
            words("Same unfinished operation. Different storage organization.",
                  y=-1.91, size=18, color=GOOD),
            words("Schematic sizes / count buffers and stack nesting / static allocation in both",
                  y=-2.27, size=11, color=MUTED), seconds=0.4)
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
                    positions=(1, 0, 1), states=("RUNNING", "WAITING", "READY"), owner="C")
            await self.show(station("SPI / DMA", -4.5, -1.65, 3.8, 1.00, ESTIMATE),
                            rule((-5.66, -1.83), (-3.34, -1.83), EDGE, 3),
                            words("DEVICE PROGRESS", -4.5, -2.27, 10, MUTED))
            issue = ((-5.20, -0.50), (-5.20, -1.11))
            ready = ((-3.80, -1.11), (-3.80, -0.50))
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
            dma = packet(-5.66, -1.83, ESTIMATE)
            await self.show(dma, seconds=0.12)
            for i in range(3):
                await self.play_for(board["L"]["marks"][i].animate.set_fill(GOOD, opacity=1),
                        dma.animate.move_to((-5.66 + (i + 1) * 2.32 / 3, -1.83, 0)),
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
                    positions=(3, 0, 0), states=("READY", "WAITING", "RUNNING"), owner="L")
        queue_label = words("3 PACKETS READY", 4.5, -1.68, 14, LOGGER, 3.4)
        await self.show(station("INPUT QUEUE", 4.5, -1.64, 3.8, 1.03, LOGGER), queue_label)
        feed = ((4.5, -1.09), (4.5, -0.50))
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
                                4.5, -1.68, 14, LOGGER if remaining else MUTED, 3.4)
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


    @staticmethod
    def tx(microseconds):
        return -5.45 + 11.3 * microseconds / AXIS_US


    async def trace_shell(self, subtitle):
        board = await self.loop_board(subtitle, positions=(3, 0, 1),
                    states=("READY", "WAITING", "RUNNING"), owner="L")
        await self.work(board, "L", 0, 1, seconds=0.2)
        axis = [rule((self.tx(0), -1.65), (self.tx(AXIS_US), -1.65))]
        for tick in (0, 100, 200, 250, 300, 400, 500, 600, 650):
            color = BAD if tick == DEADLINE_US else MUTED
            axis += [rule((self.tx(tick), -1.65), (self.tx(tick), -1.75), color),
                     words(str(tick), self.tx(tick), -1.90, 10, color)]
        axis += [rule((self.tx(DEADLINE_US), -1.64), (self.tx(DEADLINE_US), -1.17), BAD, 2),
                 words("DEADLINE", self.tx(DEADLINE_US), -2.10, 9, BAD),
                 words("µs after C release", 4.65, -2.10, 9, MUTED)]
        cursor = rule((self.tx(0), -1.64), (self.tx(0), -1.20), INK, 2.7)
        await self.show(axis, cursor, seconds=0.5)
        return board, cursor


    async def trace_segment(self, cursor, begin_us, end_us, color, markers=(), seconds=None):
        x1, x2 = self.tx(begin_us), self.tx(end_us)
        bar = panel(x1 + 0.003, -1.42, 0.006, 0.28, color, color,
                    opacity=0.87, stroke=0)
        self.content.append(bar)
        self.add(bar)
        animations = [bar.animate.stretch((x2 - x1) / 0.006, 0).move_to(
                          ((x1 + x2) / 2, -1.42, 0)),
                      cursor.animate.move_to((x2, -1.42, 0))]
        animations += [mark.animate.set_fill(GOOD, opacity=1) for mark in markers]
        await self.play_for(*animations,
                seconds=(end_us - begin_us) * 0.016 if seconds is None else seconds,
                rate_func=linear)


    async def control_execution(self, board, cursor, start):
        if board["owner"] != "C":
            raise ValueError("C must own the CPU during its response-time computation.")
        for i in range(4):
            await self.trace_segment(cursor, start + i * 20, start + (i + 1) * 20,
                                     CONTROL, (board["C"]["marks"][i],))
        await self.show(words("C", self.tx(start + CONTROL_US / 2), -1.42,
                              11, BG, 0.5), seconds=0.2)


    async def trace_result(self, example):
        blocking = example["blocking"][1]
        color = GOOD if example["met"] else BAD
        equation = "%d + %d + %d = %d µs" % (
            blocking, DISPATCH_US, CONTROL_US, example["finish"])
        outcome = ("MET · %d µs margin" % example["margin"] if example["met"]
                   else "MISSED by %d µs" % -example["margin"])
        await self.show(words(equation, -2.35, -2.36, 18, color, 7.5),
                        words(outcome, 3.65, -2.36, 14, color, 4.5), seconds=0.5)


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
            board = await self.loop_board(title, positions=(3, 1, 1),
                    states=("READY", "READY", "RUNNING"), owner="L")
            frames = []
            if not preemptive:
                frames.append(Rectangle(width=13.23, height=2.85, color=GOOD,
                              stroke_width=1.7, fill_opacity=0).move_to((0, 0.84, 0)))
            else:
                for x, color in ((-4.5, CONTROL), (0, ESTIMATE), (4.5, LOGGER)):
                    frames.append(Rectangle(width=4.20, height=2.85, color=color,
                                  stroke_width=1.7, fill_opacity=0).move_to((x, 0.84, 0)))
            labels = (("task", "task", "task") if not preemptive else
                      ("high-priority executor", "medium-priority executor", "background executor")
                      if index == 1 else ("high-priority thread", "medium-priority thread", "thread: async executor"))
            await self.show(frames, *[words(label, x, -1.39, 12, color, 4.1)
                    for label, x, color in zip(labels, (-4.5, 0, 4.5), (CONTROL, ESTIMATE, LOGGER))],
                    words("ONE CORE / scheduling changes ownership, never the number of CPUs",
                          y=-2.12, size=12, color=MUTED), seconds=0.5)
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
        def metric(title, claim, qualifier, x, y, color):
            return (panel(x, y, 6.10, 1.25),
                    words(title, x, y + 0.38, 13, color, 5.6),
                    words(claim, x, y + 0.01, 17, INK, 5.6),
                    words(qualifier, x, y - 0.37, 11, MUTED, 5.6))

        await self.explain(
            "Which performance cost could this workload improve?",
            "Keep E's update-estimate operation and the useful workload the same.",
            hold=2.3)
        await self.show(
            card("E / update estimate", "The same numerical algorithm in both designs",
                 0, 1.80, 12.6, 0.82, ESTIMATE), seconds=0.55)

        await self.show(
            metric("RAM", "Saved state + stacks + buffers",
                   "Possible reduction; count the complete budget",
                   -3.27, 0.31, CONTROL), seconds=0.45)
        await self.focus_rect(-3.27, 0.31, 6.20, 1.35, color=CONTROL)
        await self.notice(
            "RAM can improve when many programs mostly wait.",
            "Count retained state, active stacks, buffers and queues in both designs.",
            hold=2.2)

        await self.explain(
            "Coordination has a separate implementation cost.",
            "Waiting, waking and selecting a program all consume CPU work.",
            hold=1.2)
        await self.show(
            metric("COORDINATION", "Wait / wake / dispatch",
                   "Measure the selected runtime and drivers",
                   3.27, 0.31, LOGGER), seconds=0.45)
        await self.focus_rect(3.27, 0.31, 6.20, 1.35, color=LOGGER)
        await self.notice(
            "A smaller coordination path can help; more yields also add work.",
            "The runtime, driver and synchronization choices determine the actual cost.",
            hold=1.9)

        await self.focus_rect(0, 1.80, 12.70, 0.92, color=ESTIMATE)
        await self.watch(
            "Keep E's numerical operation unchanged.",
            "Changing its execution wrapper does not remove the estimate calculation.",
            hold=0.8)
        await self.show(
            metric("COMPUTATION", "The same estimate update",
                   "Async does not remove the arithmetic",
                   -3.27, -1.23, ESTIMATE), seconds=0.45)
        await self.pause(1.4)

        await self.explain(
            "Now ask when C's result becomes usable.",
            "Response includes blocking, dispatch, execution and the dependencies that exist.",
            hold=1.4)
        await self.show(
            metric("DEADLINE RESPONSE", "When the result becomes usable",
                   "Bound blocking and interference",
                   3.27, -1.23, GOOD), seconds=0.45)
        await self.focus_rect(3.27, -1.23, 6.20, 1.35, color=GOOD)
        await self.notice(
            "Earlier access to the CPU can improve deadline response.",
            "The controller's opportunity to execute can change while its arithmetic stays the same.",
            hold=1.9)
        await self.clear_focus()
        await self.show(
            words("Equivalent work, drivers, allocation rules and timing obligations.",
                  y=-2.23, size=14, color=GOOD), seconds=0.4)
        await self.notice(
            "State the metric and compare equally capable designs.",
            "Measure RAM, throughput, energy and jitter; justify the required timing bounds.",
            color=GOOD, hold=2.5)



    async def inversion(self):
        board = await self.loop_board("PRIORITIES: C HIGH / E MEDIUM / L LOW / L initially owns the shared SPI resource",
                    positions=(1, 1, 2), states=("READY", "READY", "RUNNING"), owner="L")
        owner_label = words("OWNER: L", 4.5, -1.69, 18, LOGGER, 3.3)
        await self.show(station("SHARED SPI", 4.5, -1.63, 3.8, 1.05, LOGGER), owner_label)
        request = ((-4.5, -0.50), (-4.5, -1.63), (2.54, -1.63))
        await self.draw_route(request, CONTROL, seconds=0.6)
        await self.show(rule((4.5, -0.50), (4.5, -1.09), LOGGER, 1.6),
                        words("resource ownership", 4.5, -2.30, 11, MUTED))
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
        await self.focus_rect(4.5, -1.63, 3.88, 1.13, LOGGER)
        await self.notice("CPU owner: E. SPI owner: L. C waits for L's progress.",
                          "This indirect delay is priority inversion; the highest-priority task can still be blocked.", BAD)
        boost = badge("L inherits C's effective priority", -0.45, -2.26, GOOD, 7.7)
        await self.explain("Now add priority inheritance: raise L to C's effective priority.",
                           "This is generic priority inheritance, not a property supplied by every async mutex.")
        await self.show(boost, seconds=0.35)
        await self.set_state(board, "E", "READY")
        await self.select(board, "L")
        await self.focus_loop(board, "L", 2)
        await self.work(board, "L", 1, 4, seconds=2.0)
        await self.hide(owner_label, boost, seconds=0.25)
        free_label = words("FREE", 4.5, -1.69, 18, GOOD, 3.3)
        await self.show(free_label, seconds=0.3)
        await self.set_state(board, "C", "READY")
        await self.notice("L releases SPI. C becomes ready to acquire it.",
                          "The dependency is resolved by the resource owner's progress, not by changing C's syntax.")
        await self.set_state(board, "L", "READY")
        await self.select(board, "C")
        await self.focus_loop(board, "C", 1)
        await self.pulse_data(request, CONTROL, seconds=1.0)
        await self.hide(free_label, seconds=0.15)
        await self.show(words("OWNER: C", 4.5, -1.69, 18, CONTROL, 3.3), seconds=0.3)
        await self.work(board, "C", 0, 2, seconds=0.7)
        await self.notice("C now acquires SPI and continues.",
                          "Bound lock hold time, device transactions and service queues as well as CPU execution.")


    async def timeouts(self):
        board = await self.loop_board("TRIAL 1 / a cooperative timeout wrapper surrounds L's inner operation",
                    positions=(0, 0, 1), states=("WAITING", "WAITING", "RUNNING"), owner="L")
        timer = words("ARMED", 4.5, -1.70, 17, MUTED, 3.4)
        await self.show(station("TIMEOUT TIMER", 4.5, -1.64, 3.8, 1.07, BAD), timer)
        await self.explain("A timeout can become ready while its inner operation still runs.",
                           "The wrapper cannot inspect the timer while a synchronous inner call holds control.")
        await self.focus_loop(board, "L", 1)
        await self.work(board, "L", 0, 1, seconds=0.8)
        await self.hide(timer, seconds=0.15)
        await self.show(words("READY", 4.5, -1.70, 17, BAD, 3.4), seconds=0.3)
        await self.focus_rect(4.5, -1.64, 3.88, 1.15, BAD)
        await self.notice("The timer is ready. L still owns the CPU.",
                          "An async timeout does not itself enforce a CPU execution budget.", BAD, hold=2.0)
        await self.work(board, "L", 1, 4, seconds=1.7)
        await self.set_state(board, "L", "READY")
        await self.select(board, None)
        await self.notice("Only after the inner call returns can the wrapper check the timer.",
                          "A schedulable monitor or watchdog needs its own enforcement boundary and timing analysis.", hold=2.0)
        await self.hide(list(self.content), seconds=0.35)
        self.focus = None
        await self.show(words("TRIAL 2 / cancellation after an external command has started", y=2.43,
                              size=12, color=MUTED),
                        station("CALLER", -4.65, 0.94, 3.1, 1.2, CONTROL),
                        station("DRIVER", 0, 0.94, 3.7, 1.2, MUTED),
                        station("DEVICE", 4.65, 0.94, 3.1, 1.2, ESTIMATE))
        caller_state = words("REQUEST ACTIVE", -4.65, 0.75, 14, CONTROL, 2.8)
        device_state = words("IDLE", 4.65, 0.75, 14, MUTED, 2.8)
        await self.show(caller_state, device_state,
                        words("operation in flight", 0, 0.75, 14, MUTED, 3.35))
        issue = ((-3.06, 0.94), (-1.89, 0.94))
        device = ((1.89, 0.94), (3.06, 0.94))
        await self.draw_route(issue, CONTROL, seconds=0.35)
        await self.draw_route(device, ESTIMATE, seconds=0.35)
        await self.explain("Cancellation has to account for effects already outside the caller.",
                           "Follow one command from caller to driver to the physical device.", hold=1.8)
        await self.pulse_data(issue, CONTROL, seconds=0.8)
        await self.pulse_data(device, ESTIMATE, seconds=0.8)
        await self.hide(device_state, seconds=0.15)
        await self.show(words("COMMAND ISSUED", 4.65, 0.75, 14, ESTIMATE, 2.8), seconds=0.3)
        await self.watch("The caller cancels after the driver has issued the command.")
        await self.hide(caller_state, seconds=0.15)
        await self.show(words("REQUEST CANCELLED", -4.65, 0.75, 14, BAD, 2.8), seconds=0.3)
        await self.focus_rect(4.65, 0.94, 3.18, 1.28, ESTIMATE)
        await self.notice("Cancelling the caller does not rewind the device.",
                          "The command may already be acting on the physical system.", BAD, hold=2.0)
        await self.show(card("DRIVER + PROTOCOL CONTRACT", "Specify abort, completion, cleanup and resource lifetimes",
                             0, -1.03, 11.8, 1.2, GOOD))
        await self.notice("Define the actual cancellation contract before relying on it.",
                          "Immediate abort, deferred cleanup and completion have different semantics and timing costs.")


    async def assurance(self):
        await self.explain(
            "Could C finish on time and still use an invalid input?",
            "Consider valid snapshot storage whose timestamp violates C's input-age policy.",
            hold=2.1)
        example = (
            words("ILLUSTRATIVE INPUT-AGE POLICY", y=1.99, size=13, color=MUTED),
            card("STATE SNAPSHOT", "Age: 12 ms / allowed: 5 ms",
                 -3.45, 0.78, 5.05, 1.38, ESTIMATE),
            card("C / read state snapshot", "A valid memory access can still read stale data",
                 3.45, 0.78, 5.05, 1.38, CONTROL),
            words("valid storage and lifetime", -3.45, -0.34, 14, GOOD, 5.05),
            words("timely computation is still possible", 3.45, -0.34, 14, MUTED, 5.05),
        )
        await self.show(example, seconds=0.55)
        await self.focus_rect(-3.45, 0.78, 5.17, 1.50, color=ESTIMATE)
        await self.watch(
            "Watch the snapshot's age when C reads it.",
            "Twelve milliseconds exceeds this example's five-millisecond limit.",
            hold=0.8)
        age_route = await self.draw_route(((-0.88, 0.78), (0.88, 0.78)),
                                         ESTIMATE, seconds=0.45)
        await self.pulse_data(((-0.88, 0.78), (0.88, 0.78)), ESTIMATE, seconds=1.1)
        stale = badge("AGE LIMIT EXCEEDED", 3.45, -0.94, BAD, width=4.25)
        await self.show(stale, seconds=0.35)
        await self.notice(
            "Valid memory does not establish valid control input.",
            "A timely task can still violate its required data-age contract.",
            color=BAD, hold=2.1)
        await self.clear_focus()
        await self.hide(example, age_route, stale, seconds=0.35)

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
        for number, (title, question, evidence, color, explanation) in enumerate(rows):
            y = 1.64 - number * 1.12
            await self.watch(explanation, hold=0.6)
            await self.show(
                panel(0, y, 12.65, 0.93),
                panel(-6.27, y, 0.07, 0.93, color, color, stroke=0),
                words(title, -5.06, y, 15, color, 2.18),
                words(question, -3.53, y + 0.18, 17, INK, 9.22, left=True),
                words(evidence, -3.53, y - 0.22, 12, MUTED, 9.22, left=True),
                seconds=0.45)
            await self.focus_rect(0, y, 12.76, 1.03, color=color)
            await self.pause(0.9)
        await self.clear_focus()
        await self.notice(
            "Each guarantee needs a stated property and assumptions.",
            "Apply these contracts to threads, async tasks or a hybrid.",
            color=GOOD, hold=1.5)


    async def control_boundary(self):
        await self.show(
            card("SENSOR CAPTURE", "Timestamp / DMA", -4.75, 1.35,
                 3.1, 1.45, ESTIMATE),
            station("C / HIGH-PRIORITY CONTROL", 0, 1.35, 3.85, 1.45, CONTROL),
            card("ACTUATOR", "Bound command latency", 4.75, 1.35,
                 3.1, 1.45, CONTROL),
            station("L / BACKGROUND ASYNC", -4.75, -1.45, 3.1, 1.45, LOGGER),
            station("E / ESTIMATION", 0, -1.45, 3.85, 1.45, ESTIMATE),
            card("MONITOR / PROTECTION", "Independent fault path", 4.75, -1.45,
                 3.1, 1.45, GOOD), seconds=0.6)
        c = job("C", 0, 1.13, CONTROL)
        e = job("E", 0, -1.67, ESTIMATE)
        l = job("L", -5.56, -1.61, LOGGER)
        queue = [panel(x, -1.39, 0.33, 0.27, PANEL, LOGGER)
                 for x in (-4.70, -4.23, -3.76)]
        await self.show(c, e, l, queue,
                        words("log queue", -4.23, -1.83, 10, MUTED, 1.85),
                        seconds=0.4)
        await self.explain(
            "Where does the end-to-end control deadline end?",
            "Trace one sample through shared-CPU computation to the physical actuator.",
            hold=2.3)

        sample_route = ((-3.16, 1.35), (-1.97, 1.35))
        command_route = ((1.97, 1.35), (3.16, 1.35))
        snapshot_route = ((0, -0.68), (0, 0.58))
        log_route = ((-1.22, 0.58), (-1.22, 0.04), (-4.75, 0.04), (-4.75, -0.68))
        fault_route = ((4.75, -0.68), (4.75, 0.58))

        await self.focus_rect(-4.75, 1.35, 3.22, 1.57, color=ESTIMATE)
        await self.watch(
            "Start with a timestamped sensor sample.",
            "Acquisition and delivery count before control can use the data.",
            hold=1.1)
        await self.show(words("sample", -2.58, 1.81, 10, ESTIMATE, 1.1), seconds=0.3)
        await self.draw_route(sample_route, ESTIMATE, seconds=0.5)
        await self.pulse_data(sample_route, ESTIMATE, seconds=1.2)
        await self.notice(
            "The sample arrived; C still needs a usable state snapshot.",
            "Arrival alone does not establish freshness or consistency.",
            hold=1.8)

        await self.focus_rect(0, -1.45, 3.97, 1.57, color=ESTIMATE)
        await self.explain(
            "E publishes its next coherent snapshot.",
            "Only E's illustrated computation advances during this CPU turn.",
            hold=1.8)
        await self.compute(e, 0, 4, seconds=1.8)
        await self.draw_route(snapshot_route, ESTIMATE, seconds=0.45)
        await self.show(
            words("coherent snapshot", 1.35, 0.24, 11, ESTIMATE, 2.4),
            words("timestamp + age", 1.35, -0.14, 11, MUTED, 2.4), seconds=0.3)
        await self.pulse_data(snapshot_route, ESTIMATE, seconds=1.0)
        await self.notice(
            "C can now read a coherent snapshot with an allowed age.",
            "Specify the age policy separately from memory ownership.",
            hold=1.4)

        await self.focus_rect(0, 1.35, 3.97, 1.57, color=CONTROL)
        await self.explain(
            "C gets the CPU and computes the command.",
            "E and L retain their positions while C advances.",
            hold=1.5)
        await self.compute(c, 0, 4, seconds=1.8)
        await self.show(words("command", 2.58, 1.81, 10, CONTROL, 1.1), seconds=0.3)
        await self.draw_route(command_route, CONTROL, seconds=0.45)
        await self.pulse_data(command_route, CONTROL, seconds=1.0)
        applied = words("command applied", 4.75, 0.85, 11, GOOD, 2.7)
        await self.show(applied, seconds=0.3)
        await self.notice(
            "The end-to-end path includes applying the actuator command.",
            "Finishing C's arithmetic is only one part of that path.",
            hold=1.6)

        await self.focus_rect(-4.75, -1.45, 3.22, 1.57, color=LOGGER)
        await self.explain(
            "A log copy enters a bounded background queue.",
            "The full-queue policy must preserve the control contract.",
            hold=1.3)
        await self.draw_route(log_route, LOGGER, seconds=0.5)
        await self.show(words("bounded log queue", -3.33, 0.33, 11, LOGGER, 2.65),
                        seconds=0.3)
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

        await self.focus_rect(4.75, -1.45, 3.22, 1.57, color=GOOD)
        await self.explain(
            "A detected fault follows a separate containment contract.",
            "The monitor needs a schedulable enforcement boundary and platform evidence.",
            hold=1.5)
        await self.draw_route(fault_route, GOOD, seconds=0.45)
        await self.show(words("detect", 5.71, 0.24, 11, GOOD, 1.25),
                        words("contain", 5.71, -0.14, 11, GOOD, 1.25), seconds=0.3)
        await self.pulse_data(fault_route, GOOD, seconds=1.0)
        await self.hide(applied, seconds=0.2)
        await self.show(words("specified fault response", 4.75, 0.85,
                              10, GOOD, 2.8), seconds=0.3)
        await self.notice(
            "The monitor's fault response has its own bounds.",
            "Scheduling urgency and fault containment are separate design obligations.",
            hold=1.0)
        await self.clear_focus()
        await self.show(
            words("Choose execution boundaries to match urgency and containment needs.",
                  y=2.27, size=12, color=GOOD, width=12.5), seconds=0.4)
        await self.notice(
            "Prove execution, blocking and data-age bounds.",
            "Async, threads or a hybrid can fit when their concrete contracts support the system.",
            color=GOOD, hold=0.1)
