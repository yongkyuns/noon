"""One Extra Column: reproduced with Noon for demonstration purposes.

Based on CUDA Algorithm Lab by Arash (ourarash), under the MIT license.
Original visualization:
https://github.com/ourarash/cuda-algorithm-lab/blob/main/matrix_transpose/03_padded/03_transpose_bank_conflicts_visualization.html
Original CUDA kernel:
https://github.com/ourarash/cuda-algorithm-lab/blob/main/matrix_transpose/03_padded/03_transpose_padded.cu
Copyright and MIT permission notice: see cuda_transpose_banks.LICENSE.

Default: labeled CPU reference replay, not CUDA execution. To replay captured
addresses and values, replace GPU_TRACE=None with the assignment produced by
record_cuda.cu in tools/cuda-transpose. Service order and timing are schematic,
not recorded GPU scheduling or a benchmark.
"""
from noon import *
import re

SPEED = 1.0
COLUMN = 3
GPU_TRACE = None  # Replace with the recorder's assignment to use captured reads.
SYNTAX_HIGHLIGHT = True

# Strong enough to identify active lines, while staying quieter than syntax colors.
# Tune fill between 0.15 (softer) and 0.25 (stronger) for your display.
CODE_HIGHLIGHT_COLOR = color_from_hex("#68839B")
CODE_HIGHLIGHT_FILL = 0.20
CODE_HIGHLIGHT_STROKE = 0

# Every constructor receives a Noon Color, never an unconverted CSS string.
BG = color_from_hex("#0C1220")
PANEL = color_from_hex("#151F31")
PANEL_LIGHT = color_from_hex("#1E2B43")
EDGE = color_from_hex("#3D4F69")
INK = color_from_hex("#EDF3FC")
MUTED = color_from_hex("#A5B4CC")
CYAN = color_from_hex("#67DAEE")
GREEN = color_from_hex("#8CE2B5")
AMBER = color_from_hex("#F2C27B")
VIOLET = color_from_hex("#BAABFF")
PINK = color_from_hex("#F59CC5")
CORAL = color_from_hex("#FF968A")
ROW_COLORS = (CYAN, AMBER, VIOLET, PINK)

# Shared authored layout constants. Tests check these exact coordinates.
TILE_LEFT, TILE_TOP, CELL = 1.55, 1.62, 0.11
CODE_TOP, CODE_STEP, CODE_LINE_H = 2.09, 0.37, 0.32
QUEUE_W, QUEUE_H = 0.46, 0.24
REGISTER_SCALE = 0.58
REGISTER_CELL_W = 0.33
REGISTER_CELL_H = 0.43
ROUND_IN, ROUND_OUT = 0.16, 0.18  # Same schematic round durations in both runs.

SOURCE_STEPS = {
    "allocate": ("declaration",),
    "coordinates": ("input_x", "input_y"),
    "loop": ("load_loop",),
    "bounds": ("load_bounds",),
    "copy": ("load_dst", "load_src"),
    "barrier": ("sync",),
    "output_coordinates": ("output_x", "output_y"),
    "output_bounds": ("store_loop", "store_bounds"),
    "read": ("read_src",),
    "store": ("store_dst",),
}



def reference_trace(stride, column, base_word=0):
    """Evaluate BOTH loops for all 256 threads, with one block and N=32.

    Each read record is [lane, column, shared_word, output_index, value].
    Input[r,c] = 32*r+c. Shared addresses are in 32-bit words, not bytes.
    """
    tile, loads = {}, []
    for j in (0, 8, 16, 24):
        count = 0
        for ty in range(8):
            for tx in range(32):
                r = ty + j
                tile[base_word + r * stride + tx] = r * 32 + tx
                count += 1
        loads.append(count)
    output, selected = [None] * 1024, []
    for j in (0, 8, 16, 24):
        for ty in range(8):
            for tx in range(32):
                c = ty + j
                word = base_word + tx * stride + c
                output[c * 32 + tx] = tile[word]
                if c == column:
                    selected.append([tx, c, word, c * 32 + tx, tile[word]])
    valid = sum(output[c * 32 + r] == r * 32 + c
                for r in range(32) for c in range(32))
    return dict(stride=stride, base_word=base_word, loads=loads,
                reads=selected, verified_output_words=valid)


def get_trace(stride):
    if GPU_TRACE is None:
        return reference_trace(stride, COLUMN)
    if not isinstance(GPU_TRACE, dict) or GPU_TRACE.get("column") != COLUMN:
        raise ValueError("GPU_TRACE must match COLUMN; rerun the CUDA recorder.")
    payload = GPU_TRACE.get("runs")
    if not isinstance(payload, list) or any(not isinstance(r, dict) for r in payload):
        raise ValueError("GPU_TRACE.runs must be a list of run dictionaries.")
    runs = [r for r in payload if r.get("stride") == stride]
    if len(runs) != 1:
        raise ValueError("GPU_TRACE needs exactly one run for each stride: 32 and 33.")
    run = runs[0]
    base = run.get("base_word")
    if type(base) is not int or base < 0:
        raise ValueError("GPU trace has an invalid shared-memory base address.")
    expected = reference_trace(stride, COLUMN, base)
    if any(run.get(k) != v for k, v in expected.items()):
        raise ValueError("GPU trace does not match the fixed 32x32 transpose fixture.")
    return run


def service_batches(reads):
    """One distinct word per bank per schematic round; duplicates broadcast.

    The ordering within a bank is for explanation, NOT recorded hardware order.
    """
    banks = [{} for _ in range(32)]
    for lane, _, word, _, _ in reads:
        banks[word % 32].setdefault(word, []).append(lane)
    queues = [list(bank.values()) for bank in banks]
    return [[lane for q in queues if i < len(q) for lane in q[i]]
            for i in range(max((len(q) for q in queues), default=0))]


def tile_point(row, col):
    return (TILE_LEFT + (col + 0.5) * CELL,
            TILE_TOP - (row + 0.5) * CELL, 0)


def queue_point(lane):
    return (-6.05 + (lane % 8) * 0.71,
            -0.79 - (lane // 8) * 0.37, 0)


def bank_center(bank):
    return (0.95 + (bank % 8) * 0.75,
            1.76 - (bank // 8) * 0.64, 0)


def bank_point(bank):
    x, y, z = bank_center(bank)
    return (x, y - 0.095, z)


def register_point(lane):
    return (0.66 + (lane % 16) * 0.375,
            -1.53 - (lane // 16) * 0.57, 0)


def output_point(lane, column):
    return tile_point(column, lane)


def code_entries(stage, stride=32):
    declaration = ("__shared__ float tile[TILE_DIM][TILE_DIM];"
                   if stride == 32 else
                   "__shared__ float tile[TILE_DIM][TILE_DIM + 1];")
    if stage == "load":
        return [
            ("declaration", declaration),
            ("input_x", "int x = blockIdx.x * TILE_DIM + threadIdx.x;"),
            ("input_y", "int y = blockIdx.y * TILE_DIM + threadIdx.y;"),
            ("load_loop", "for (int j = 0; j < TILE_DIM; j += BLOCK_ROWS) {"),
            ("load_bounds", "  if (x < cols && y + j < rows) {"),
            ("load_dst", "    tile[threadIdx.y + j][threadIdx.x] ="),
            ("load_src", "        in[static_cast<size_t>(y + j) * cols + x];"),
            ("close_if", "  }"),
            ("close_loop", "}"),
            ("sync", "__syncthreads();"),
        ]
    if stage == "output":
        return [
            ("output_x", "x = blockIdx.y * TILE_DIM + threadIdx.x;"),
            ("output_y", "y = blockIdx.x * TILE_DIM + threadIdx.y;"),
            ("store_loop", "for (int j = 0; j < TILE_DIM; j += BLOCK_ROWS) {"),
            ("store_bounds", "  if (x < rows && y + j < cols) {"),
            ("store_dst", "    out[static_cast<size_t>(y + j) * rows + x] ="),
            ("read_src", "        tile[threadIdx.x][threadIdx.y + j];"),
            ("close_if", "  }"),
            ("close_loop", "}"),
        ]
    if stage == "read":
        return [
            ("store_dst", "out[static_cast<size_t>(y + j) * rows + x] ="),
            ("read_src", "    tile[threadIdx.x][threadIdx.y + j];"),
        ]
    raise ValueError("unknown code page")


def syntax_spans(source):
    """Nonoverlapping ASCII token spans; identifiers are never substring-matched.

    These are lexical colors, not a CUDA compiler or execution model.
    """
    spans = []
    pattern = r"//[^\n]*|[A-Za-z_]\w*(?:\.[xy])?|\b\d+\b"
    for match in re.finditer(pattern, source):
        token = match.group()
        kind = None
        if token.startswith("//"):
            kind = "comment"
        elif token == "__syncthreads":
            kind = "barrier"
        elif token in ("__shared__", "int", "float", "for", "if",
                       "static_cast", "size_t"):
            kind = "keyword"
        elif token.startswith(("threadIdx.", "blockIdx.")):
            kind = "index"
        elif token in ("TILE_DIM", "BLOCK_ROWS") or token.isdigit():
            kind = "constant"
        elif token in ("in", "tile", "out"):
            kind = token
        if kind:
            spans.append((match.start(), match.end(), kind))
    return spans


def ink_for_code(source):
    palette = {"comment": MUTED, "barrier": PINK, "keyword": VIOLET,
               "index": AMBER, "constant": AMBER,
               "in": CYAN, "tile": CYAN, "out": GREEN}
    return {"[%d:%d]" % (a, b): palette[k]
            for a, b, k in syntax_spans(source)}


def label(s, x=0, y=0, size=20, color=INK, width=12.6, left=False):
    m = Text(str(s), font="DejaVu Sans Mono", font_size=size, color=color)
    w = m.get_right()[0] - m.get_left()[0]
    if w > width:
        m.scale(width / w)
        w = width
    return m.move_to((x + w / 2 if left else x, y, 0))


def rect(x, y, w, h, color=EDGE, opacity=0.15, stroke=1):
    return Rectangle(width=w, height=h, color=color,
                     fill_opacity=opacity, stroke_width=stroke).move_to((x, y, 0))


def line(a, b, color=EDGE, width=1):
    return Line(a, b, color=color, stroke_width=width)


def packet(lane, point, scale=1.0):
    """A packet is two concentric leaves, NOT a semantic Group/VGroup.

    Animating 32 VGroups together enters Noon's family-Transform path, which
    rejects multiple family Transforms in one activation in the reported build.
    Keep the rectangle and its label as ordinary leaves. Both use the same
    center, destination, scale, and play interval, so they still move together.
    """
    x, y, _ = point
    parts = (
        rect(x, y, QUEUE_W, QUEUE_H, ROW_COLORS[lane // 8], 1, 0),
        label(lane, x, y, 12, BG, width=QUEUE_W - 0.06),
    )
    if scale != 1.0:
        for part in parts:
            part.scale(scale)
    return parts


def packet_leaves(requests):
    """Flatten only the authored packet tuples for Scene.add and FadeOut."""
    return [part for parts in requests for part in parts]


def packet_motion(parts, destination, scale=1.0):
    """Build two leaf animations; the caller plays ALL packets in parallel.

    These packet parts are concentric, so scaling each part about its own
    destination center preserves the packet's shared pivot. No callback,
    family Transform, extra play call, or serialized packet motion is needed.
    """
    animations = []
    for part in parts:
        animation = part.animate.move_to(destination)
        if scale != 1.0:
            animation = animation.scale(scale)
        animations.append(animation)
    return animations


class CudaTransposeBanks(Scene):
    """One retained scene, ordinary Noon animations, no frame callbacks."""

    async def play_for(self, *animations, seconds=0.5, proof=None):
        # Optional test observer only: it cannot schedule or interpolate the scene.
        # Ordinary gallery playback does not install a probe.
        probe = getattr(self, "_cuda_probe", None)
        if probe is not None:
            probe.before(self, seconds / SPEED, proof)
        await self.play(*animations, run_time=seconds / SPEED)
        if probe is not None:
            probe.after(self, proof)

    async def pause(self, seconds=0.7):
        probe = getattr(self, "_cuda_probe", None)
        if probe is not None:
            probe.before(self, seconds / SPEED, None)
        await self.wait(seconds / SPEED)
        if probe is not None:
            probe.after(self, None)

    def observe(self, event, **values):
        probe = getattr(self, "_cuda_probe", None)
        if probe is not None:
            probe.checkpoint(self, event, **values)

    async def fade(self, objects, seconds=0.30):
        if objects:
            await self.play_for(*(FadeOut(o) for o in objects), seconds=seconds)

    async def narrate(self, title, detail="", color=INK):
        new = [label(title, y=-3.01, size=23, color=color),
               label(detail or " ", y=-3.44, size=15, color=MUTED)]
        await self.play_for(*(FadeOut(o) for o in self.captions),
                      *(FadeIn(o) for o in new), seconds=0.22)
        self.captions = new

    async def status(self, message, color=MUTED):
        new = label(message, -3.53, -2.35, 14, color, width=5.8)
        await self.play_for(FadeOut(self.status_text), FadeIn(new), seconds=0.14)
        self.status_text = new

    async def phase(self, name, stride):
        new = label("%s  /  stride %d" % (name, stride),
                    -3.50, 2.55, 16, CYAN, width=6.0)
        await self.play_for(FadeOut(self.phase_text), FadeIn(new), seconds=0.18)
        self.phase_text = new

    async def show_code(self, page, stride):
        """Common font scale, explicit indentation, and one Text per code line."""
        await self.fade(self.code_objects, 0.25)
        entries = code_entries(page, stride)
        size = 16
        # Measuring two equal glyph strings yields monospace *advance*, not ink width.
        mm = label("MM", size=size)
        m = label("M", size=size)
        advance = mm.width - m.width
        prepared = []
        for key, source in entries:
            body = source.lstrip()
            indent = len(source) - len(body)
            obj = Text(body, font="DejaVu Sans Mono", font_size=size,
                       color=INK, t2c=ink_for_code(body) if SYNTAX_HIGHLIGHT else None)
            prepared.append((key, obj, indent))
        max_w = max(o.width + indent * advance for _, o, indent in prepared)
        if page == "load":
            # Reserve the longer padded declaration BEFORE the edit: no font-size jump.
            padded = Text(code_entries("load", 33)[0][1], font="DejaVu Sans Mono",
                          font_size=size, color=INK)
            max_w = max(max_w, padded.width)
        max_h = max(o.height for _, o, _ in prepared)
        factor = min(1.0, 5.70 / max_w, 0.245 / max_h)
        self.highlights, self.code_objects, self.code_text = {}, [], {}
        self.code_scale = factor
        for n, (key, obj, indent) in enumerate(prepared):
            y = CODE_TOP - n * CODE_STEP
            obj.scale(factor)
            # Leading whitespace never depends on the engine's ink bounds.
            obj.move_to((-6.28 + indent * advance * factor + obj.width / 2, y, 0))
            gutter = label("%02d" % (n + 1), -6.60, y, 11, MUTED)
            # Animate fill alpha explicitly; set_opacity(1) can overwrite a
            # VMobject's faint fill/stroke alpha instead of just revealing it.
            highlight = rect(-3.48, y, 6.14, CODE_LINE_H,
                             CODE_HIGHLIGHT_COLOR, 0.0,
                             CODE_HIGHLIGHT_STROKE)
            self.highlights[key] = highlight
            self.code_text[key] = obj
            self.code_objects.extend((highlight, gutter, obj))
        self.active_keys = ()
        # Add transparent markers first; only glyphs/gutter fade in.
        self.add(*(self.code_objects[i] for i in range(0, len(self.code_objects), 3)))
        await self.play_for(*(FadeIn(o) for i, o in enumerate(self.code_objects) if i % 3),
                      seconds=0.40)

    async def focus(self, step):
        keys = SOURCE_STEPS[step]
        if any(key not in self.highlights for key in keys):
            raise ValueError("source step %r is missing from the displayed CUDA page" % step)
        if keys != self.active_keys:
            await self.play_for(
                *(self.highlights[k].animate.set_fill(opacity=0.0) for k in self.active_keys),
                *(self.highlights[k].animate.set_fill(opacity=CODE_HIGHLIGHT_FILL) for k in keys),
                seconds=0.16,
            )
        self.active_keys = keys

    async def move_step(self, step, *animations, seconds=0.5, proof=None):
        # The source focus is established BEFORE the corresponding data movement.
        await self.focus(step)
        await self.play_for(*animations, seconds=seconds, proof=proof)

    def tile_grid(self, title):
        x, top, size = TILE_LEFT, TILE_TOP, 32 * CELL
        objects = [label(title, 3.45, 2.54, 16, INK, width=5.8)]
        objects += [line((x + i * CELL, top, 0), (x + i * CELL, top - size, 0))
                    for i in range(33)]
        objects += [line((x, top - i * CELL, 0), (x + size, top - i * CELL, 0))
                    for i in range(33)]
        for i in (0, 8, 16, 24, 31):
            objects.append(label(i, x - 0.26, top - (i + 0.5) * CELL, 11, MUTED))
            objects.append(label(i, x + (i + 0.5) * CELL, top + 0.16, 11, MUTED))
        # A small key remains meaningful after transposition: color follows source rows.
        for group, color in enumerate(ROW_COLORS):
            x = 1.14 + group * 1.45
            objects += [rect(x, -2.39, 0.12, 0.12, color, 1, 0),
                        label("%d-%d" % (group * 8, group * 8 + 7),
                              x + 0.45, -2.39, 11, MUTED)]
        return objects

    async def construct(self):
        if isinstance(SPEED, bool) or not isinstance(SPEED, (int, float)) or not 0 < SPEED < 100:
            raise ValueError("SPEED must be a finite number between 0 and 100.")
        if type(COLUMN) is not int or not 0 <= COLUMN < 32:
            raise ValueError("COLUMN must be an integer from 0 to 31.")
        traces = [get_trace(s) for s in (32, 33)]
        self.captions, self.code_objects, self.highlights, self.active_keys = [], [], {}, ()
        self.add(rect(0, 0, 14.23, 8.01, BG, 1, 0))
        self.panels = [rect(-3.51, 0.10, 6.65, 5.46, PANEL, 1, 0),
                       rect(3.43, 0.10, 6.65, 5.46, PANEL, 1, 0)]
        self.add(*self.panels)
        title = label("ONE EXTRA COLUMN", y=3.57, size=30)
        provenance = ("GPU-recorded selected read addresses + values" if GPU_TRACE is not None
                      else "CPU reference replay / CUDA is not running")
        meta = label(provenance + "  |  block 32 x 8  |  column %d" % COLUMN,
                     y=3.14, size=12, color=MUTED)
        foot = label("CUDA Algorithm Lab  |  service rounds are schematic, not GPU cycles or a benchmark",
                     y=-3.84, size=10, color=MUTED)
        self.phase_text = label("CUDA / one interior 32 x 32 tile", -3.5, 2.55, 15, CYAN)
        self.status_text = label("TILE_DIM = 32    BLOCK_ROWS = 8", -3.53, -2.35, 14, MUTED)
        await self.play_for(Write(title), FadeIn(meta), FadeIn(foot),
                      FadeIn(self.phase_text), FadeIn(self.status_text), seconds=0.85)
        await self.narrate("Transpose the data. Then fix the memory layout.",
                     "One real CUDA statement stays connected to every visible transfer.")
        await self.pause(1.0)
        for trace in traces:
            await self.load_tile(trace)
        self.observe("complete")
        await self.finish(traces)

    async def load_tile(self, trace):
        self.observe("launch", trace=trace)
        stride = trace["stride"]
        await self.phase("EDIT LAYOUT" if stride == 33 else "01  LOAD ROWS", 32)
        await self.status("New launch  |  0 / 1,024 words ready")
        await self.show_code("load", 32)  # Deliberately show the one-line edit on the second run.
        visual = self.tile_grid("GLOBAL INPUT  ->  SHARED TILE")
        source = [rect(tile_point(0, lane)[0], 2.08, 0.086, 0.082, CYAN, 1, 0)
                  for lane in range(32)]
        source_label = label("one input row / 32 adjacent words", 3.45, 2.29, 12, MUTED)
        visual += source + [source_label]
        await self.play_for(*(FadeIn(o) for o in visual), seconds=0.45)
        await self.focus("allocate")
        if stride == 33:
            await self.narrate("Add one unused float to every row.",
                         "This is a recompile and rerun, not a live GPU array resize.", GREEN)
            # Only the changed declaration is replaced; the rest of the page stays put.
            old = self.code_text["declaration"]
            old_index = self.code_objects.index(old)
            body = code_entries("load", 33)[0][1]
            new = Text(body, font="DejaVu Sans Mono", font_size=16, color=INK,
                       t2c=ink_for_code(body) if SYNTAX_HIGHLIGHT else None)
            # Keep exactly the original page font scale, reserved for this longer line.
            new.scale(self.code_scale)
            new.move_to((-6.28 + new.width / 2, CODE_TOP, 0))
            pad = rect(TILE_LEFT + 32.5 * CELL, TILE_TOP - 16 * CELL,
                       CELL, 32 * CELL, GREEN, 0.30, 1)
            pad_label = label("+1", TILE_LEFT + 32 * CELL + 0.53, 0.15, 15, GREEN)
            await self.play_for(FadeOut(old), Write(new), FadeIn(pad), FadeIn(pad_label), seconds=0.75)
            self.code_objects[old_index] = new
            self.code_text["declaration"] = new
            visual += [pad, pad_label]
            await self.phase("01  LOAD ROWS", stride)
            await self.pause(1.6)
        await self.focus("coordinates")
        await self.status("block=(0,0)  x=lane  y=threadIdx.y")
        await self.narrate("Load adjacent words into a shared-memory row.",
                     "Color follows the input row group, not the writer's thread identity.")
        await self.pause(0.8 if stride == 32 else 0.4)
        bands = []
        for k, j in enumerate((0, 8, 16, 24)):
            await self.focus("loop")
            await self.status("j=%02d  |  rows %02d..%02d  |  %d words ready" %
                        (j, j, j + 7, sum(trace["loads"][:k])))
            await self.focus("bounds")
            row = j + COLUMN % 8
            color = ROW_COLORS[k]
            packets = [rect(tile_point(row, lane)[0], 2.08, 0.086, 0.082, color, 1, 0)
                       for lane in range(32)]
            band = rect(TILE_LEFT + 16 * CELL, TILE_TOP - (j + 4) * CELL,
                        32 * CELL, 8 * CELL, color, 0.23, 0)
            self.add(*packets)
            # Both halves of the assignment remain lit throughout the transfer.
            await self.move_step("copy", *(p.animate.move_to(tile_point(row, lane))
                                     for lane, p in enumerate(packets)),
                           FadeIn(band), seconds=0.95 if stride == 32 and k == 0 else 0.42,
                           proof={"kind": "load", "trace": trace, "j": j,
                                  "packets": [(p,) for p in packets], "band": band})
            self.remove(*packets)
            bands.append(band)
            await self.status("j=%02d complete  |  %d / 1,024 words ready" %
                        (j, sum(trace["loads"][:k + 1])), color)
            self.observe("load_ready", trace=trace, j=j)
            if k == 0 and stride == 32:
                await self.pause(1.1)
        await self.status("j loop complete  |  1,024 words ready", GREEN)
        await self.phase("02  BLOCK BARRIER", stride)
        await self.focus("barrier")
        await self.narrate("Wait for every warp before reading the tile.",
                     "__syncthreads() is a block barrier; warp order here is illustrative.", PINK)
        arrivals = [rect(1.53 + w * 0.52, -2.19, 0.39, 0.21, PINK, 0.30, 1)
                    for w in range(8)]
        await self.play_for(*(FadeIn(o) for o in arrivals), seconds=0.30)
        await self.play_for(*(o.animate.set_fill(PINK, opacity=0.85) for o in arrivals), seconds=0.65)
        await self.pause(1.8 if stride == 32 else 1.0)
        self.observe("barrier", trace=trace, arrivals=arrivals)
        await self.fade(arrivals, 0.18)
        await self.show_code("output", stride)
        await self.phase("03  READ A COLUMN", stride)
        await self.focus("output_coordinates")
        await self.status("selected warp ty=%d  |  selected j=%d" % (COLUMN % 8, COLUMN // 8 * 8))
        await self.narrate("The output swaps block coordinates.",
                     "This fixture uses block (0,0); follow one column of the completed tile.")
        await self.pause(1.0)
        await self.focus("output_bounds")
        selection = [rect(*tile_point(lane, COLUMN)[:2], 0.097, 0.097,
                          ROW_COLORS[lane // 8], 0.9, 0) for lane in range(32)]
        await self.move_step("read", *(FadeIn(o) for o in selection), seconds=0.45)
        await self.narrate("Lane r reads tile[r][%d]." % COLUMN,
                     "The same value and row-group color follow the request into its bank.")
        await self.pause(1.0)
        # Retain the exact read/store statement, but clear the long page before
        # packets cross to the request tray. This avoids paths crossing source text.
        await self.show_code("read", stride)
        await self.focus("read")
        requests = [packet(lane, tile_point(lane, COLUMN), scale=0.20)
                    for lane in range(32)]
        self.add(*packet_leaves(requests))
        # The selected data keeps its identity while the grid opens into a request tray.
        await self.move_step("read", *(FadeOut(o) for o in visual + bands + selection),
                       *(animation for lane, parts in enumerate(requests)
                         for animation in packet_motion(parts, queue_point(lane), scale=5.0)),
                       seconds=0.80,
                       proof={"kind": "tray", "trace": trace, "packets": requests})
        await self.bank_replay(trace, requests)

    async def bank_replay(self, trace, requests):
        stride, reads = trace["stride"], trace["reads"]
        batches = service_batches(reads)
        await self.focus("read")
        facts = [
            label("word = %d + lane * %d + %d" % (trace["base_word"], stride, COLUMN), -3.46, 0.65, 16, AMBER, width=5.9),
            label("bank = word % 32", -3.46, 0.24, 16, MUTED),
            label("REQUEST TRAY  /  packet number = lane", -3.46, -0.29, 12, MUTED),
        ]
        bank_objects = [label("32 SHARED-MEMORY BANKS", 3.44, 2.54, 16)]
        bank_boxes = []
        for bank in range(32):
            x, y, _ = bank_center(bank)
            card = rect(x, y, 0.65, 0.54, EDGE, 0.15, 1)
            bank_boxes.append(card)
            bank_objects += [card, label("B%02d" % bank, x, y + 0.15, 11, MUTED)]
        register_objects = [label("LANE REGISTERS  /  returned values", 3.44, -0.83, 12, MUTED)]
        for lane in range(32):
            x, y, _ = register_point(lane)
            register_objects.append(rect(x, y + 0.045, REGISTER_CELL_W,
                                         REGISTER_CELL_H, EDGE, 0.12, 1))
            register_objects.append(label(lane, x, y + 0.17, 9, MUTED))
        await self.play_for(*(FadeIn(o) for o in facts + bank_objects + register_objects), seconds=0.45)
        self.observe("banks", trace=trace, banks=bank_objects, registers=register_objects)
        used = sorted({event[2] % 32 for event in reads})
        color = GREEN if len(batches) == 1 else CORAL
        await self.play_for(*(bank_boxes[b].animate.set_stroke(color, width=2) for b in used), seconds=0.3)
        await self.status("%d banks used  |  %d read rounds required" % (len(used), len(batches)), color)
        if stride == 32:
            await self.narrate("32 distinct words request bank %d." % used[0],
                         "These addresses differ, so the bank cannot broadcast one value.", CORAL)
        else:
            await self.narrate("The same 32 values now reach 32 different banks.",
                         "Only the shared row stride changed: 32 -> 33.", GREEN)
        # Trace-derived examples, including a nonzero physical base from a GPU capture.
        example = label("lane 0: word %d -> B%02d" % (reads[0][2], reads[0][2] % 32),
                        -3.45, 1.01, 14, ROW_COLORS[0], width=5.9)
        await self.play_for(FadeIn(example), seconds=0.18)
        await self.pause(2.0)
        # Move the mapping explanation to the caption band while requests travel.
        # The packet paths cannot obscure the formula or the CUDA statement.
        await self.fade(facts + [example], 0.25)
        mapping = "bank = (%d + lane * %d + %d) %% 32" % (trace["base_word"], stride, COLUMN)
        await self.narrate("One round." if len(batches) == 1 else "One bank. 32 serialized read rounds.",
                     mapping + "  /  color follows the value", color)
        for batch in batches:
            await self.move_step("read",
                           *(animation for lane in batch
                             for animation in packet_motion(
                                 requests[lane], bank_point(reads[lane][2] % 32))),
                           seconds=ROUND_IN,
                           proof={"kind": "bank", "trace": trace,
                                  "batch": batch, "packets": requests})
            await self.move_step("read",
                           *(animation for lane in batch
                             for animation in packet_motion(
                                 requests[lane], register_point(lane), scale=REGISTER_SCALE)),
                           seconds=ROUND_OUT,
                           proof={"kind": "register", "trace": trace,
                                  "batch": batch, "packets": requests})
        await self.status("%d / %d read rounds complete" % (len(batches), len(batches)), color)
        await self.pause(0.7)
        await self.phase("04  STORE A ROW", stride)
        await self.narrate("The shared load completes. Now the warp stores.",
                     "All 32 lanes write consecutive global-memory words.", GREEN)
        await self.focus("store")
        await self.fade(bank_objects + register_objects, 0.35)
        output = self.tile_grid("GLOBAL OUTPUT  /  TRANSPOSED")
        band = rect(TILE_LEFT + 16 * CELL, output_point(0, COLUMN)[1],
                    32 * CELL, CELL, GREEN, 0.33, 1)
        await self.play_for(*(FadeIn(o) for o in output), FadeIn(band), seconds=0.4)
        # Same packet objects, same values: column r becomes output row COLUMN.
        to_tile_scale = 0.20 / REGISTER_SCALE
        await self.move_step("store",
                       *(animation for lane, parts in enumerate(requests)
                         for animation in packet_motion(
                             parts, output_point(lane, COLUMN), scale=to_tile_scale)),
                       seconds=0.95,
                       proof={"kind": "store", "trace": trace, "packets": requests})
        values = ", ".join(str(int(r[4])) for r in reads[:8])
        result = [label("out[%d, 0:8]" % COLUMN, -3.45, 0.75, 18, GREEN),
                  label(values, -3.45, 0.27, 17, INK, width=5.85),
                  label("global word indices: %d..%d" % (reads[0][3], reads[-1][3]),
                        -3.45, -0.27, 14, MUTED),
                  label("%d read round%s" % (len(batches), "" if len(batches) == 1 else "s"),
                        -3.45, -1.12, 29, color),
                  label("Full fixture: 1,024 / 1,024 values match", -3.45, -1.77, 13, MUTED)]
        await self.play_for(*(FadeIn(o) for o in result), seconds=0.5)
        self.observe("output_row", trace=trace, packets=requests, labels=result)
        await self.status("Shown: one output row. Validation covers the whole tile.", GREEN)
        await self.pause(2.2)
        await self.fade(output + [band] + result + packet_leaves(requests) + self.code_objects, 0.4)
        self.code_objects, self.highlights, self.active_keys = [], {}, ()

    async def finish(self, traces):
        await self.fade(self.panels + [self.phase_text, self.status_text], 0.4)
        await self.narrate("Same answer. Fewer shared-memory conflicts.",
                     "Bank-conflict reduction is not a measured whole-kernel speedup.", GREEN)
        objects = []
        for x, trace, accent, heading in ((-3.33, traces[0], CORAL, "UNPADDED"),
                                          (3.33, traces[1], GREEN, "ONE EXTRA COLUMN")):
            stride = trace["stride"]
            rounds = len(service_batches(trace["reads"]))
            objects += [rect(x, 0.55, 5.95, 4.28, PANEL, 1, 0),
                        rect(x, 2.69, 5.95, 0.04, accent, 1, 0),
                        label(heading, x, 2.14, 18, accent),
                        label("tile[32][%d]" % stride, x, 1.52, 24),
                        label(rounds, x, 0.42, 66, accent),
                        label("schematic read round%s" % ("" if rounds == 1 else "s"), x, -0.34, 16, MUTED),
                        label("%s bytes / tile" % format(32 * stride * 4, ","), x, -0.98, 19)]
        cost = label("+128 bytes  /  +3.125% shared memory", y=-2.06, size=22, color=GREEN)
        await self.play_for(*(FadeIn(o) for o in objects), seconds=0.75)
        await self.play_for(Write(cost), seconds=0.7)
        await self.pause(3.5)
