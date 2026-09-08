import assert from "node:assert/strict";
import path from "node:path";
import { fileURLToPath } from "node:url";
import playwright from "playwright";
import { serveRepository } from "./browser-test-server.mjs";
import { browserArgs } from "./manim-raster-support.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

const source = `
from noon import *


def close(actual, expected, label, tolerance=1e-5):
    assert abs(float(actual) - float(expected)) <= tolerance, f"{label}: {actual} != {expected}"


def union_bounds(*members):
    return (
        min(member.get_critical_point(LEFT).x for member in members),
        min(member.get_critical_point(DOWN).y for member in members),
        max(member.get_critical_point(RIGHT).x for member in members),
        max(member.get_critical_point(UP).y for member in members),
    )


class RetainedFamilyLayout(Scene):
    def construct(self):
        first = Text("Layout A", font_size=40).shift(LEFT * 1.75 + UP * 0.4)
        second = Text("Layout BBB", font_size=32).shift(RIGHT * 1.25 + DOWN * 0.3)
        family = VGroup(first, VGroup(second))

        min_x, min_y, max_x, max_y = union_bounds(first, second)
        close(family.width, max_x - min_x, "nested retained family width")
        close(family.height, max_y - min_y, "nested retained family height")
        center = family.get_center()
        close(center.x, (min_x + max_x) * 0.5, "nested retained family center x")
        close(center.y, (min_y + max_y) * 0.5, "nested retained family center y")

        first_before = first.get_center()
        second_before = second.get_center()
        delta = RIGHT * 0.75 + UP * 0.55
        family.shift(delta)
        close(first.get_center().x, first_before.x + delta.x, "family shift first x")
        close(first.get_center().y, first_before.y + delta.y, "family shift first y")
        close(second.get_center().x, second_before.x + delta.x, "family shift second x")
        close(second.get_center().y, second_before.y + delta.y, "family shift second y")

        family.center()
        centered = family.get_center()
        close(centered.x, 0.0, "family center x")
        close(centered.y, 0.0, "family center y")

        arranged_a = Text("A", font_size=36)
        arranged_b = Text("BBBB", font_size=36)
        arranged = VGroup(arranged_a, VGroup(arranged_b))
        arranged.arrange(RIGHT, buff=0.375, center=True)
        gap = arranged_b.get_critical_point(LEFT).x - arranged_a.get_critical_point(RIGHT).x
        close(gap, 0.375, "nested retained arrange gap")
        arranged_center = arranged.get_center()
        close(arranged_center.x, 0.0, "arranged family center x")
        close(arranged_center.y, 0.0, "arranged family center y")

        square = Square(side_length=1.25).shift(LEFT * 0.9)
        mixed_text = Text("Mixed", font_size=30).shift(RIGHT * 0.8)
        mixed = VGroup(square, mixed_text)
        min_x, min_y, max_x, max_y = union_bounds(square, mixed_text)
        close(mixed.width, max_x - min_x, "mixed family width")
        close(mixed.height, max_y - min_y, "mixed family height")
        mixed_center = mixed.get_center()
        close(mixed_center.x, (min_x + max_x) * 0.5, "mixed family center x")
        close(mixed_center.y, (min_y + max_y) * 0.5, "mixed family center y")

        square_before = square.get_center()
        mixed_text_before = mixed_text.get_center()
        mixed.shift(DOWN * 0.6)
        close(square.get_center().y, square_before.y - 0.6, "mixed family square shift")
        close(mixed_text.get_center().y, mixed_text_before.y - 0.6, "mixed family text shift")

        placement_target = Text("Target", font_size=34).shift(RIGHT * 2.5 + UP * 0.8)
        placement_a = Text("P", font_size=30)
        placement_b = Text("QQ", font_size=30).shift(RIGHT * 0.8)
        placement = VGroup(placement_a, placement_b)
        placement.move_to(placement_target)
        close(placement.get_center().x, placement_target.get_center().x, "move_to retained target x")
        close(placement.get_center().y, placement_target.get_center().y, "move_to retained target y")
        placement.next_to(placement_target, RIGHT, buff=0.25)
        placement_left = placement.get_center().x - placement.width * 0.5
        gap = placement_left - placement_target.get_critical_point(RIGHT).x
        close(gap, 0.25, "next_to retained target gap")
        placement.align_to(placement_target, UP)
        placement_top = placement.get_center().y + placement.height * 0.5
        close(
            placement_top,
            placement_target.get_critical_point(UP).y,
            "align_to retained target top",
        )

        typst_label = Typst("*Typst*", font_size=36).shift(LEFT * 0.6)
        typst_equation = MathTypst("x^2", font_size=36).shift(RIGHT * 0.8)
        typst_family = VGroup(typst_label, typst_equation)
        min_x, min_y, max_x, max_y = union_bounds(typst_label, typst_equation)
        close(typst_family.width, max_x - min_x, "Typst family width")
        close(typst_family.height, max_y - min_y, "Typst family height")
        typst_family.next_to(placement_target, DOWN, buff=0.2)
        gap = placement_target.get_critical_point(DOWN).y - typst_family.get_critical_point(UP).y
        close(gap, 0.2, "Typst family next_to gap")
        close(
            typst_family.get_center().x,
            placement_target.get_center().x,
            "Typst family next_to alignment",
        )

        self.add(first, second, arranged_a, arranged_b, mixed_text, typst_family)
`;

const server = await serveRepository(root, 4194);
let browser;
try {
  browser = await playwright.chromium.launch({ channel: "chromium", headless: true, args: browserArgs("webgpu") });
  const page = await browser.newPage();
  const errors = [];
  page.on("pageerror", error => errors.push(String(error)));
  page.on("console", message => { if (message.type() === "error") errors.push(message.text()); });
  await page.goto(`${server.baseUrl}/web/manim-compat-smoke.html`);
  await page.waitForFunction(() => window.noonManimCompat, null, { timeout: 30000 });
  // runLive owns source attachment; the unrelated animated ready probes need
  // not run again for this static family qualification.
  const result = await page.evaluate(source => window.noonManimCompat.runLive(source), source);
  assert.equal(result.mode, "semantic");
  assert.equal(result.metrics.objectCount, 7);
  assert.equal(result.frame.objects.length, 7);
  assert.equal(result.frame.present_object_count, 7);
  assert.equal(new Set(result.frame.objects.map(object => object.id)).size, 7);
  assert.ok(result.metrics.presentedFrames > 0 && result.metrics.drawCalls > 0);
  assert.ok(result.metrics.instancesDrawn > 7, "Text and Typst must draw glyph instances");
  for (const object of result.frame.objects) {
    assert.ok(object.bounds.width > 0 && object.bounds.height > 0);
  }
  assert.deepEqual(errors, []);
  console.log("PASS shared Text family layout: preserved Python assertions and normal retained rendering");
} finally { await browser?.close(); await server.close(); }
