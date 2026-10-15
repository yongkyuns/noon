// Pixel acceptance checks. Consumes actual Noon canvas PNGs in the browser runner.
// Unit tests use synthetic pixel buffers only to verify these predicates themselves.
import assert from 'node:assert/strict';

function indices(image, center, width, height) {
  const u = image.height / 8;
  const x0 = Math.max(0, Math.ceil(image.width / 2 + (center[0] - width / 2) * u));
  const x1 = Math.min(image.width - 1, Math.floor(image.width / 2 + (center[0] + width / 2) * u));
  const y0 = Math.max(0, Math.ceil(image.height / 2 - (center[1] + height / 2) * u));
  const y1 = Math.min(image.height - 1, Math.floor(image.height / 2 - (center[1] - height / 2) * u));
  const out = [];
  for (let y = y0; y <= y1; ++y) for (let x = x0; x <= x1; ++x) out.push((y * image.width + x) * 4);
  return out;
}
function fraction(image, region, predicate) {
  const samples = indices(image, region.center, region.width, region.height);
  assert.ok(samples.length, 'CUDA_PIXELS[coverage]: empty pixel region');
  return samples.filter(i => predicate(image.data[i], image.data[i+1], image.data[i+2])).length / samples.length;
}
function mean(image, region) {
  const samples = indices(image, region.center, region.width, region.height);
  assert.ok(samples.length, 'CUDA_PIXELS[coverage]: empty highlight sample');
  return samples.reduce((s, i) => s + (image.data[i] + image.data[i+1] + image.data[i+2]) / 3, 0) / samples.length;
}
function overlap(a, b) {
  return Math.abs(a.center[0] - b.center[0]) < (a.width + b.width) / 2 + 0.012 &&
    Math.abs(a.center[1] - b.center[1]) < (a.height + b.height) / 2 + 0.012;
}
const mix = (a, b, t) => a + (b-a)*t;

export function checkTextRegions(image, regions) {
  for (const region of regions) {
    const coverage = fraction(image, region, (r,g,b) => Math.max(r,g,b) > 145);
    assert.ok(coverage >= 0.015, `CUDA_PIXELS[text]: missing glyph coverage for ${region.text}`);
  }
  return regions.length;
}

export function checkHighlight(image, highlights) {
  const reference = mean(image, {center: [-6.40, 2.34], width: 0.09, height: 0.12});
  const active = [];
  for (const [key, h] of Object.entries(highlights)) {
    // A reserved gap between line numbers and source text, not glyph pixels.
    const light = mean(image, {center: [-6.40, h.center[1]], width: 0.09, height: 0.12});
    const delta = light - reference;
    if (h.alpha > 0.0001) {
      assert.ok(delta >= 9, `CUDA_PIXELS[highlight]: ${key} indistinguishable from background (${delta.toFixed(2)})`);
      checkTextRegions(image, [{center:h.text_center, width:h.text_width, height:h.text_height, text:h.text}]);
      active.push({key, contrastDelta: delta});
    } else {
      assert.ok(Math.abs(delta) <= 5, `CUDA_PIXELS[highlight]: inactive ${key} appears highlighted (${delta.toFixed(2)})`);
    }
  }
  assert.ok(active.length, 'CUDA_PIXELS[highlight]: no active source row');
  return active;
}

export function checkSample(image, event, alpha) {
  assert.ok(event.kind, 'a packet-operation interval is required');
  // Tests use exactly the symmetric easing midpoint, or a sample infinitesimally
  // before completion. They do not assume linear timing for arbitrary alpha.
  assert.ok(alpha === 0.5 || alpha === 1);
  const poses = event.expected_target.map((b, i) => {
    const a = event.expected_source[i];
    return {...b, center: [mix(a.center[0], b.center[0], alpha), mix(a.center[1], b.center[1], alpha)],
      width: mix(a.width, b.width, alpha), height: mix(a.height, b.height, alpha)};
  });
  let checked = 0, occluded = 0, labels = 0, ghosts = 0;
  for (const pose of poses) {
    // Legitimate crossings can occlude a packet at a midpoint. Never assert a
    // hidden back-layer packet must be visible; endpoints must be unoccluded.
    if (poses.some(other => other.lane !== pose.lane && overlap(pose, other))) {
      ++occluded;
      assert.equal(alpha, 0.5, `CUDA_PIXELS[overlap]: endpoint packets overlap at lane ${pose.lane}`);
      continue;
    }
    const region = {...pose, width: pose.width * 0.82, height: pose.height * 0.82};
    const colorMatch = (r,g,b) => [r,g,b].every((v,i) => Math.abs(v - pose.rgb[i]) < 42);
    const coverage = fraction(image, region, colorMatch);
    assert.ok(coverage >= 0.28,
      `CUDA_PIXELS[packet]: ${event.kind} lane ${pose.lane} missing/wrong color at ${pose.center} (${coverage})`);
    ++checked;
    if (pose.has_label && pose.width >= 0.24 && pose.height >= 0.12) {
      // Project the label's ink bounds through the same uniform packet scale.
      // A tiny antialiased digit can cover only a few dark pixels: dividing by
      // the much larger packet area incorrectly rejects visibly rendered text.
      const source = event.expected_source[pose.lane];
      const label = event.observed_source[pose.lane].label;
      assert.ok(label && label.width > 0 && label.height > 0,
        `CUDA_PIXELS[label]: packet ${pose.lane} has no source glyph bounds`);
      const glyph = {center: pose.center,
        width: label.width * pose.width / source.width,
        height: label.height * pose.height / source.height};
      // Glyph edges blend the authored dark ink with the bright packet. Check
      // at least half of that color contrast instead of requiring a fully dark
      // pixel, which a subpixel stem need never contain.
      const ink = [12,18,32];
      const contrast = pose.rgb.map((value, i) => value - ink[i]);
      const squaredContrast = contrast.reduce((sum, value) => sum + value * value, 0);
      const dark = fraction(image, glyph, (r,g,b) =>
        [r,g,b].reduce((sum, value, i) => sum + (pose.rgb[i] - value) * contrast[i], 0)
          >= squaredContrast * 0.5);
      assert.ok(dark >= 0.012, `CUDA_PIXELS[label]: packet ${pose.lane} has no rendered lane text`);
      ++labels;
    }
    if (alpha === 1 && event.kind !== 'load') {
      const old = event.expected_source[pose.lane];
      const moved = Math.hypot(old.center[0] - pose.center[0], old.center[1] - pose.center[1]) > 0.05;
      if (moved && !poses.some(other => overlap(old, other))) {
        const remaining = fraction(image, {...old, width: old.width*0.75, height: old.height*0.75}, colorMatch);
        assert.ok(remaining < 0.20, `CUDA_PIXELS[ghost]: lane ${pose.lane} remains at its old position`);
        ++ghosts;
      }
    }
  }
  if (alpha === 1) assert.equal(checked, 32, 'CUDA_PIXELS[coverage]: did not inspect all endpoint packets');
  return {checked, occluded, labels, ghosts, highlights: checkHighlight(image, event.highlights)};
}
