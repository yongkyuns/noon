"""Async, threads, and the deadline — an original, 13-chapter Noon tutorial.

Run this file in Noon's Python editor. The complete lesson is 328 authored
seconds. Set START_CHAPTER and END_CHAPTER to the same number to study one
chapter; SPEED changes presentation speed, never the illustrated microseconds.

This is educational animation, not a controller implementation or a benchmark.
All three response-time examples use the same hypothetical single-core model.
Domain numbers are computed below; Noon owns animation, time, and rendering.
No frame callbacks, replacement renderer, or external assets are required.

Primary technical references and a chapter guide accompany this file in
async_threads_realtime.md. In particular, async state representation does not
determine scheduling policy; a Ready await need not yield; separate interrupt
executors may preempt; and Rust memory safety does not prove deadlines.
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
    ("Two independent choices", 12, "choices"),
    ("Where suspended state lives", 22, "memory"),
    ("Waiting releases the CPU", 24, "waiting"),
    ("An await is only a possible yield", 28, "await_paths"),
    ("Priority cannot interrupt a poll", 34, "long_poll"),
    ("Bounded cooperative work can succeed", 25, "bounded_poll"),
    ("Preemption changes the blocking bound", 35, "preemption"),
    ("Three architectures on one core", 28, "architectures"),
    ("What gets faster?", 22, "performance"),
    ("A mutex can defeat priority", 28, "inversion"),
    ("Timeout and cancellation are contracts", 24, "timeouts"),
    ("What can be guaranteed?", 26, "assurance"),
    ("Build the control boundary first", 20, "control_boundary"),
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


def link(a, b, color=MUTED, width=1.7, tip=0.13):
    """An authored diagram connector made from ordinary Line primitives."""
    dx, dy = b[0] - a[0], b[1] - a[1]
    length = sqrt(dx * dx + dy * dy)
    ux, uy = dx / length, dy / length
    base = (b[0] - tip * ux, b[1] - tip * uy)
    return (
        rule(a, b, color, width),
        rule((base[0] - tip * uy / 2, base[1] + tip * ux / 2), b, color, width),
        rule((base[0] + tip * uy / 2, base[1] - tip * ux / 2), b, color, width),
    )


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


def authored_objects(*items):
    # Expand only authored tuples/lists. Native Noon families (for example Code)
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

    async def show(self, *items, seconds=0.45):
        objects = authored_objects(*items)
        self.content.extend(objects)
        await self.play_for(*(FadeIn(obj) for obj in objects), seconds=seconds)

    async def hide(self, *items, seconds=0.3):
        objects = authored_objects(*items)
        await self.play_for(*(FadeOut(obj) for obj in objects), seconds=seconds)
        removed = {id(obj) for obj in objects}
        self.content = [obj for obj in self.content if id(obj) not in removed]

    async def caption(self, main, detail="", color=INK):
        new = [words(main, y=-2.99, size=21, color=color),
               words(detail or " ", y=-3.43, size=13, color=MUTED)]
        await self.play_for(*(FadeOut(obj) for obj in self.captions),
                            *(FadeIn(obj) for obj in new), seconds=0.25)
        self.captions = new

    async def begin_chapter(self, number, title):
        old = self.content + self.captions + self.heading
        self.content, self.captions = [], []
        self.heading = [words(title, -6.48, 3.20, 28, width=12.6, left=True),
                        words("%02d / 13" % number, 5.93, 3.72, 12, MUTED)]
        await self.play_for(*(FadeOut(obj) for obj in old),
                            *(FadeIn(obj) for obj in self.heading), seconds=0.4)

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
                 words("ONE CORE  /  illustrative timing bounds  /  not a benchmark",
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
            if remaining > 0.000001:
                await self.wait(remaining)

    async def choices(self):
        await self.show(words("STATE REPRESENTATION", -3.25, 2.05, 14, MUTED),
                        words("SCHEDULING POLICY", 3.25, 2.05, 14, MUTED))
        await self.show(card("Thread stack", "Saved call stack", -3.25, 1.10, 4.65),
                        card("Future state", "Saved suspension state", -3.25, -0.3, 4.65,
                             color=LOGGER))
        await self.caption("Where a task keeps its state is one decision.")
        await self.pause(1.6)
        await self.show(card("Cooperative", "Switch at cooperative boundaries", 3.25, 1.10, 4.65),
                        card("Preemptive", "Interrupt lower-priority execution", 3.25, -0.3,
                             4.65, color=LOGGER))
        await self.show(link((-0.87, 1.10), (0.87, 1.10)),
                        link((-0.87, -0.3), (0.87, -0.3)),
                        link((-0.87, 0.95), (0.87, -0.15), LOGGER),
                        link((-0.87, -0.15), (0.87, 0.95), CONTROL))
        await self.caption("When another task may run is another.",
                           "Async can exist inside a preemptive system.")
        await self.show(words("Rust's ownership rules are a third, separate axis.",
                              y=-1.65, size=20, color=GOOD))

    async def memory(self):
        await self.show(words("CONVENTIONAL THREADS", -3.35, 2.1, 18),
                        words("STACKLESS FUTURES", 3.35, 2.1, 18),
                        rule((0, 2.2), (0, -1.35)))
        stacks, active = [], []
        for x, (initial, _, color) in zip((-5.35, -3.4, -1.45), ROLES):
            stacks += [panel(x, 0.32, 1.36, 2.25, PANEL, color),
                       words(initial, x, 1.65, 17, color),
                       words("reserved", x, -1.11, 10, MUTED, 1.65)]
            for j in range(3):
                active.append(panel(x, -0.56 + j * 0.28, 1.12, 0.21,
                                    color, color, opacity=0.30, stroke=0))
        await self.show(stacks, active)
        await self.caption("A suspended thread retains its stack.",
                           "The reserved stack must cover its deepest applicable execution.")
        await self.pause(2.3)
        for x, (initial, _, color) in zip((1.45, 3.35, 5.25), ROLES):
            await self.show(card(initial + " future", "resume state", x, 0.9, 1.64,
                                 0.85, color), seconds=0.3)
        active_stack = (panel(3.35, -0.55, 5.55, 1.05),
                        words("ACTIVE EXECUTION STACK", 3.35, -0.24, 16))
        await self.show(active_stack)
        running = badge("L  RUNNING", 3.35, -0.56, LOGGER, 2.3)
        await self.show(running)
        await self.caption("A suspended future retains what it needs to resume.",
                           "Locals live across awaits may include large buffers.")
        await self.pause(2.2)
        await self.hide(running)
        await self.show(badge("C  RUNNING", 3.35, -0.56, CONTROL, 2.3))
        await self.caption("Many waiting activities can reserve less stack memory.",
                           "Futures still need execution stacks; preemption adds nesting.")
        await self.show(words("Both models can allocate statically.", y=-1.75, size=21,
                              color=GOOD),
                        words("Schematic sizes: no byte-count comparison is implied.",
                              y=-2.2, size=12, color=MUTED))

    async def waiting(self):
        controller = card("1 · C starts I/O", "RUNNING", -4.65, 1.5, 3.15, 1.15, CONTROL)
        cpu = card("ONE CPU", "One execution stream", 0, 1.5, 3.6, 1.15, INK)
        peripheral = card("SPI + DMA", "Hardware transaction", 4.65, 1.5, 3.15, 1.15,
                          ESTIMATE)
        await self.show(words("EVENT SEQUENCE: issue → suspend → wake → resume", y=2.26,
                              size=12, color=MUTED), controller, cpu, peripheral,
                        link((-3.04, 1.5), (-1.83, 1.5), CONTROL),
                        link((1.83, 1.5), (3.04, 1.5), ESTIMATE))
        await self.caption("1. Start the sensor transfer and register a wakeup.")
        await self.pause(2)
        wait = card("2 · C WAITING", "Future returned Pending", -4.65, -0.25, 3.15,
                    1.15, CONTROL)
        log = card("L · RUNNING", "Useful work on the CPU", 0, -0.25, 3.6, 1.15, LOGGER)
        dma = panel(4.65, -0.25, 3.15, 0.5, PANEL, ESTIMATE)
        progress = panel(3.2, -0.25, 0.16, 0.28, ESTIMATE, ESTIMATE, stroke=0)
        await self.show(wait, log, dma, progress,
                        link((-4.65, 0.88), (-4.65, 0.38), CONTROL))
        await self.caption("2. Pending returns control to the executor.",
                           "The peripheral progresses while the CPU runs another task.")
        await self.play_for(progress.animate.move_to((6.0, -0.25, 0)),
                            seconds=3, rate_func=linear)
        irq = badge("completion IRQ", 4.65, -1.37, ESTIMATE, 2.8)
        ready = badge("3 · C READY", -4.65, -1.37, CONTROL, 2.6)
        await self.show(irq, ready, link((4.65, -0.58), (4.65, -1.1), ESTIMATE),
                        link((3.15, -1.37), (-3.25, -1.37), CONTROL))
        await self.caption("3. Completion wakes the task; it becomes ready.",
                           "A wakeup makes work eligible. The scheduler chooses when it runs.")
        await self.pause(2.5)
        await self.hide(log)
        await self.show(card("4 · C RUNNING", "Poll resumes with the result", 0, -0.25,
                             3.6, 1.15, CONTROL),
                        link((-3.25, -1.37), (-0.7, -0.88), CONTROL))
        await self.caption("4. The scheduler selects C and execution resumes.",
                           "A blocking RTOS driver can use the same interrupt + DMA technique.")
        await self.show(words("Waiting overlaps with work. CPU computation is still serial.",
                              y=-2.15, size=17, color=GOOD))

    async def await_paths(self):
        await self.show(panel(0, 1.26, 12.6, 1.93),
                        words("ILLUSTRATIVE RUST", -5.9, 2.02, 11, MUTED, left=True))
        code = Code(code_string="loop {\n    let packet = rx.receive().await;\n    process_packet(packet);\n}",
                    language="rust", formatter_style="monokai", background="rectangle",
                    background_config={"fill_opacity": 0, "stroke_opacity": 0},
                    paragraph_config={"font": "DejaVu Sans Mono", "font_size": 20})
        if code.width > 10.7:
            code.scale(10.7 / code.width)
        code.move_to((0, 1.18, 0))
        await self.show(code)
        await_line = code.code_lines.submobjects[1]
        await self.show(rule((-5.85, await_line.get_center()[1]),
                            (-5.55, await_line.get_center()[1]), CONTROL, 4))
        ready = card("Ready", "Queued packet exists", -4.45, -0.48, 3.25, 0.96, LOGGER)
        compute = card("process_packet()", "Synchronous computation", 0, -0.48, 3.55,
                       0.96, LOGGER)
        again = card("receive().await", "May be Ready again", 4.45, -0.48, 3.25, 0.96,
                     LOGGER)
        await self.show(ready)
        await self.caption("Ready: execution continues inside the current poll.")
        first_link = link((-2.79, -0.48), (-1.82, -0.48), LOGGER)
        await self.show(compute, first_link)
        await self.pause(1.7)
        poll_bracket = (link((1.82, -0.48), (2.79, -0.48), LOGGER),
                        rule((-6.05, -1.07), (6.05, -1.07), LOGGER),
                        words("ONE TOP-LEVEL POLL CAN INCLUDE ALL OF THIS", y=-1.33,
                              size=13, color=LOGGER))
        await self.show(again, poll_bracket)
        await self.caption("An await in a loop does not guarantee a yield.",
                           "A ready receive can repeat without returning to the executor.")
        await self.pause(3)
        await self.hide(ready, compute, again, first_link, poll_bracket)
        # Build the alternative path in its own row; the code remains unchanged.
        await self.show(card("Pending", "No packet is available", -4.45, -0.5, 3.3, 1.0,
                             CONTROL),
                        card("Return to executor", "Top-level poll ends", 0, -0.5, 3.55,
                             1.0, CONTROL),
                        card("Another ready task", "Can now run", 4.45, -0.5, 3.3, 1.0,
                             GOOD),
                        link((-2.77, -0.5), (-1.82, -0.5), CONTROL),
                        link((1.82, -0.5), (2.77, -0.5), GOOD))
        await self.caption("Pending: control returns to the executor.",
                           "The useful bound is the longest complete poll, including nested work.")
        await self.show(words("Q = maximum duration of one complete task poll", y=-1.73,
                              size=20, color=GOOD),
                        words("This duration needs evidence; async syntax does not prove it.",
                              y=-2.17, size=13, color=MUTED))

    @staticmethod
    def tx(microseconds):
        return -5.45 + 11.3 * microseconds / AXIS_US

    async def trace_shell(self):
        await self.show(words("C: period 1,000 µs  |  deadline 250 µs  |  execution 80 µs",
                              y=2.12, size=14, color=CONTROL),
                        words("One release  ·  dispatch 5 µs  ·  no other interference or I/O waits",
                              y=1.72, size=12, color=MUTED),
                        panel(0, 0.27, 12.7, 2.08),
                        words("CPU", -6.03, 0.37, 12, MUTED, 0.65),
                        rule((self.tx(0), -0.08), (self.tx(AXIS_US), -0.08)))
        axis = []
        for tick in (0, 100, 200, 300, 400, 500, 600, 650):
            axis += [rule((self.tx(tick), -0.08), (self.tx(tick), -0.23)),
                     words(str(tick), self.tx(tick), -0.48, 11, MUTED)]
        axis += [rule((self.tx(DEADLINE_US), -0.04),
                      (self.tx(DEADLINE_US), 1.05), BAD, 2.2),
                 words("DEADLINE 250", self.tx(DEADLINE_US), 1.15, 12, BAD),
                 words("µs after release", 4.85, 1.13, 11, MUTED),
                 words("C becomes READY at t = 0", -3.6, -1.05, 14, CONTROL)]
        cursor = rule((self.tx(0), 0.06), (self.tx(0), 0.82), INK, 3)
        await self.show(axis, cursor)
        return cursor

    async def trace_bar(self, interval, color, title="", opacity=0.85):
        start, end = interval
        x1, x2 = self.tx(start), self.tx(end)
        bar = panel((x1 + x2) / 2, 0.37, x2 - x1, 0.5, color, color,
                    opacity=opacity, stroke=0)
        objects = [bar]
        if title:
            objects.append(words(title, (x1 + x2) / 2, 0.37, 15, INK if opacity < 0.5 else BG,
                                 max(0.12, x2 - x1 - 0.08)))
        await self.show(objects, seconds=0.3)
        return objects

    async def advance(self, cursor, begin_us, end_us):
        # Linear presentation time while the execution cursor moves. Annotation
        # holds are explicit pauses between steps, never distorted bar widths.
        await self.play_for(cursor.animate.move_to((self.tx(end_us), 0.44, 0)),
                            seconds=(end_us - begin_us) * 0.016, rate_func=linear)

    async def trace_result(self, example):
        blocking = example["blocking"][1]
        color = GOOD if example["met"] else BAD
        equation = "%d + %d + %d = %d µs" % (
            blocking, DISPATCH_US, CONTROL_US, example["finish"])
        outcome = ("MET · %d µs margin" % example["margin"] if example["met"]
                   else "MISSED by %d µs" % -example["margin"])
        await self.show(words(equation, -1.9, -1.63, 27, color, 7.7),
                        badge(outcome, 4.1, -1.63, color, 3.95))

    async def long_poll(self):
        example = response_example(500)
        cursor = await self.trace_shell()
        await self.caption("1. C is ready, but L is inside synchronous encoding.",
                           "One cooperative executor; C has higher selection priority.")
        await self.trace_bar(example["blocking"], LOGGER, "L · one long poll")
        await self.pause(2)
        await self.advance(cursor, 0, DEADLINE_US)
        await self.caption("2. The deadline arrives while the same poll is running.",
                           "Task selection priority cannot interrupt the current poll.", BAD)
        await self.pause(2.5)
        await self.advance(cursor, DEADLINE_US, 500)
        await self.caption("3. L finally returns. The executor can select C.",
                           "The thin gray interval is the assumed 5 µs dispatch cost.")
        await self.trace_bar(example["dispatch"], MUTED)
        await self.advance(cursor, 500, 505)
        await self.trace_bar(example["control"], CONTROL, "C")
        await self.advance(cursor, 505, 585)
        await self.trace_result(example)
        await self.show(words("E is present in the system, but adds no interference in this example.",
                              y=-2.2, size=12, color=MUTED))
        await self.caption("4. C completes at 585 µs: this job missed its deadline.",
                           "Low average CPU use would not repair this blocking interval.", BAD)

    async def bounded_poll(self):
        example = response_example(50)
        cursor = await self.trace_shell()
        await self.caption("1. Bound every relevant poll to at most 50 µs.",
                           "Required assumption: C is selected at the next scheduling opportunity.")
        await self.trace_bar(example["blocking"], LOGGER, "L")
        await self.show(words("residual poll ≤ 50 µs", -3.55, 0.98, 12, LOGGER))
        await self.pause(3)
        await self.advance(cursor, 0, 50)
        await self.caption("2. The poll returns; C can run next.",
                           "A queued-data loop needs a verified bound or an explicit scheduling opportunity.")
        await self.trace_bar(example["dispatch"], MUTED)
        await self.advance(cursor, 50, 55)
        await self.trace_bar(example["control"], CONTROL, "C")
        await self.pause(1.4)
        await self.advance(cursor, 55, 135)
        await self.trace_result(example)
        await self.show(words("Same scale, same deadline, same 80 µs of control computation.",
                              y=-2.2, size=13, color=MUTED))
        await self.caption("3. C completes at 135 µs, with 115 µs to spare.",
                           "A fair queue with other tasks ahead of C needs a larger interference bound.", GOOD)

    async def preemption(self):
        example = response_example(10)
        cursor = await self.trace_shell()
        await self.caption("1. Keep L's long computation. Change the preemption boundary.",
                           "Assume all applicable lower-priority blocking is bounded by 10 µs.")
        await self.trace_bar(example["blocking"], LOGGER)
        await self.show(words("≤ 10 µs blocking", -3.9, 0.98, 12, LOGGER))
        await self.pause(2.8)
        await self.advance(cursor, 0, 10)
        await self.caption("2. Higher-priority execution interrupts L.",
                           "Interrupt masking and resource protocols determine the remaining bound.")
        await self.trace_bar(example["dispatch"], MUTED)
        await self.advance(cursor, 10, 15)
        await self.trace_bar(example["control"], CONTROL, "C")
        await self.pause(2)
        await self.advance(cursor, 15, 95)
        await self.trace_result(example)
        await self.caption("3. C completes at 95 µs. L can resume afterward.",
                           "L resumes from its interrupted computation; it did not have to finish the poll.", GOOD)
        await self.pause(3)
        await self.trace_bar((95, 585), LOGGER, "L resumes · later work", opacity=0.24)
        await self.show(badge("Embassy interrupt executor", -3.1, -2.21, CONTROL, 5.75),
                        badge("Preemptive RTOS thread", 3.1, -2.21, ESTIMATE, 5.75))
        await self.caption("This benefit belongs to preemptive scheduling.",
                           "Both architectures can provide it. These figures do not measure their overheads.")
        await self.pause(4)
        await self.caption("Lower dispatch cost helps; bounded interference decides feasibility.",
                           "Real analysis also includes higher-priority work, interrupts, jitter and resource waits.")

    async def architectures(self):
        await self.show(words("PRIORITY", -5.77, 2.12, 12, MUTED),
                        link((-5.77, -1.86), (-5.77, 1.8), MUTED),
                        card("ONE CORE", "One execution stream", 4.8, 0.1, 2.7, 1.55, INK))
        single = (panel(-0.9, 0.1, 7.5, 3.67),
                  words("ONE EXECUTOR", -0.9, 1.54, 16, MUTED),
                  card("C · control", "", -0.9, 0.73, 6.3, 0.7, CONTROL),
                  card("E · estimate", "", -0.9, -0.12, 6.3, 0.7, ESTIMATE),
                  card("L · log", "", -0.9, -0.97, 6.3, 0.7, LOGGER),
                  link((2.9, 0.1), (3.4, 0.1), MUTED))
        await self.show(single)
        await self.caption("One executor: cooperative between its task polls.",
                           "A task's selection order does not create preemption inside a poll.")
        await self.pause(3.7)
        await self.hide(single)
        multiple = []
        for y, title, detail, color in (
                (1.32, "HIGH IRQ EXECUTOR · C", "Control poll", CONTROL),
                (0.10, "MEDIUM IRQ EXECUTOR · E", "Estimator polls", ESTIMATE),
                (-1.12, "BACKGROUND EXECUTOR · L", "Telemetry / logging polls", LOGGER)):
            multiple += authored_objects(card(title, detail, -0.9, y, 7.5, 0.94, color),
                                         link((2.9, y), (3.4, 0.1), color))
        await self.show(multiple)
        await self.caption("Different interrupt priorities can preempt across executors.",
                           "Embassy's Cortex-M InterruptExecutor supplies this scheduling boundary.")
        await self.pause(4)
        await self.show(words("Preemption still consumes stack space and dispatch work.",
                              y=-2.2, size=13, color=MUTED))
        await self.pause(2)
        await self.hide(multiple)
        hybrid = []
        for y, title, detail, color in (
                (1.32, "HIGH-PRIORITY THREAD · C", "Control", CONTROL),
                (0.1, "MEDIUM-PRIORITY THREAD · E", "Estimation", ESTIMATE),
                (-1.12, "LOW-PRIORITY THREAD", "Async executor: log / commands / telemetry", LOGGER)):
            hybrid += authored_objects(card(title, detail, -0.9, y, 7.5, 0.94, color),
                                       link((2.9, y), (3.4, 0.1), color))
        await self.show(hybrid)
        await self.caption("Async tasks can live inside RTOS scheduling boundaries.",
                           "Multiple executors or threads do not create multiple cores.")

    async def performance(self):
        boxes = (
            ("RAM", "Often less reserved stack", "Many waiting tasks; future sizes still matter", -3.27, 1.27, CONTROL),
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

    async def inversion(self):
        high = card("C · HIGH", "Needs SPI", -4.75, 1.25, 3.0, 0.95, CONTROL)
        low = card("L · LOW", "Owns SPI lock", -4.75, -1.35, 3.0, 0.95, LOGGER)
        resource = card("SPI RESOURCE", "Lock currently held by L", -0.15, -1.35,
                        3.65, 0.95, LOGGER)
        lower_link = link((-3.21, -1.35), (-2.02, -1.35), LOGGER)
        await self.show(words("HIGH > MEDIUM > LOW  /  one shared SPI resource", y=2.1,
                              size=14, color=MUTED),
                        high,
                        card("E · MEDIUM", "Ready to compute", -4.75, -0.05, 3.0, 0.95,
                             ESTIMATE),
                        low, resource,
                        card("CPU", "", 4.6, 0.05, 3.0, 2.9, INK),
                        lower_link)
        waiting = badge("C  WAITING for L", -0.15, 1.25, CONTROL, 3.65)
        await self.show(waiting,
                        link((-3.21, 1.25), (-2.02, 1.25), CONTROL),
                        link((-0.15, 0.98), (-0.15, -0.83), CONTROL))
        await self.caption("1. C blocks on a resource owned by lower-priority L.",
                           "Being ready to run and being able to obtain the resource are different.")
        await self.pause(2.5)
        medium = badge("E  RUNNING", 4.6, -0.47, ESTIMATE, 2.5)
        medium_link = link((-3.21, -0.05), (3.05, -0.05), ESTIMATE)
        await self.show(medium, medium_link)
        await self.caption("2. E runs, delaying the owner that C needs.",
                           "Priority inversion: C's resource wait depends on lower-priority progress.", BAD)
        await self.show(words("An async mutex does not inherently provide priority inheritance.",
                              y=-2.15, size=14, color=BAD))
        await self.pause(3.8)
        await self.hide(medium, medium_link)
        boosted = badge("L  BOOSTED", 4.6, -0.47, LOGGER, 2.5)
        boosted_link = link((1.74, -1.35), (3.05, -0.75), LOGGER)
        await self.show(boosted, boosted_link)
        await self.caption("3. A suitable resource protocol can let L release the lock.",
                           "Generic priority inheritance illustration; this is not an Embassy mutex feature.")
        await self.pause(3.2)
        await self.hide(waiting, boosted, high, low, resource, lower_link, boosted_link)
        await self.show(card("C · HIGH", "Using SPI", -4.75, 1.25, 3.0, 0.95, CONTROL),
                        card("L · LOW", "Lock released", -4.75, -1.35, 3.0, 0.95, LOGGER),
                        card("SPI RESOURCE", "Lock now held by C", -0.15, -1.35,
                             3.65, 0.95, CONTROL),
                        link((1.74, -1.35), (3.05, -0.75), CONTROL),
                        badge("C ACQUIRES SPI", -0.15, 1.25, CONTROL, 3.65),
                        badge("C  RUNNING", 4.6, -0.47, CONTROL, 2.5))
        await self.caption("Design the resource dependency together with the priority.",
                           "Dedicated critical resources, bounded transactions, and proven ceiling / inheritance protocols.")
        await self.pause(2)
        await self.caption("CPU preemption does not necessarily stop an in-flight bus transaction.",
                           "Bound the device operation, queueing, lock hold time and cleanup too.")

    async def timeouts(self):
        await self.show(panel(0, 0.92, 12.55, 2.37),
                        words("ONE COOPERATIVE EXECUTOR", y=1.82, size=14, color=MUTED),
                        panel(0, 0.91, 11.4, 0.57, LOGGER, LOGGER, opacity=0.75, stroke=0),
                        words("A single synchronous poll is still running", y=0.91,
                              size=19, color=BG),
                        rule((-3.45, 0.36), (-3.45, 1.37), BAD, 2.5),
                        words("timer expires here", -3.45, 0.04, 13, BAD))
        cursor = rule((-5.7, 0.53), (-5.7, 1.29), INK, 3)
        await self.show(cursor)
        await self.caption("A timer can become ready while the task is still inside poll().")
        await self.play_for(cursor.animate.move_to((-3.45, 0.91, 0)),
                            seconds=1.4, rate_func=linear)
        await self.caption("An async timeout is not a CPU execution budget.",
                           "The timeout wrapper cannot check its timer until polling returns.", BAD)
        await self.show(card("INDEPENDENTLY SCHEDULABLE MONITOR", "A separate enforcement boundary",
                             0, -1.12, 9.5, 1.02, GOOD),
                        words("Its interrupt masking and platform limits still need bounds.",
                              y=-2.06, size=13, color=MUTED))
        await self.pause(3.7)
        await self.caption("Now consider cancellation after an external operation starts.")
        await self.hide(list(self.content))
        await self.show(words("CANCELLATION AFTER EXTERNAL EFFECTS BEGIN", y=2.06,
                              size=16, color=MUTED),
                        card("select()", "Another branch completes", -4.6, 0.95, 3.05, 1.2,
                             CONTROL),
                        card("Operation future", "Dropped / cancelled", 0, 0.95, 3.75, 1.2,
                             BAD),
                        card("Peripheral", "Effects may have begun", 4.6, 0.95, 3.05, 1.2,
                             ESTIMATE),
                        link((-3.04, 0.95), (-1.92, 0.95), BAD),
                        link((1.92, 0.95), (3.04, 0.95), ESTIMATE))
        await self.caption("Cancellation may happen after bytes or commands have been issued.")
        await self.pause(2)
        await self.show(card("DRIVER + PROTOCOL CONTRACT", "Abort or finish safely; preserve buffer and protocol lifetimes",
                             0, -0.88, 11.9, 1.25, GOOD),
                        link((4.6, 0.28), (4.6, -0.21), ESTIMATE),
                        words("Dropping a future is not a hardware rollback.", y=-2.05,
                              size=20, color=BAD))
        await self.caption("Define what cancellation means for the actual driver and device.",
                           "Immediate abort, deferred cleanup and completion can have different timing costs.")

    async def assurance(self):
        rows = (
            ("OWNERSHIP", "Who may access this memory?", "Ownership + sound safe APIs", CONTROL,
             "Rust protects memory access under its language and API assumptions.",
             "The same protection is available to async tasks and Rust threads."),
            ("RESOURCES", "Who can block whom?", "Enforced resource protocol", LOGGER,
             "Resource rules can exclude specific classes of misuse.",
             "The guarantee applies to the enforced model, not arbitrary channels or external devices."),
            ("TIMING", "Will the job finish in time?", "Execution + blocking + interference", ESTIMATE,
             "Deadlines need execution bounds and scheduling analysis.",
             "Runtime protection helps enforce assumptions; memory safety alone does not."),
            ("SYSTEM", "Is the physical response correct?", "Requirements + verification", GOOD,
             "Correct ownership does not prove correct or timely control.",
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
                           "RTIC illustrates async tasks plus enforced priority / resource rules; whole-system safety is broader.")

    async def control_boundary(self):
        sensor = card("SENSOR CAPTURE", "Hardware timestamps / DMA", -4.78, 1.38, 3.05,
                      1.14, ESTIMATE)
        controller = card("HIGH-PRIORITY CONTROL", "Bound execution and input age", 0, 1.38,
                          4.03, 1.14, CONTROL)
        actuator = card("ACTUATOR", "Bound command latency", 4.78, 1.38, 3.05, 1.14, CONTROL)
        await self.show(sensor, controller, actuator,
                        link((-3.2, 1.38), (-2.06, 1.38), ESTIMATE),
                        link((2.06, 1.38), (3.2, 1.38), CONTROL),
                        words("sample", -2.63, 1.91, 11, MUTED),
                        words("command", 2.63, 1.91, 11, MUTED))
        await self.caption("Start with the control path's end-to-end deadline.",
                           "Release, sensor wait, computation, communication and actuator update all count.")
        await self.show(card("ESTIMATION / NAVIGATION", "Publish a coherent state snapshot", 0,
                             -0.26, 4.03, 0.97, ESTIMATE),
                        link((0, 0.28), (0, 0.76), ESTIMATE),
                        words("timestamp + maximum age", 2.0, 0.51, 12, ESTIMATE, 3.65))
        await self.show(card("BACKGROUND ASYNC", "Log / telemetry / commands", -4.58,
                             -1.72, 3.5, 1.05, LOGGER),
                        link((-1.84, 0.72), (-3.84, -1.12), LOGGER),
                        words("bounded queue", -4.6, -0.23, 12, LOGGER, 3.2),
                        words("defined overflow policy", -4.6, -0.62, 11, MUTED, 3.2))
        await self.caption("Use async where waiting and memory dominate.",
                           "Logging backpressure must have an explicit policy at the control boundary.")
        await self.show(card("MONITOR / PROTECTION", "Independent budget / fault path", 4.58,
                             -1.72, 3.5, 1.05, GOOD),
                        link((4.78, -1.12), (4.78, 0.76), GOOD),
                        words("detect / contain", 3.57, -0.72, 11, GOOD, 2.15))
        await self.pause(2.4)
        await self.caption("Use scheduling boundaries where urgency differs.",
                           "Urgency is not the same as criticality; fault containment needs its own design.")
        await self.pause(3.1)
        await self.caption("Prove execution, blocking and data-age bounds.",
                           "Async, threads or a hybrid can fit when their concrete contracts support the system.", GOOD)
