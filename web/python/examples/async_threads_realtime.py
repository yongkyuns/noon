"""Async, threads, and the deadline — an original, 13-chapter Noon tutorial.

Run this file in Noon's Python editor. The complete lesson is 408 authored
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
    ("State and scheduling are separate choices", 16, "choices"),
    ("Paused work keeps its place", 26, "memory"),
    ("Both runtimes can release the CPU for I/O", 42, "waiting"),
    ("Available work can keep the CPU busy", 32, "await_paths"),
    ("Selection priority cannot interrupt a poll", 34, "long_poll"),
    ("Bounded cooperative work can succeed", 28, "bounded_poll"),
    ("Preempt, preserve, resume", 38, "preemption"),
    ("Three architectures, still one CPU", 36, "architectures"),
    ("Which performance metric improves?", 24, "performance"),
    ("The highest priority can still be blocked", 42, "inversion"),
    ("Timeout and cancellation need contracts", 30, "timeouts"),
    ("Safety needs explicit guarantees", 24, "assurance"),
    ("Follow the sample all the way to actuation", 36, "control_boundary"),
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
    return Square(side_length=0.17, color=color, stroke_width=0).set_fill(
        color=color, opacity=1).move_to((x, y, 0))


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
        for point in points[1:]:
            await self.play_for(dot.animate.move_to((*point, 0)),
                                seconds=seconds / (len(points) - 1), rate_func=linear)
        await self.hide(dot, seconds=0.12)

    async def hide(self, *items, seconds=0.3):
        objects = authored_objects(*items)
        await self.play_for(*(FadeOut(obj) for obj in objects), seconds=seconds)
        removed = {id(obj) for obj in objects}
        self.content = [obj for obj in self.content if id(obj) not in removed]

    async def caption(self, main, detail="", color=INK):
        new = [words(main, y=-2.99, size=21, color=color),
               words(detail or " ", y=-3.43, size=13, color=MUTED)]
        # Remove the old sentence before writing the next, so no transition
        # frame superimposes two different explanations in the same space.
        if self.captions:
            await self.play_for(*(FadeOut(obj) for obj in self.captions), seconds=0.13)
        else:
            await self.pause(0.13)
        await self.play_for(Write(new[0]), FadeIn(new[1]), seconds=0.35)
        self.captions = new

    async def begin_chapter(self, number, title):
        old = self.content + self.captions + self.heading
        self.content, self.captions = [], []
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
        self.add(panel(0, 0, 14.24, 8.02, BG, BG, stroke=0),
                 words("REAL-TIME SYSTEMS / VISUAL FIELD GUIDE", -6.48, 3.72,
                       10, MUTED, left=True),
                 rule((-6.48, 2.83), (6.48, 2.83)),
                 rule((-6.48, -2.61), (6.48, -2.61)),
                 words("ONE CORE PER DESIGN  /  schematic work marks  /  illustrative timing bounds",
                       y=-3.83, size=10, color=MUTED))
        for x, (initial, name, color) in zip((-4.75, -0.1, 4.35), ROLES):
            self.add(words(initial + "  " + name, x, 2.61, 12, color))
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
        await self.show(words("COOPERATIVE", -0.7, 1.94, 17, CONTROL),
                        words("PREEMPTIVE", 3.65, 1.94, 17, ESTIMATE),
                        words("THREADS", -4.94, 0.88, 19, INK, 2.55),
                        words("ASYNC TASKS", -4.94, -0.66, 19, INK, 2.55))
        await self.show(card("Yield or block", "Scheduler switches contexts", -0.7, 0.88,
                             3.85, 1.16, CONTROL),
                        card("Priority interrupt", "A higher-priority thread can run", 3.65, 0.88,
                             3.85, 1.16, ESTIMATE))
        await self.caption("Threads describe execution contexts with saved stacks.",
                           "A scheduler decides when those contexts can hand over the CPU.")
        await self.pause(1.4)
        await self.show(card("Task returns control", "One executor cooperates between polls", -0.7,
                             -0.66, 3.85, 1.16, CONTROL),
                        card("Priorities around tasks", "Preempt across execution boundaries", 3.65,
                             -0.66, 3.85, 1.16, ESTIMATE))
        focus = rule((-2.6, -1.37), (1.2, -1.37), GOOD, 3)
        self.content.append(focus)
        await self.play_for(FadeIn(focus), seconds=0.55)
        await self.caption("Async describes tasks that save state when they suspend.",
                           "The surrounding runtime can still contain preemptive priorities.")
        await self.play_for(focus.animate.move_to((3.65, -1.37, 0)), seconds=1.3)
        await self.pause(1.3)
        await self.show(words("State storage and scheduling are separate choices.", y=-1.91,
                              size=22, color=GOOD))
        await self.caption("Now follow the computation: run, wait, become ready, resume.",
                           "A colored tile is a task. Its small marks preserve completed work.")

    async def memory(self):
        await self.show(words("ALTERNATIVE MEMORY ORGANIZATIONS", y=2.21, size=12, color=MUTED),
                        words("THREADS", -3.5, 1.83, 18, CONTROL),
                        words("ASYNC TASKS", 3.5, 1.83, 18, LOGGER),
                        rule((0, 1.97), (0, -1.59)))
        for x, (initial, _, color) in zip((-5.35, -3.55, -1.75), ROLES):
            await self.show(panel(x, 0.66, 1.45, 1.42, PANEL, color),
                            words(initial + " stack", x, 1.16, 13, color, 1.27),
                            words("reserved", x, 0.08, 10, MUTED, 1.27), seconds=0.28)
        await self.show(station("CPU USES THE SELECTED THREAD STACK", -3.55, -0.96,
                               5.4, 1.18),
                        station("CPU USES THE ACTIVE EXECUTION STACK", 3.5, -0.96,
                                5.4, 1.18))
        for x, (initial, _, color) in zip((1.6, 3.5, 5.4), ROLES):
            await self.show(panel(x, 0.68, 1.45, 0.99, PANEL, color),
                            words(initial + " state", x, 1.34, 13, color, 1.40), seconds=0.28)
        tc = job("C", -3.55, -1.08, CONTROL)
        tl = job("L", -1.75, 0.63, LOGGER)
        ac = job("C", 3.5, -1.08, CONTROL)
        al = job("L", 5.4, 0.63, LOGGER)
        await self.show(tc, tl, ac, al)
        await self.caption("A blocked thread keeps its call stack and execution position.")
        await self.compute(tc, 0, 1, seconds=0.8)
        await self.move_job(tc, (-5.35, -1.08), (-5.35, 0.63), seconds=1.3)
        await self.move_job(tl, (-1.75, -1.08), (-3.55, -1.08), seconds=1.3)
        await self.compute(tl, 0, 2, seconds=1.0)
        await self.pause(1.2)
        await self.caption("A suspended async task keeps its continuation and retained data.",
                           "The active stack can then execute a different task.")
        await self.compute(ac, 0, 1, seconds=0.8)
        await self.move_job(ac, (1.6, -1.08), (1.6, 0.63), seconds=1.3)
        await self.move_job(al, (5.4, -1.08), (3.5, -1.08), seconds=1.3)
        await self.compute(al, 0, 2, seconds=1.0)
        await self.move_job(al, (5.4, -1.08), (5.4, 0.63), seconds=1.1)
        await self.move_job(ac, (1.6, -1.08), (3.5, -1.08), seconds=1.1)
        await self.compute(ac, 1, 2, seconds=0.7)
        await self.caption("The computation resumes from the saved position.",
                           "Many suspended tasks can save RAM; retained buffers and stack nesting still matter.")
        await self.show(words("Both models can allocate statically. Sizes here are schematic.",
                              y=-2.09, size=17, color=GOOD))

    async def waiting(self):
        await self.show(station("READY", -4.65, 0.15, 2.85, 2.65, CONTROL),
                        station("ONE CPU", -0.45, 0.15, 3.35, 2.65),
                        station("WAITING", 4.5, 0.85, 3.2, 1.4, CONTROL),
                        station("SPI / DMA", 4.5, -1.3, 3.2, 1.15, ESTIMATE),
                        words("work progresses here", -0.45, -0.74, 12, MUTED, 2.9),
                        words("saved", 5.52, 0.81, 10, MUTED, 1.0),
                        words("state", 5.52, 0.48, 10, MUTED, 1.0),
                        panel(4.5, -1.47, 2.5, 0.16, EDGE, EDGE, stroke=0))
        await self.draw_route(((-3.18, 0.65), (-2.17, 0.65)), CONTROL)
        await self.draw_route(((1.28, 0.65), (2.85, 0.65)), CONTROL)
        issue = ((1.28, -0.53), (2.08, -0.53), (2.08, -1.3), (2.85, -1.3))
        await self.draw_route(issue, ESTIMATE)
        await self.draw_route(((4.5, -0.68), (4.5, 0.10)), ESTIMATE)
        await self.draw_route(((3.45, 1.60), (3.45, 1.97), (-5.55, 1.97),
                               (-5.55, 1.52)), CONTROL)
        mode_label = None
        for mode, retained in (("BLOCKING THREAD", "block + keep the call stack"),
                               ("ASYNC TASK", "suspend + keep continuation state")):
            if mode_label is not None:
                await self.hide(mode_label)
            mode_label = words(mode + "  /  " + retained, y=-2.18, size=18, color=GOOD)
            c = job("C", -4.65, 0.65, CONTROL)
            l = job("L", -4.65, -0.40, LOGGER)
            dma = packet(3.33, -1.47)
            await self.show(mode_label, c, l, dma)
            await self.caption("1. C runs and starts the sensor transfer.",
                               "This pass uses a " + mode.lower() + ". The device has its own progress track.")
            await self.move_job(c, (-0.45, 0.65), seconds=0.8)
            await self.compute(c, 0, 1, seconds=0.6)
            await self.pulse_data(issue, seconds=0.8)
            await self.move_job(c, (4.5, 0.65), seconds=0.9)
            await self.move_job(l, (-4.65, 0.65), (-0.45, 0.65), seconds=1.0)
            await self.caption("2. C is waiting. L computes while DMA progresses.",
                               "C's completed-work marker stays frozen outside the CPU.")
            for i in range(4):
                await self.play_for(l.submobjects[i + 2].animate.set_fill(GOOD, opacity=1),
                                    dma.animate.move_to((3.33 + 2.34 * (i + 1) / 4, -1.47, 0)),
                                    seconds=0.58, rate_func=linear)
            await self.pulse_data(((4.5, -0.68), (4.5, 0.10)), seconds=0.5)
            await self.caption("3. Completion makes C ready; it does not run immediately.")
            await self.move_job(c, (3.45, 0.65), (3.45, 1.97), (-5.55, 1.97),
                                (-5.55, 0.65), (-4.65, 0.65), seconds=2.3)
            await self.pause(0.8)
            await self.hide(l, seconds=0.35)
            await self.move_job(c, (-0.45, 0.65), seconds=0.9)
            await self.compute(c, 1, 4, seconds=1.2)
            await self.caption("4. The scheduler selects C; its computation resumes.",
                               "Both designs can overlap I/O with CPU work. Neither adds a second CPU.")
            await self.pause(1.0)
            await self.hide(c, dma, seconds=0.3)

    async def await_paths(self):
        await self.show(station("READY", -4.75, 0.65, 2.85, 1.82, CONTROL),
                        station("ONE CPU / COOPERATIVE EXECUTOR", 0, 0.65, 4.10, 1.82),
                        station("INPUT QUEUE", 4.7, 0.65, 3.0, 1.82, ESTIMATE),
                        station("WAITING FOR DATA", 4.7, -1.44, 3.0, 1.35, LOGGER),
                        words("eligible, but not selected", -4.75, -0.66, 12, MUTED, 2.85))
        await self.draw_route(((-3.27, 0.53), (-2.10, 0.53)), CONTROL)
        await self.draw_route(((3.15, 0.53), (2.10, 0.53)), ESTIMATE)
        c = job("C", -4.75, 0.53, CONTROL)
        l = job("L", 0, 0.53, LOGGER)
        packets = [packet(x, 0.53) for x in (3.85, 4.7, 5.55)]
        await self.show(c, l, packets)
        await self.caption("1. L asks for input. A packet is already available.",
                           "The task can continue in the same uninterrupted execution segment.")
        scope = words("ONE UNINTERRUPTED RUN", 0, -1.19, 17, LOGGER, 5.6)
        await self.show(scope)
        for i, item in enumerate(packets):
            await self.play_for(item.animate.move_to((0.96, 0.53, 0)),
                                seconds=0.95, rate_func=linear)
            await self.compute(l, i, i + 1, seconds=0.7)
            await self.hide(item, seconds=0.16)
            if i == 0:
                await self.caption("2. More input is ready, so L keeps computing.",
                                   "An immediately-ready async operation need not hand over the CPU.")
        await self.pause(1.7)
        await self.show(words("empty", 4.7, 0.53, 18, MUTED))
        await self.caption("3. This time no data is available. L must suspend.",
                           "Its saved progress remains visible while it waits.")
        await self.hide(scope)
        await self.draw_route(((1.10, -0.31), (1.10, -0.73), (2.58, -0.73),
                               (2.58, -1.54), (3.15, -1.54)), LOGGER)
        await self.move_job(l, (1.10, 0.53), (1.10, -0.73), (2.58, -0.73),
                            (2.58, -1.54), (4.7, -1.54), seconds=2.1)
        await self.move_job(c, (0, 0.53), seconds=1.0)
        await self.compute(c, 0, 3, seconds=1.7)
        await self.caption("4. Scheduling control returns; another ready task can run.",
                           "Bound the whole uninterrupted segment, including repeated immediately-ready work.")
        await self.show(words("Small operations can still form one long CPU occupation.",
                              y=-2.35, size=17, color=GOOD))

    @staticmethod
    def tx(microseconds):
        return -5.45 + 11.3 * microseconds / AXIS_US

    async def trace_shell(self, parked_title):
        await self.show(words("C: period 1,000 µs  |  deadline 250 µs  |  computation 80 µs",
                              y=2.18, size=13, color=CONTROL),
                        words("One release · dispatch 5 µs · no other interference or I/O waits",
                              y=1.81, size=11, color=MUTED),
                        station("C READY", -4.65, 0.87, 2.85, 1.22, CONTROL),
                        station("ONE CPU", 0, 0.87, 3.15, 1.22),
                        station(parked_title, 4.65, 0.87, 2.85, 1.22, LOGGER))
        await self.draw_route(((-3.18, 0.71), (-1.62, 0.71)), CONTROL, seconds=0.4)
        park_route = await self.draw_route(((1.62, 0.71), (3.18, 0.71)), LOGGER, seconds=0.4)
        await self.show(rule((self.tx(0), -0.92), (self.tx(AXIS_US), -0.92)))
        axis = []
        for tick in (0, 100, 200, 300, 400, 500, 600, 650):
            axis += [rule((self.tx(tick), -0.92), (self.tx(tick), -1.04)),
                     words(str(tick), self.tx(tick), -1.25, 11, MUTED)]
        axis += [rule((self.tx(DEADLINE_US), -0.80),
                      (self.tx(DEADLINE_US), -0.09), BAD, 2.2),
                 words("DEADLINE 250", self.tx(DEADLINE_US), 0.05, 11, BAD),
                 words("µs after C release", 4.59, -1.59, 11, MUTED)]
        cursor = rule((self.tx(0), -0.81), (self.tx(0), -0.25), INK, 2.7)
        c = job("C", -4.65, 0.71, CONTROL)
        l = job("L", 0, 0.71, LOGGER, completed=1)
        await self.show(axis, cursor, c, l)
        return cursor, c, l, park_route

    async def trace_segment(self, cursor, begin_us, end_us, color, markers=(), seconds=None):
        x1, x2 = self.tx(begin_us), self.tx(end_us)
        bar = panel(x1 + 0.003, -0.53, 0.006, 0.36, color, color,
                    opacity=0.87, stroke=0)
        self.content.append(bar)
        self.add(bar)
        # Stretch the existing native rectangle, rather than replacing its
        # procedural content. Stretch plus translation keeps the left edge fixed.
        animations = [bar.animate.stretch((x2 - x1) / 0.006, 0).move_to(
                          ((x1 + x2) / 2, -0.53, 0)),
                      cursor.animate.move_to((x2, -0.53, 0))]
        animations += [mark.animate.set_fill(GOOD, opacity=1) for mark in markers]
        await self.play_for(*animations,
                            seconds=(end_us - begin_us) * 0.016 if seconds is None else seconds,
                            rate_func=linear)

    async def control_execution(self, cursor, c, start):
        for i in range(4):
            await self.trace_segment(cursor, start + i * 20, start + (i + 1) * 20,
                                     CONTROL, (c.submobjects[i + 2],))
        await self.show(words("C", self.tx(start + CONTROL_US / 2), -0.53,
                              12, BG, 0.5), seconds=0.2)

    async def trace_result(self, example):
        blocking = example["blocking"][1]
        color = GOOD if example["met"] else BAD
        equation = "%d + %d + %d = %d µs" % (
            blocking, DISPATCH_US, CONTROL_US, example["finish"])
        outcome = ("MET · %d µs margin" % example["margin"] if example["met"]
                   else "MISSED by %d µs" % -example["margin"])
        await self.show(words(equation, -1.9, -2.06, 23, color, 7.3),
                        badge(outcome, 4.03, -2.06, color, 4.1))

    async def long_poll(self):
        example = response_example(500)
        cursor, c, l, _ = await self.trace_shell("FINISHED WORK")
        await self.caption("1. L is computing. C becomes ready at t = 0.",
                           "One cooperative executor; even higher selection priority cannot interrupt this poll.")
        await self.trace_segment(cursor, 0, DEADLINE_US, LOGGER, (l.submobjects[3],))
        await self.caption("2. The deadline passes. The same computation still owns the CPU.",
                           "C has made no progress while it waits in the ready area.", BAD)
        await self.pause(1.4)
        await self.trace_segment(cursor, DEADLINE_US, 500, LOGGER,
                                 (l.submobjects[4], l.submobjects[5]))
        await self.show(words("L: one long poll", self.tx(250), -0.53, 12, BG))
        await self.caption("3. L finally returns control; C can be selected.")
        await self.move_job(l, (4.65, 0.71), seconds=0.95)
        await self.trace_segment(cursor, 500, 505, MUTED)
        await self.move_job(c, (0, 0.71), seconds=1.0)
        await self.control_execution(cursor, c, 505)
        await self.trace_result(example)
        await self.caption("4. C finishes at 585 µs, missing its deadline by 335 µs.",
                           "Low average CPU utilization would not fix this uninterrupted blocking interval.", BAD)

    async def bounded_poll(self):
        example = response_example(50)
        cursor, c, l, _ = await self.trace_shell("L REMAINS READY")
        await self.caption("1. L performs a bounded chunk, then returns control.",
                           "Assume the remaining duration of every relevant poll is at most 50 µs.")
        await self.trace_segment(cursor, 0, 50, LOGGER, (l.submobjects[3],))
        await self.move_job(l, (4.65, 0.71), seconds=1.0)
        await self.caption("2. Select C next; keep L's unfinished computation for later.",
                           "The next-selection assumption is required for this particular response bound.")
        await self.trace_segment(cursor, 50, 55, MUTED)
        await self.move_job(c, (0, 0.71), seconds=1.0)
        await self.control_execution(cursor, c, 55)
        await self.trace_result(example)
        await self.caption("3. C finishes at 135 µs with 115 µs of margin.",
                           "The same 80 µs of control computation begins sooner.", GOOD)
        await self.pause(2)
        await self.show(words("Other ready tasks ahead of C would add interference.",
                              y=-1.65, size=14, color=MUTED, width=8.6, x=-1.75))
        await self.caption("Cooperation can meet deadlines when the complete bounds fit.",
                           "Yielding more often has a cost; prove the poll and queueing bounds.")

    async def preemption(self):
        example = response_example(10)
        cursor, c, l, park_route = await self.trace_shell("L PREEMPTED")
        await self.caption("1. Keep L's long computation. Allow higher-priority preemption.",
                           "Here, 10 µs bounds L's applicable non-preemptible execution.")
        await self.trace_segment(cursor, 0, 10, LOGGER)
        await self.move_job(l, (4.65, 0.71), seconds=1.2)
        await self.caption("2. L is paused with its current progress preserved.",
                           "It did not have to finish the computation or reach an async suspension.")
        await self.trace_segment(cursor, 10, 15, MUTED)
        await self.move_job(c, (0, 0.71), seconds=1.0)
        await self.control_execution(cursor, c, 15)
        await self.trace_result(example)
        await self.caption("3. C finishes at 95 µs, with 155 µs of margin.",
                           "Now watch the original L computation resume.", GOOD)
        await self.pause(1.0)
        await self.hide(c, park_route, seconds=0.35)
        await self.draw_route(((3.18, 0.71), (1.62, 0.71)), LOGGER, seconds=0.45)
        await self.move_job(l, (0, 0.71), seconds=1.25)
        for begin, end, marker in ((95, 250, 3), (250, 415, 4), (415, 585, 5)):
            await self.trace_segment(cursor, begin, end, LOGGER, (l.submobjects[marker],))
        await self.caption("4. L continues from the saved point and completes its remaining work.",
                           "Preemptive threads and priority-separated async executors can both provide this boundary.")
        await self.pause(1.5)
        await self.caption("Preemption changes blocking. It does not remove resource dependencies.",
                           "Interrupt masking, locks, device operations and higher-priority work still need bounds.")

    async def architectures(self):
        await self.show(station("ONE CPU", 4.85, 0, 2.7, 2.1),
                        words("one selected computation", 4.85, -0.72, 11, MUTED, 2.4),
                        words("dispatch / resume", 2.48, 1.97, 11, MUTED, 2.3),
                        rule((2.45, -1.42), (2.45, 1.42), MUTED, 2.2))
        for y in (1.42, 0, -1.42):
            await self.show(rule((1.62, y), (2.45, y), MUTED, 2.2), seconds=0.32)
            await self.show(Dot((2.45, y, 0), radius=0.046, color=MUTED), seconds=0.16)
        await self.show(rule((2.45, 0), (3.45, 0), MUTED, 2.2), seconds=0.35)
        modes = (
            ("ONE COOPERATIVE EXECUTOR",
             (("C · control", "a task in the executor"),
              ("E · estimation", "a task in the executor"),
              ("L · logging", "a task in the executor")),
             "L returns control before C can run.",
             "Task selection alone does not interrupt a running poll."),
            ("SEPARATE PREEMPTIVE PRIORITIES",
             (("HIGH PRIORITY · C", "control execution boundary"),
              ("MEDIUM PRIORITY · E", "estimation execution boundary"),
              ("BACKGROUND · L", "background execution boundary")),
             "Higher-priority execution can interrupt lower-priority work.",
             "One example is Embassy's Cortex-M interrupt executors at distinct priorities."),
            ("RTOS THREADS WITH BACKGROUND ASYNC",
             (("HIGH-PRIORITY THREAD · C", "control"),
              ("MEDIUM-PRIORITY THREAD · E", "estimation"),
              ("LOW-PRIORITY THREAD", "async executor: logging / commands / telemetry")),
             "An RTOS can schedule the boundaries around async tasks.",
             "Threaded and async execution can be combined in the same design."),
        )
        for index, (title, rows, main, detail) in enumerate(modes):
            objects = [words(title, -2.25, 2.17, 14, GOOD, 7.85)]
            if index == 0:
                objects += [panel(-2.3, 0, 7.8, 4.04, PANEL, CONTROL)]
            for y, (name, detail_line), color in zip((1.42, 0, -1.42), rows,
                                                    (CONTROL, ESTIMATE, LOGGER)):
                if index != 0:
                    objects += [panel(-2.3, y, 7.8, 1.06, PANEL, color)]
                objects += [words(name, -3.05, y + 0.18, 15, color, 5.82),
                            words(detail_line, -3.05, y - 0.22, 11, MUTED, 5.82)]
            c, e, l = (job(letter, 0.70, y, color, 1 if letter == "L" else 0)
                       for letter, y, color in (("C", 1.42, CONTROL),
                                               ("E", 0, ESTIMATE), ("L", -1.42, LOGGER)))
            await self.show(objects, c, e, l)
            await self.caption(main, detail)
            await self.move_job(l, (2.45, -1.42), (2.45, 0), (4.85, 0), seconds=1.35)
            await self.compute(l, 1, 2, seconds=0.8)
            await self.move_job(l, (2.45, 0), (2.45, -1.42), (0.70, -1.42), seconds=1.35)
            await self.move_job(c, (2.45, 1.42), (2.45, 0), (4.85, 0), seconds=1.35)
            await self.compute(c, 0, 3, seconds=1.1)
            if index == 1:
                await self.move_job(c, (2.45, 0), (2.45, 1.42), (0.70, 1.42), seconds=1.2)
                await self.move_job(l, (2.45, -1.42), (2.45, 0), (4.85, 0), seconds=1.2)
                await self.compute(l, 2, 3, seconds=0.6)
            await self.pause(0.7)
            if index < 2:
                await self.hide(objects, c, e, l, seconds=0.4)
        await self.caption("Several task containers still share one execution stream.",
                           "The marked junction is the scheduling choice; it is not another processor.")

    async def performance(self):
        boxes = (
            ("RAM", "Often less reserved stack", "Many waiting tasks; saved-state sizes still matter", -3.27, 1.27, CONTROL),
            ("COORDINATION", "Wait / wake costs can decrease", "Executor and RTOS implementations differ", 3.27, 1.27, LOGGER),
            ("COMPUTATION", "The same EKF arithmetic executes", "Async does not make the algorithm faster", -3.27, -0.73, ESTIMATE),
            ("DEADLINES", "Blocking + interference + execution", "Measure worst-case response, not only averages", 3.27, -0.73, GOOD),
        )
        for title, claim, qualifier, x, y, color in boxes:
            await self.show(panel(x, y, 6.1, 1.73),
                            words(title, x, y + 0.54, 13, color),
                            words(claim, x, y + 0.04, 18, INK, 5.6),
                            words(qualifier, x, y - 0.47, 12, MUTED, 5.6))
            await self.pause(1.45)
        await self.caption("Async can reduce coordination and memory costs.",
                           "Throughput, energy, RAM, jitter and worst-case response are different metrics.")
        await self.pause(3)
        await self.show(words("More frequent yields also add work. Measure the concrete design.",
                              y=-2.12, size=17, color=GOOD))
        await self.caption("Neither model is universally faster.",
                           "Compare equally capable drivers, allocation policies and scheduling guarantees.")

    async def resource_move(self, token, home_y, into_cpu=True):
        if home_y == -0.14:
            route = ((-3.52, -0.14), (-1.91, -0.14))
            destinations = ((-0.3, -0.14),) if into_cpu else ((-4.9, home_y),)
        else:
            route = ((-3.52, home_y), (-2.57, home_y), (-2.57, -0.14), (-1.91, -0.14))
            destinations = ((-2.57, home_y), (-2.57, -0.14), (-0.3, -0.14)) if into_cpu else (
                (-2.57, -0.14), (-2.57, home_y), (-4.9, home_y))
        arrows = await self.draw_route(route if into_cpu else tuple(reversed(route)),
                                       CONTROL if home_y > 0 else ESTIMATE if home_y == -0.14 else LOGGER,
                                       seconds=0.36)
        await self.move_job(token, *destinations, seconds=1.0)
        await self.hide(arrows, seconds=0.18)

    async def inversion(self):
        homes = [station(title, -4.9, y, 2.66, 1.18, color)
                 for title, y, color in (("C / HIGH", 1.42, CONTROL),
                                         ("E / MEDIUM", 0, ESTIMATE),
                                         ("L / LOW", -1.42, LOGGER))]
        await self.show(homes, station("ONE CPU", -0.3, 0, 3.15, 2.55),
                        station("SHARED SPI", 4.65, 0, 3.15, 1.75, LOGGER),
                        words("The lock owner and the running task are different facts.",
                              y=2.24, size=13, color=MUTED))
        await self.draw_route(((1.33, -0.14), (3.02, -0.14)), MUTED)
        c, e, l = job("C", -4.9, 1.28, CONTROL), job("E", -4.9, -0.14, ESTIMATE), job("L", -4.9, -1.56, LOGGER, 1)
        owner = words("OWNER: L", 4.65, -0.10, 21, LOGGER, 2.8)
        await self.show(c, e, l, owner,
                        words("resource ownership", 4.65, -0.53, 11, MUTED, 2.8))
        await self.caption("1. C runs, but SPI is still owned by L.")
        await self.resource_move(c, 1.28)
        await self.pulse_data(((1.33, -0.14), (3.02, -0.14)), CONTROL, seconds=0.7)
        await self.resource_move(c, 1.28, False)
        await self.hide(homes[0][1])
        c_state = words("C / HIGH / WAITING", -4.9, 1.80, 11, CONTROL, 2.42)
        await self.show(c_state)
        await self.caption("2. C waits. E runs, while the lock owner L makes no progress.",
                           "This dependency can delay the highest-priority task.", BAD)
        await self.resource_move(e, -0.14)
        await self.compute(e, 0, 2, seconds=1.8)
        dependency_note = words("C waits for L's resource release", 4.65, -1.33, 14, BAD, 3.3)
        await self.show(dependency_note)
        await self.pause(1.5)
        await self.caption("3. A resource protocol raises L's effective priority.",
                           "Generic priority inheritance; an async mutex does not inherently supply it.")
        await self.hide(homes[2][1])
        boosted_title = words("L / EFFECTIVE HIGH", -4.9, -1.04, 11, LOGGER, 2.42)
        await self.show(boosted_title)
        await self.resource_move(e, -0.14, False)
        await self.resource_move(l, -1.56)
        await self.compute(l, 1, 4, seconds=2.0)
        await self.hide(owner, dependency_note, boosted_title)
        free = words("FREE", 4.65, -0.10, 21, GOOD, 2.8)
        await self.show(free, homes[2][1])
        await self.caption("4. L completes its protected work and releases SPI.",
                           "The preserved L computation made the progress that C needed.")
        await self.resource_move(l, -1.56, False)
        await self.hide(c_state)
        await self.show(homes[0][1])
        await self.resource_move(c, 1.28)
        await self.pulse_data(((1.33, -0.14), (3.02, -0.14)), CONTROL, seconds=0.7)
        await self.hide(free)
        await self.show(words("OWNER: C", 4.65, -0.10, 21, CONTROL, 2.8))
        await self.compute(c, 0, 3, seconds=1.4)
        await self.caption("5. C acquires SPI and can finally continue.",
                           "Bound lock hold time, service queues and device transactions as well as CPU execution.")

    async def timeouts(self):
        await self.show(panel(-2.1, 0.69, 8.15, 2.65),
                        words("CALLING TASK / TIMEOUT WRAPPER", -2.1, 1.72, 15, CONTROL, 7.5),
                        station("CPU / INNER COMPUTATION", -2.1, 0.53, 3.7, 1.45),
                        station("TIMER EVENT", 4.4, 0.93, 3.3, 1.5, BAD))
        l = job("L", -2.1, 0.33, LOGGER)
        busy = words("synchronous work is still running", -2.1, -0.38, 13, LOGGER, 7.5)
        armed = words("ARMED", 4.4, 0.73, 20, MUTED, 2.9)
        await self.show(l, busy, armed)
        timer_route = ((2.71, 0.93), (2.36, 0.93), (2.36, 1.52), (1.99, 1.52))
        await self.draw_route(timer_route, BAD, seconds=0.45)
        await self.caption("1. The timeout wrapper is waiting for its inner computation to return.")
        await self.compute(l, 0, 1, seconds=1.0)
        await self.hide(armed, seconds=0.2)
        await self.show(words("READY", 4.4, 0.73, 20, BAD, 2.9), seconds=0.35)
        await self.pulse_data(timer_route, BAD, seconds=0.7)
        await self.caption("An async timeout is not a CPU execution budget.",
                           "2. The timer is ready, but the inner computation keeps using the CPU.", BAD)
        await self.compute(l, 1, 3, seconds=1.5)
        await self.show(card("INDEPENDENTLY SCHEDULABLE MONITOR", "A separate enforcement boundary, with platform and interrupt-masking limits",
                             0, -1.53, 10.7, 1.02, GOOD))
        await self.pause(1.0)
        await self.compute(l, 3, 4, seconds=0.8)
        await self.hide(busy, seconds=0.2)
        await self.show(words("control returns to the timeout wrapper", -2.1, -0.38,
                              13, GOOD, 7.5), seconds=0.35)
        await self.caption("3. The wrapper can check readiness only after the inner call returns.",
                           "Event readiness and execution-time enforcement are different contracts.")
        await self.pause(1.0)
        await self.caption("Now consider cancellation after an external operation starts.")
        await self.hide(list(self.content), seconds=0.35)
        await self.show(words("CANCELLATION AFTER EXTERNAL EFFECTS BEGIN", y=2.06,
                              size=16, color=MUTED),
                        station("CALLER", -4.65, 0.94, 3.1, 1.2, CONTROL),
                        station("DRIVER", 0, 0.94, 3.7, 1.2, MUTED),
                        station("DEVICE", 4.65, 0.94, 3.1, 1.2, ESTIMATE))
        caller_state = words("REQUEST ACTIVE", -4.65, 0.75, 14, CONTROL, 2.8)
        driver_state = words("operation in flight", 0, 0.75, 14, MUTED, 3.35)
        device_state = words("IDLE", 4.65, 0.75, 14, MUTED, 2.8)
        await self.show(caller_state, driver_state, device_state)
        issue_route = ((-3.06, 0.94), (-1.89, 0.94))
        device_route = ((1.89, 0.94), (3.06, 0.94))
        await self.draw_route(issue_route, CONTROL, seconds=0.4)
        await self.draw_route(device_route, ESTIMATE, seconds=0.4)
        await self.caption("1. A request reaches the driver, which issues a device command.")
        await self.pulse_data(issue_route, CONTROL, seconds=0.8)
        await self.pulse_data(device_route, ESTIMATE, seconds=0.8)
        await self.hide(device_state, seconds=0.2)
        await self.show(words("COMMAND ISSUED", 4.65, 0.75, 14, ESTIMATE, 2.8), seconds=0.35)
        await self.caption("2. Cancellation stops the caller's operation; the device may already be acting.")
        await self.hide(caller_state, driver_state, seconds=0.2)
        await self.show(words("REQUEST CANCELLED", -4.65, 0.75, 14, BAD, 2.8),
                        words("cleanup / abort policy", 0, 0.75, 14, GOOD, 3.35), seconds=0.35)
        await self.pause(1.0)
        await self.show(card("DRIVER + PROTOCOL CONTRACT", "Specify abort, completion, cleanup and resource lifetimes",
                             0, -1.03, 11.8, 1.2, GOOD),
                        words("A cancellation request does not rewind the device.", y=-2.12,
                              size=18, color=BAD))
        await self.draw_route(((4.65, 0.30), (4.65, -0.39)), ESTIMATE, seconds=0.4)
        await self.caption("Define what cancellation means for the actual driver and device.",
                           "Immediate abort, deferred cleanup and completion can have different timing costs.")

    async def assurance(self):
        rows = (
            ("MEMORY", "Who may access this memory?", "Enforced access rules + sound APIs", CONTROL,
             "Memory protection comes from the language, APIs and isolation policy.",
             "Both threads and async tasks can operate within strong memory contracts."),
            ("RESOURCES", "Who can block whom?", "Enforced resource protocol", LOGGER,
             "Resource rules can exclude specific classes of misuse.",
             "The guarantee applies to the enforced model, not arbitrary channels or external devices."),
            ("TIMING", "Will the job finish in time?", "Execution + blocking + interference", ESTIMATE,
             "Deadlines need execution bounds and scheduling analysis.",
             "Runtime protection helps enforce assumptions; memory safety alone does not."),
            ("SYSTEM", "Is the physical response correct?", "Requirements + verification", GOOD,
             "Safe memory access does not prove correct or timely control.",
             "Algorithms, data age, fault handling and the physical system all remain in scope."),
        )
        for number, (title, question, evidence, color, main, detail) in enumerate(rows):
            y = 1.64 - number * 1.14
            await self.show(panel(0, y, 12.65, 0.94),
                            panel(-6.27, y, 0.07, 0.94, color, color, stroke=0),
                            words(title, -4.89, y, 16, color, 2.45),
                            words(question, -0.91, y + 0.18, 18, INK, 5.5),
                            words(evidence, 2.0, y - 0.23, 13, MUTED, 7.45))
            await self.caption(main, detail)
            await self.pause(2.7)
        await self.caption("Ask what is safe by construction, and under which assumptions.",
                           "The guarantee must name a property; neither execution model proves whole-system safety.")

    async def control_boundary(self):
        await self.show(card("SENSOR CAPTURE", "Timestamp / DMA", -4.75, 1.35, 3.1, 1.45, ESTIMATE),
                        station("C / HIGH-PRIORITY CONTROL", 0, 1.35, 3.85, 1.45, CONTROL),
                        card("ACTUATOR", "Bound command latency", 4.75, 1.35, 3.1, 1.45, CONTROL),
                        station("L / BACKGROUND ASYNC", -4.75, -1.45, 3.1, 1.45, LOGGER),
                        station("E / ESTIMATION", 0, -1.45, 3.85, 1.45, ESTIMATE),
                        card("MONITOR / PROTECTION", "Independent fault path", 4.75, -1.45, 3.1, 1.45, GOOD))
        c, e = job("C", 0, 1.13, CONTROL), job("E", 0, -1.67, ESTIMATE)
        queue = [panel(x, -1.42, 0.39, 0.29, PANEL, LOGGER)
                 for x in (-5.35, -4.75, -4.15)]
        await self.show(c, e, queue,
                        words("capacity / overflow policy", -4.75, -1.96, 10, MUTED, 2.8),
                        words("sample", -2.58, 1.81, 10, ESTIMATE, 1.1),
                        words("command", 2.58, 1.81, 10, CONTROL, 1.1),
                        words("bounded log queue", -3.33, 0.33, 11, LOGGER, 2.65),
                        words("coherent snapshot", 1.35, 0.24, 11, ESTIMATE, 2.4),
                        words("timestamp + age", 1.35, -0.14, 11, MUTED, 2.4),
                        words("detect", 5.71, 0.24, 11, GOOD, 1.25),
                        words("contain", 5.71, -0.14, 11, GOOD, 1.25))
        sample_route = ((-3.16, 1.35), (-1.97, 1.35))
        command_route = ((1.97, 1.35), (3.16, 1.35))
        snapshot_route = ((0, -0.68), (0, 0.58))
        log_route = ((-1.22, 0.58), (-1.22, 0.04), (-4.75, 0.04), (-4.75, -0.68))
        fault_route = ((4.75, -0.68), (4.75, 0.58))
        for route, color in ((sample_route, ESTIMATE), (command_route, CONTROL),
                             (snapshot_route, ESTIMATE), (log_route, LOGGER), (fault_route, GOOD)):
            await self.draw_route(route, color, seconds=0.45)
        await self.caption("Start with the control path's end-to-end deadline.",
                           "Release, sensor wait, computation, communication and actuator update all count.")
        await self.pulse_data(sample_route, ESTIMATE, seconds=1.2)
        await self.caption("1. E runs on the shared CPU and publishes a coherent snapshot.",
                           "The sample and snapshot carry timestamps; C must enforce their maximum age.")
        await self.compute(e, 0, 4, seconds=2.0)
        await self.pulse_data(snapshot_route, ESTIMATE, seconds=1.2)
        await self.caption("2. C runs next, computes the control update, and issues a command.",
                           "These are sequential computations on one CPU; the diagram shows their data contracts.")
        await self.compute(c, 0, 4, seconds=2.0)
        await self.pulse_data(command_route, CONTROL, seconds=1.2)
        applied = words("command applied", 4.75, 0.92, 11, GOOD, 2.7)
        await self.show(applied, seconds=0.35)
        await self.caption("3. A log copy enters a bounded queue; background service follows later.",
                           "Choose overflow and backpressure behavior that preserves the control deadline.")
        await self.pulse_data(log_route, LOGGER, seconds=1.8)
        await self.play_for(queue[1].animate.set_fill(LOGGER, opacity=1), seconds=0.4)
        await self.pause(1.0)
        await self.play_for(queue[1].animate.set_fill(PANEL, opacity=1), seconds=0.4)
        await self.show(words("copy drained later", -4.75, -1.73, 10, LOGGER, 2.8), seconds=0.35)
        await self.caption("4. A detected fault uses a separately specified containment path.",
                           "Monitor scheduling, fault coverage and platform limits need their own evidence.")
        await self.pulse_data(fault_route, GOOD, seconds=1.2)
        await self.hide(applied, seconds=0.2)
        await self.show(words("specified fault response", 4.75, 0.92, 10, GOOD, 2.8), seconds=0.35)
        await self.show(words("Choose execution boundaries to match urgency and containment needs.",
                              y=2.27, size=12, color=GOOD, width=12.5))
        await self.caption("Prove execution, blocking and data-age bounds.",
                           "Async, threads or a hybrid can fit when their concrete contracts support the system.", GOOD)
