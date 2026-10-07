#!/usr/bin/env python3
"""Numerical design check, NOT GPU/shader execution or product qualification.

Compare a float32 separable simulation with packed RGB24 intermediates against
an independent float64, full two-dimensional Gaussian convolution. The actual
Rust/GPU readback tests remain authoritative for implementation qualification.
"""
import argparse
import hashlib
import json
import math
from pathlib import Path
import numpy as np


def reference(mask, sigma):
    radius = math.ceil(3 * sigma)
    y, x = np.mgrid[-radius:radius + 1, -radius:radius + 1]
    kernel = np.exp(-(x.astype(np.float64)**2 + y.astype(np.float64)**2) / (2 * sigma**2))
    kernel /= kernel.sum()
    shape = tuple(a + b - 1 for a, b in zip(mask.shape, kernel.shape))
    # Full 2D convolution; no production 1D coefficients or intermediate masks.
    result = np.fft.irfftn(np.fft.rfftn(mask.astype(np.float64), shape, axes=(0, 1)) *
                          np.fft.rfftn(kernel, shape, axes=(0, 1)), shape, axes=(0, 1))
    return result[radius:radius + mask.shape[0], radius:radius + mask.shape[1]]


def stored_mask(value, bits):
    scale = np.float32((1 << bits) - 1)
    encoded = np.rint(np.clip(value, 0, 1).astype(np.float32) * scale).astype(np.uint32)
    if bits == 8:
        return encoded.astype(np.float32) / scale
    red = ((encoded >> 16) & 255).astype(np.float32)
    green = ((encoded >> 8) & 255).astype(np.float32)
    blue = (encoded & 255).astype(np.float32)
    return ((red * np.float32(65536) + green * np.float32(256)) + blue) / scale


def candidate(mask, sigma, bits=24):
    radius = math.ceil(3 * sigma)
    offsets = np.arange(-radius, radius + 1, dtype=np.float64)
    coefficients = np.exp(-0.5 * (offsets / sigma)**2)
    coefficients = (coefficients / coefficients.sum()).astype(np.float32)
    out = mask.astype(np.float32)
    for axis in (1, 0):
        sums = np.zeros_like(out)
        for delta, weight in zip(range(-radius, radius + 1), coefficients):
            length = out.shape[axis]
            if abs(delta) >= length:
                continue
            dst, src = [slice(None), slice(None)], [slice(None), slice(None)]
            dst[axis] = slice(max(0, -delta), min(length, length - delta))
            src[axis] = slice(max(0, delta), min(length, length + delta))
            sums[tuple(dst)] += weight * out[tuple(src)]
        out = stored_mask(sums, bits)
    return out


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--report', type=Path, required=True)
    args = parser.parse_args()
    rng = np.random.default_rng(1897)
    y, x = np.mgrid[:17, :23]
    inputs = {
        'opaque-edge': np.ones((17, 23)),
        'one-byte-faint-rectangle': ((x >= 5) & (x <= 15) & (y >= 4) & (y <= 12)) / 255,
        'fractional-circle': np.clip(4.2 - np.hypot(x - 11.3, y - 8.7), 0, 1),
        'asymmetric-mask': rng.integers(0, 256, (17, 23)) / 255,
    }
    results = []
    for name, value in inputs.items():
        value = np.rint(value * 255) / 255  # exact RGBA8 capture domain
        for sigma in (0.25, 0.7, 1.0, 3.25, 12.0, 64.0):
            expected = reference(value, sigma)
            measured = candidate(value, sigma)
            error = float(np.max(np.abs(expected - measured)))
            assert error <= 1e-5, (name, sigma, error)
            results.append({'case': name, 'sigma': sigma, 'float_absolute_error': error})
    faint = np.rint(inputs['one-byte-faint-rectangle'] * 255) / 255
    negative_error = float(np.max(np.abs(reference(faint, 3.25) - candidate(faint, 3.25, bits=8))))
    assert negative_error > 1e-5, 'negative control failed to reject 8-bit intermediates'
    report = {
        'schema': 1,
        'status': 'numerical-design-only; Rust compilation and GPU execution not implied',
        'cases': results,
        'maximum_float_absolute_error': max(row['float_absolute_error'] for row in results),
        'rgba8_mask_negative_control_error': negative_error,
        'negative_control_rejected': True,
        'tolerance': 1e-5,
        'numpy_version': np.__version__,
        'script_sha256': hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
    }
    args.report.write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps({k: v for k, v in report.items() if k != 'cases'}, indent=2))


if __name__ == '__main__':
    main()
