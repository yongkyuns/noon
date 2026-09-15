"""Compare actual Manim/Cairo and Noon/browser captures at identical scene times.

No fitting, registration, color correction, or mask warping is applied. Small
rasterization tolerances are separate from strict per-leaf semantic checks.
"""
from __future__ import annotations
import json
from pathlib import Path
import sys
import numpy as np
from PIL import Image, ImageFilter

root = Path(__file__).resolve().parents[1] / 'browser-smoke-artifacts'
manim_dir = root / 'tiger-manim-reference'
noon_dir = root / 'tiger-differential'
reference = json.loads((manim_dir / 'reference.json').read_text())
browser = json.loads((noon_dir / 'browser.json').read_text())
report = {'manim_version': reference['manim_version'], 'noon_runtime': browser['runtimeReference'],
          'samples': [], 'failures': [], 'tolerances': {
              'paint_channel_absolute': 1e-6, 'endpoint_bbox_pixels': 2,
              'foreground_iou': 0.95, 'blurred_foreground_mean_rgb_error_255': 8.0}}

def check(ok, message):
    if not ok:
        report['failures'].append(message)

def rgb_alpha(paint):
    if paint is None:
        return np.zeros(4)
    return np.array([paint['red'], paint['green'], paint['blue'], paint['alpha']])

def bounding(mask):
    y, x = np.nonzero(mask)
    if len(x) == 0:
        return None
    return [int(x.min()), int(y.min()), int(x.max()), int(y.max())]

captures = {capture['label']: capture for capture in browser['captures']}
check(browser['outcome'] == 'pass', 'Noon exact-time capture did not pass')
check(reference['outcome'] == 'pass', 'Manim reference did not pass')
for sample in reference['samples']:
    label = f"{sample['direction']}-{sample['alpha']:.2f}"
    actual = captures[label]['debug']['objects']
    expected = sample['paints']
    check(len(actual) == len(expected), f'{label}: leaf count mismatch')
    max_alpha_error = 0.0
    max_rgb_error = 0.0
    for index, (row, paint) in enumerate(zip(actual, expected)):
        for property in ['fill', 'stroke']:
            # debugFrame already reports effective paint including Appearance.
            a = rgb_alpha(row[property])
            b = np.array(paint[f'{property}_rgbas'][0])
            error = float(abs(a[3] - b[3]))
            max_alpha_error = max(max_alpha_error, error)
            check(error < 1e-6, f'{label} leaf {index} {property}: alpha error {error}')
            if min(a[3], b[3]) > 1e-6:
                error = float(np.max(np.abs(a[:3]-b[:3])))
                max_rgb_error = max(max_rgb_error, error)
                check(error < 1e-6, f'{label} leaf {index} {property}: RGB error {error}')
        check(abs(row['stroke_width'] - paint['stroke_width']*0.01) < 1e-7,
              f'{label} leaf {index}: screen-space stroke width mismatch')
    a_image = Image.open(noon_dir / f'{label}.png').convert('RGB')
    b_image = Image.open(manim_dir / f'{label}.png').convert('RGB')
    check(a_image.size == b_image.size, f'{label}: canvas dimensions differ')
    a = np.array(a_image, dtype=float)
    b = np.array(b_image, dtype=float)
    a_mask = a.max(axis=2) > 25
    b_mask = b.max(axis=2) > 25
    union = a_mask | b_mask
    iou = float((a_mask & b_mask).sum() / max(1, union.sum()))
    blurred_a = np.array(a_image.filter(ImageFilter.GaussianBlur(0.5)), dtype=float)
    blurred_b = np.array(b_image.filter(ImageFilter.GaussianBlur(0.5)), dtype=float)
    error = float(np.abs(blurred_a-blurred_b)[union].mean())
    bbox_a, bbox_b = bounding(a_mask), bounding(b_mask)
    check(iou >= 0.95, f'{label}: foreground IoU {iou:.6f} < 0.95')
    check(error <= 8.0, f'{label}: foreground RGB error {error:.6f} > 8.0')
    if sample['alpha'] in [0.0, 1.0]:
        check(bbox_a is not None and bbox_b is not None and
              max(abs(x-y) for x,y in zip(bbox_a, bbox_b)) <= 2,
              f'{label}: endpoint bounds differ {bbox_a} != {bbox_b}')
    report['samples'].append({'sample': label, 'max_alpha_error': max_alpha_error,
        'max_rgb_error': max_rgb_error, 'foreground_iou': iou,
        'blurred_foreground_rgb_error_255': error, 'noon_bbox': bbox_a, 'manim_bbox': bbox_b})
    # Evidence is a side-by-side raster capture, never an adjusted comparison.
    side = Image.new('RGB', (a_image.width*2, a_image.height))
    side.paste(b_image, (0,0))
    side.paste(a_image, (a_image.width,0))
    side.save(noon_dir / f'comparison-{label}.png')

counterexample = [row['fill']['alpha'] for row in browser['paddingPaintMidpoint']['objects']]
report['padding_midpoint'] = counterexample
check(counterexample == reference['padding_opacity_counterexample'], 'padding opacity counterexample differs')
check(browser['restorationChangedPixels'] == 0, 'Noon did not return to its exact original raster')
report['outcome'] = 'pass' if not report['failures'] else 'fail'
(noon_dir / 'manim-comparison.json').write_text(json.dumps(report, indent=2))
print(json.dumps(report, indent=2))
sys.exit(0 if report['outcome'] == 'pass' else 1)
