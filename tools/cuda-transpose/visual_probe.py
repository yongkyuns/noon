"""Read-only checkpoint observer for the REAL Noon scene (not a runtime mock).

Loaded by scripts/cuda-transpose-qualification.mjs in the Pyodide worker.
Expected addresses and positions below are independent acceptance contracts:
this file never calls the demo's reference_trace, coordinate helpers, focus,
packet_motion, or service_batches. Noon still executes every play/wait.
"""
import json
import math


class FidelityError(AssertionError):
    pass


def require(ok, code, message):
    if not ok:
        raise FidelityError("CUDA_FIDELITY[%s]: %s" % (code, message))


def close(actual, expected, code="position", tolerance=0.0002):
    require(len(actual) == len(expected) and all(
        math.isfinite(float(a)) and abs(float(a) - b) <= tolerance
        for a, b in zip(actual, expected)), code, "%r != %r" % (actual, expected))


def expected_reads(stride, column, base):
    # Closed-form oracle, deliberately not a copy of the kernel's two loops.
    return [[r, column, base + r * stride + column, 32 * column + r, 32 * r + column]
            for r in range(32)]


def validate_trace(trace, column):
    require(isinstance(trace, dict), "trace", "trace must be an object")
    stride, base = trace.get("stride"), trace.get("base_word")
    require(type(stride) is int and stride in (32, 33) and
            type(base) is int and base >= 0, "trace", "unsupported address layout")
    require(trace.get("reads") == expected_reads(stride, column, base),
            "trace", "addresses, lane/value association, or output indices differ")
    require(trace.get("loads") == [256] * 4 and trace.get("verified_output_words") == 1024,
            "trace", "fixture is incomplete")


def position(stage, lane, column, stride, base=0, j=0):
    # Screen contract is independent of the scene's position helpers.
    if stage == "input":
        return [1.55 + (lane + 0.5) * 0.11, 2.08]
    if stage == "loaded":
        return [1.55 + (lane + 0.5) * 0.11, 1.62 - (j + column % 8 + 0.5) * 0.11]
    if stage == "column":
        return [1.55 + (column + 0.5) * 0.11, 1.62 - (lane + 0.5) * 0.11]
    if stage == "tray":
        return [-6.05 + lane % 8 * 0.71, -0.79 - lane // 8 * 0.37]
    if stage == "bank":
        bank = (base + lane * stride + column) % 32
        return [0.95 + bank % 8 * 0.75, 1.76 - bank // 8 * 0.64 - 0.095]
    if stage == "register":
        return [0.66 + lane % 16 * 0.375, -1.53 - lane // 16 * 0.57]
    if stage == "store":
        return [1.55 + (lane + 0.5) * 0.11, 1.62 - (column + 0.5) * 0.11]
    raise FidelityError("unknown expected packet stage " + str(stage))


RGB = ((103, 218, 238), (242, 194, 123), (186, 171, 255), (245, 156, 197))
SCALES = {"column": 0.20, "tray": 1.0, "bank": 1.0, "register": 0.58, "store": 0.20}
SOURCE_TEXT = {
    "load_dst": "tile[threadIdx.y + j][threadIdx.x] =",
    "load_src": "in[static_cast<size_t>(y + j) * cols + x];",
    "read_src": "tile[threadIdx.x][threadIdx.y + j];",
    "store_dst": "out[static_cast<size_t>(y + j) * rows + x] =",
    "sync": "__syncthreads();",
}


def observe_leaf(obj, with_fill=True):
    center = obj.get_center()
    out = {"id": int(obj.id), "center": [float(center[0]), float(center[1])],
           "width": float(obj.width), "height": float(obj.height)}
    if with_fill:
        c = obj.get_color()
        out.update(alpha=float(obj.get_fill_opacity()), rgb=[float(c.red), float(c.green), float(c.blue)])
    else:
        out["text"] = obj.text
    return out


def observe_packets(packets):
    return [{"lane": lane, "body": observe_leaf(parts[0]),
             "label": observe_leaf(parts[1], False) if len(parts) == 2 else None}
            for lane, parts in enumerate(packets)]


def observe_highlights(scene):
    gutter_right = float(scene.code.line_numbers.get_right()[0])
    source_left = float(scene.code.code_lines.get_left()[0])
    gap = source_left - gutter_right
    require(gap > 0.03, "gutter", "source overlaps its line-number gutter")
    result = {}
    for key, obj in scene.highlights.items():
        h = observe_leaf(obj)
        h["stroke_width"] = float(obj.get_stroke_width())
        text = observe_leaf(scene.code_text[key], False)
        # Sample the measured gutter/source gap, never an assumed code x origin.
        # The reference is in the existing scene panel just outside this row band.
        h.update(text=text["text"], text_id=text["id"], text_center=text["center"],
                 text_width=text["width"], text_height=text["height"],
                 sample_region={"center": [(gutter_right + source_left) / 2, h["center"][1]],
                                "width": min(0.08, gap / 2), "height": min(0.10, h["height"] / 2)},
                 reference_region={"center": [h["center"][0] - h["width"] / 2 - 0.04,
                                               h["center"][1]], "width": 0.035,
                                   "height": min(0.10, h["height"] / 2)})
        result[key] = h
    return result


def bounds(info):
    x, y = info["center"]
    w, h = info["width"], info["height"]
    require(all(math.isfinite(v) for v in (x, y, w, h)) and w > 0 and h > 0,
            "source-layout", "invalid allocated row or ink bounds")
    return [x - w / 2, y - h / 2, x + w / 2, y + h / 2]


def contains(outer, inner, code="highlight-position", tolerance=0.002):
    a, b = bounds(outer), bounds(inner)
    require(b[0] >= a[0] - tolerance and b[1] >= a[1] - tolerance and
            b[2] <= a[2] + tolerance and b[3] <= a[3] + tolerance,
            code, "source ink leaves its allocated row")


def validate_highlights(highlights, keys):
    active = {key for key, info in highlights.items() if info["alpha"] > 0.0001}
    require(active == set(keys), "highlight", "active lines %r, expected %r" % (active, keys))
    for key in keys:
        h = highlights[key]
        # A Code row is a logical allocation. Its center need not equal the
        # center of glyph ink (different statements have different ascenders).
        contains(h, {"center": h["text_center"], "width": h["text_width"],
                     "height": h["text_height"]})
        require(h["text"].strip() == SOURCE_TEXT[key],
                "source", "displayed CUDA statement does not match " + key)


def observe_source_page(scene):
    rows = {key: observe_leaf(obj, False) for key, obj in scene.code_text.items()}
    numbers = [observe_leaf(obj, False) for obj in scene.code.line_numbers]
    return {"page": "load" if "declaration" in rows else "output" if "output_x" in rows else "read",
            "rows": rows, "numbers": numbers, "regions": observe_highlights(scene),
            "panel": observe_leaf(scene.code.background[0]),
            "source": scene.code.code_string,
            "source_font_size": float(scene.code.code_lines[0].font_size),
            "gutter_font_size": float(scene.code.line_numbers[0].font_size)}


def validate_compact_clearance(regions):
    # Test the entire geometric tray->bank corridor for every column/layout,
    # against the ACTUAL allocated compact page, not a recreated line grid.
    boxes = [bounds(region) for region in regions.values()]
    left, bottom, right = min(b[0] for b in boxes), min(b[1] for b in boxes), max(b[2] for b in boxes)
    for stride in (32, 33):
        for column in range(32):
            for lane in range(32):
                a = position("tray", lane, column, stride)
                b = position("bank", lane, column, stride)
                lo = max(0, (left - 0.46 / 2 - a[0]) / (b[0] - a[0]))
                hi = min(1, (right + 0.46 / 2 - a[0]) / (b[0] - a[0]))
                if lo <= hi:
                    highest = max(a[1] + t * (b[1] - a[1]) for t in (lo, hi)) + 0.24 / 2
                    require(highest <= bottom - 0.01, "packet-clearance",
                            "compact CUDA page crosses the packet corridor")


def validate_source_page(page):
    rows, numbers, regions = page["rows"], page["numbers"], page["regions"]
    require(len(rows) == {"load": 10, "output": 8, "read": 2}[page["page"]] and
            len(numbers) == len(rows) and set(regions) == set(rows),
            "source-layout", "wrong source page, row or gutter coverage")
    require(page["source"] == "\n".join(row["text"] for row in rows.values()),
            "source", "source aliases differ from native Code rows")
    require(page["source_font_size"] == 16 and page["gutter_font_size"] == 11,
            "gutter", "smaller source-number typography was lost")
    previous_bottom = None
    for index, ((key, row), number) in enumerate(zip(rows.items(), numbers)):
        require(number["text"] == "%02d" % (index + 1), "gutter", "line numbers are not zero padded")
        require(row["id"] == regions[key]["text_id"], "identity", "row alias does not match its region")
        require(abs(regions[key]["stroke_width"]) <= 1e-9, "highlight", "source emphasis gained an outline")
        contains(regions[key], row)
        contains(regions[key], number, "gutter")
        if key in SOURCE_TEXT:
            require(row["text"].strip() == SOURCE_TEXT[key], "source", "wrong CUDA statement")
        box = bounds(regions[key])
        require(box[0] >= -6.835 and box[2] <= -0.185 and box[1] >= -2.15 and box[3] <= 2.38,
                "source-layout", "source page leaves its pane or overlaps status/header")
        if previous_bottom is not None:
            require(box[3] <= previous_bottom + 0.002, "source-layout", "allocated source rows overlap")
        previous_bottom = box[1]
    require(max(n["height"] for n in numbers) < max(row["height"] for row in rows.values()),
            "gutter", "gutter glyphs are no longer smaller than source glyphs")
    if page["page"] == "read":
        validate_compact_clearance(regions)


def validate_source_edit(previous, current):
    require(previous["page"] == current["page"] == "load", "source-edit", "unexpected page edit")
    require(previous["source"].count("[TILE_DIM][TILE_DIM];") == 1 and
            current["source"] == previous["source"].replace("[TILE_DIM][TILE_DIM];", "[TILE_DIM][TILE_DIM + 1];"),
            "source-edit", "edit changes more than the padded declaration")
    require(previous["numbers"] == current["numbers"] and previous["panel"] == current["panel"],
            "source-edit", "edit moved or replaced the reserved panel/gutter")
    for key, before in previous["rows"].items():
        after = current["rows"][key]
        for field in ("id", "center", "width", "height"):
            require(previous["regions"][key][field] == current["regions"][key][field],
                    "source-edit", "edit changed the allocated row " + key)
        if key == "declaration":
            require(before["id"] != after["id"], "source-edit", "edited row did not receive a fresh identity")
            contains(previous["regions"][key], after, "source-edit")
        else:
            require(before == after, "source-edit", "edit changed an unaffected source row " + key)


def expected_packet(lane, stage, column, stride, base, j=0):
    small = stage in ("input", "loaded")
    return {"lane": lane, "center": position(stage, lane, column, stride, base, j),
            "width": 0.086 if small else 0.46 * SCALES[stage],
            "height": 0.082 if small else 0.24 * SCALES[stage],
            "rgb": list(RGB[j // 8 if small else lane // 8]),
            "has_label": not small}


def validate_packets(observed, expected, identities=None):
    require(len(observed) == len(expected) == 32, "coverage", "expected exactly 32 lanes")
    ids = []
    for got, want in zip(observed, expected):
        lane, body = want["lane"], got["body"]
        require(got["lane"] == lane, "identity", "lane order changed")
        close(body["center"], want["center"])
        close([body["width"], body["height"]], [want["width"], want["height"]], "size")
        close(body["rgb"], [v / 255 for v in want["rgb"]], "color", 0.004)
        require(body["alpha"] >= 0.95, "visibility", "packet %d has no opaque fill" % lane)
        ids.append(body["id"])
        if want["has_label"]:
            label = got["label"]
            require(label is not None and label["text"] == str(lane), "label", "wrong lane text")
            close(label["center"], body["center"], "label")
            require(label["width"] <= body["width"] and label["height"] <= body["height"] + 0.001,
                    "label", "packet label exceeds its body")
            ids.append(label["id"])
    require(len(ids) == len(set(ids)), "identity", "duplicated visible packet objects")
    if identities is not None:
        require(ids == identities, "identity", "packets or labels replaced during transfer")
    return ids


class CudaVisualProbe:
    def __init__(self, config, emit):
        self.column = config["column"]
        self.speed = config["speed"]
        self.emit = emit
        self.sequence = 0
        self.current = None
        self.runs = []
        self.completed = []
        self.checks = 0
        self.identities = None
        self.page_identity = None
        self.page_snapshot = None
        self.pending_source_capture = None
        self.page_counts = {"load": 0, "output": 0, "read": 0}
        self.source_edits = 0

    def observe_code(self, scene):
        if not getattr(scene, "code_objects", None):
            return
        identity = id(scene.code)
        if identity == self.page_identity and scene.code.code_string == self.page_snapshot["source"]:
            return
        page = observe_source_page(scene)
        validate_source_page(page)
        if identity != self.page_identity:
            self.page_counts[page["page"]] += 1
            self.pending_source_capture = "source-page-" + page["page"]
        elif page["source"] != self.page_snapshot["source"]:
            validate_source_edit(self.page_snapshot, page)
            self.source_edits += 1
            self.pending_source_capture = "source-edit"
        self.page_identity, self.page_snapshot = identity, page

    def checkpoint(self, scene, event, **values):
        if event == "launch":
            trace = values["trace"]
            validate_trace(trace, self.column)
            require(trace["stride"] not in self.runs, "coverage", "duplicate launch")
            self.runs.append(trace["stride"])
            self.trace = trace
            self.ready = set()
            self.loaded = set()
            self.barrier = False
            self.stages = ["column"] * 32
            self.in_bank = set()
            self.returned = set()
            self.identities = None
        elif event == "load_ready":
            j = values["j"]
            require(j in self.loaded and j not in self.ready, "ordering", "count precedes its load")
            self.ready.add(j)
            message = scene.status_text.text.replace(",", "")
            require("%d / 1024 words ready" % (len(self.ready) * 256) in message,
                    "counter", "incorrect completed-load count: " + message)
        elif event == "barrier":
            require(self.ready == {0, 8, 16, 24}, "barrier", "read barrier before the full tile is ready")
            validate_highlights(observe_highlights(scene), ["sync"])
            require(len(values["arrivals"]) == 8, "barrier", "not all warp indicators are present")
            for w, obj in enumerate(values["arrivals"]):
                close(list(obj.get_center()[:2]), [1.53+w*0.52, -2.19], "barrier")
                require(obj.get_fill_opacity() >= 0.8, "barrier", "warp indicator did not arrive")
            self.barrier = True
        elif event == "banks":
            banks, registers = values["banks"], values["registers"]
            require(len(banks) == len(registers) == 65, "coverage", "32 bank and register cards required")
            for i in range(32):
                bank = banks[2*i+1]
                caption = banks[2*i+2]
                xy = [0.95+i%8*0.75, 1.76-i//8*0.64]
                close(list(bank.get_center()[:2]), xy, "bank-label")
                require(caption.text == "B%02d" % i, "bank-label", "bank card has the wrong label")
                close(list(caption.get_center()[:2]), [xy[0],xy[1]+0.15], "bank-label")
                require(registers[2*i+2].text == str(i), "register-label", "register lane label is wrong")
                xy = position("register", i, self.column, self.trace["stride"])
                close(list(registers[2*i+1].get_center()[:2]), [xy[0],xy[1]+0.045], "register-label")
                close(list(registers[2*i+2].get_center()[:2]), [xy[0],xy[1]+0.17], "register-label")
        elif event == "output_row":
            require(self.returned == set(range(32)) and all(s == "store" for s in self.stages),
                    "ordering", "output before all selected reads/stores")
            labels = values["labels"]
            require(labels[0].text == "out[%d, 0:8]" % self.column, "value", "wrong output row")
            want = ", ".join(str(32 * r + self.column) for r in range(8))
            require(labels[1].text == want, "value", "displayed output values differ")
            require(labels[2].text == "global word indices: %d..%d" %
                    (self.column * 32, self.column * 32 + 31), "value", "wrong output addresses")
            self.completed.append(self.trace["stride"])
        elif event == "complete":
            require(self.runs == [32, 33] and self.completed == [32, 33],
                    "coverage", "both layouts must complete")
        else:
            raise FidelityError("unknown checkpoint " + event)
        self.checks += 1
        receipt = {"type": "checkpoint", "name": event, "time": float(scene.time),
                   "stride": getattr(self, "trace", {}).get("stride")}
        if event == "barrier":
            receipt["highlights"] = observe_highlights(scene)
        if event == "banks":
            receipt["text_regions"] = [observe_leaf(obj, False) for obj in values["banks"][2::2]]
        self.emit(receipt)

    def before(self, scene, duration, proof):
        require(self.current is None, "ordering", "nested source interval")
        require(math.isfinite(duration) and duration > 0, "time", "invalid authored duration")
        self.sequence += 1
        event = {"type": "interval", "id": self.sequence, "start": float(scene.time),
                 "end": float(scene.time) + duration, "kind": None}
        if proof is not None:
            kind, trace = proof["kind"], proof["trace"]
            require(trace is self.trace, "trace", "motion is attached to the wrong launch")
            validate_trace(trace, self.column)
            stride, base = trace["stride"], trace["base_word"]
            j = proof.get("j", 0)
            batch = list(proof.get("batch", range(32)))
            require(len(batch) == len(set(batch)) and all(type(x) is int and 0 <= x < 32 for x in batch),
                    "coverage", "invalid/duplicate service lanes")
            target = list(self.stages)
            keys = ["load_dst", "load_src"] if kind == "load" else ["store_dst"] if kind == "store" else ["read_src"]
            if kind == "load":
                require(j in (0, 8, 16, 24) and j not in self.loaded, "coverage", "wrong load iteration")
                before_stages, target = ["input"] * 32, ["loaded"] * 32
                require("%d words ready" % (len(self.loaded) * 256) in scene.status_text.text,
                        "counter", "load-ready count advanced before motion")
            else:
                require(self.barrier, "barrier", "shared read before the block barrier")
                before_stages = list(self.stages)
                if kind == "tray":
                    require(set(before_stages) == {"column"}, "ordering", "column extraction repeated")
                    target = ["tray"] * 32
                elif kind == "bank":
                    require(not self.in_bank, "ordering", "new service round before its predecessor returned")
                    require(len(batch) == (32 if stride == 33 else 1), "banks", "wrong conflict multiplicity")
                    require(not (set(batch) & self.returned), "coverage", "servicing a lane twice")
                    require(len({(base + lane * stride + self.column) % 32 for lane in batch}) == len(batch),
                            "banks", "different words share a bank in one schematic round")
                    for lane in batch:
                        require(before_stages[lane] == "tray", "ordering", "request left its tray early")
                        target[lane] = "bank"
                elif kind == "register":
                    require(set(batch) == self.in_bank, "ordering", "return does not match current requests")
                    for lane in batch:
                        target[lane] = "register"
                elif kind == "store":
                    require(self.returned == set(range(32)) and not self.in_bank,
                            "ordering", "global store began before all shared reads completed")
                    target = ["store"] * 32
                else:
                    raise FidelityError("unknown proof kind " + kind)
            source_expected = [expected_packet(i, stage, self.column, stride, base, j)
                               for i, stage in enumerate(before_stages)]
            expected = [expected_packet(i, stage, self.column, stride, base, j)
                        for i, stage in enumerate(target)]
            observed = observe_packets(proof["packets"])
            ids = validate_packets(observed, source_expected, None if kind == "load" else self.identities)
            if kind == "tray":
                self.identities = ids
            highlights = observe_highlights(scene)
            validate_highlights(highlights, keys)
            event.update(kind=kind, stride=stride, column=self.column, j=j, batch=batch,
                         expected_source=source_expected, expected_target=expected,
                         observed_source=observed, highlights=highlights)
            self.pending_target = target
            self.pending_batch = set(batch)
        self.current = event
        self.emit(event)

    def after(self, scene, proof):
        event = self.current
        require(event is not None, "ordering", "source endpoint without an interval")
        close([float(scene.time)], [event["end"]], "time", 0.000002)
        if proof is not None:
            kind = proof["kind"]
            observed = observe_packets(proof["packets"])
            validate_packets(observed, event["expected_target"], None if kind == "load" else self.identities)
            keys = ["load_dst", "load_src"] if kind == "load" else ["store_dst"] if kind == "store" else ["read_src"]
            validate_highlights(observe_highlights(scene), keys)
            if kind == "load":
                band = observe_leaf(proof["band"])
                close(band["center"], [1.55+16*0.11, 1.62-(proof["j"]+4)*0.11], "load-band")
                close([band["width"], band["height"]], [32*0.11, 8*0.11], "load-band")
                require(abs(band["alpha"]-0.23) < 0.005, "load-band", "load band fill differs")
                self.loaded.add(proof["j"])
            else:
                self.stages = self.pending_target
                if kind == "bank":
                    self.in_bank = self.pending_batch
                elif kind == "register":
                    self.returned |= self.pending_batch
                    self.in_bank = set()
            self.checks += 1
            self.emit({"type": "observed", "id": event["id"], "time": float(scene.time),
                       "kind": kind, "stride": proof["trace"]["stride"], "packets": observed})
        self.observe_code(scene)
        if self.pending_source_capture is not None:
            page = observe_source_page(scene)
            validate_source_page(page)
            self.emit({"type": "checkpoint", "name": self.pending_source_capture,
                       "time": float(scene.time), "stride": self.trace["stride"], "code_page": page})
            self.pending_source_capture = None
        self.current = None

    def finish(self, scene):
        require(self.completed == [32, 33], "coverage", "source did not finish both launches")
        require(self.page_counts == {"load": 2, "output": 2, "read": 2} and self.source_edits == 1,
                "source-edit", "all six source pages and the padded edit must execute")
        close([float(scene.time)], [76.14 / self.speed], "storyboard-duration", 0.000002)
        self.emit({"type": "complete", "checks": self.checks, "intervals": self.sequence,
                   "time": float(scene.time), "layouts": self.completed,
                   "sourcePages": self.page_counts, "sourceEdits": self.source_edits})
