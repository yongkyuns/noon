"""Fourier epicycles for https://yongkyuns.github.io/noon/.

Revision 3: prepared native playback. No add_updater, per-frame Python math,
or per-frame path replacement. NumPy runs only while preparing the scene.
Paste this complete file into the playground and press Run.

Customize SVG_SOURCE, or supply CUSTOM_POINTS as ordered (x, y) coordinates.
CONTOUR_INDEX selects one SVG contour, sorted longest first. Open contours
are traced out and back. All stages use the same original Fourier coefficients.

Positions use bounded piecewise-linear interpolation; line rotations remain
continuous at their signed Fourier frequencies. MAX_POINT_ERROR controls the
preparation/memory/accuracy tradeoff. This is a scene-unit error, not a pixel count.
Only fixed analytic geometry is retained; native Transform changes its pose.

Validated against deployed Noon 926bdbf45faf32a03751c412063255655b8aa77a:
real Pyodide/NumPy/WASM execution, native replay samples, and browser WebGPU.
Preparation takes longer than the updater version; lowering MAX_POINT_ERROR
adds motion keys and preparation work. Python still authors at scene boundaries,
but it is not invoked for the animation frames between those boundaries.
"""

from noon import *
import numpy as np


TERM_COUNTS = (7, 21, 61)
SECONDS_PER_DRAWING = 9.0
SAMPLE_COUNT = 1024
MAX_POINT_ERROR = 0.01  # Smaller = more native position keys and longer preparation.
MIN_MOTION_SEGMENTS = 16
MAX_MOTION_SEGMENTS = 1024
CHECK_SEGMENTS = 4096
CONTOUR_INDEX = 0
CUSTOM_POINTS = None

SVG_SOURCE = """<svg xmlns="http://www.w3.org/2000/svg" viewBox="-110 -105 220 190">
  <path d="M 0 -24
    C -10 -48 -24 -78 -47 -85
    C -91 -102 -105 -70 -95 -35
    C -90 -16 -76 -1 -57 8
    C -87 7 -89 34 -71 55
    C -54 76 -28 60 -9 30
    C -4 45 -5 61 0 75
    C 5 61 4 45 9 30
    C 28 60 54 76 71 55
    C 89 34 87 7 57 8
    C 76 -1 90 -16 95 -35
    C 105 -70 91 -102 47 -85
    C 24 -78 10 -48 0 -24 Z"/>
</svg>"""


def ordered_outline():
    """Resolve one continuous outline before Noon enters live execution."""
    if CUSTOM_POINTS is not None:
        points = np.asarray(CUSTOM_POINTS, dtype=float)
        if points.ndim != 2 or points.shape[1] not in (2, 3):
            raise ValueError("CUSTOM_POINTS must contain ordered (x, y) or (x, y, z) rows.")
        return points[:, 0] + 1j * points[:, 1]

    imported = SVGMobject.from_string(SVG_SOURCE, height=4)
    contours = []
    for member in imported.family_members_with_points():
        for control_points in member.get_subpaths():
            if len(control_points) >= 4:
                contours.append(VMobject().set_points(control_points))
    contours.sort(key=lambda path: path.get_arc_length(), reverse=True)
    if not 0 <= CONTOUR_INDEX < len(contours):
        raise ValueError("CONTOUR_INDEX must be between 0 and " + str(len(contours) - 1))

    path = contours[CONTOUR_INDEX]
    # Noon returns Vec2 here. Read x/y explicitly instead of assuming XYZ arrays.
    sampled = [path.point_from_proportion(float(t)) for t in np.linspace(0, 1, 2 * SAMPLE_COUNT + 1)]
    return np.array([complex(float(p[0]), float(p[1])) for p in sampled])


def resample_outline(points, count):
    """Resample a closed/retraced polyline at approximately equal distances."""
    z = np.asarray(points, dtype=complex).reshape(-1)
    if len(z) < 2 or not np.all(np.isfinite(z)):
        raise ValueError("The outline needs at least two finite, distinct points.")
    extent = max(float(np.ptp(z.real)), float(np.ptp(z.imag)))
    if extent <= 1e-12:
        raise ValueError("The outline has no spatial extent.")
    if abs(z[-1] - z[0]) > extent * 1e-6:
        z = np.concatenate((z, z[-2::-1]))  # Open path: return along the same contour.
    else:
        z = z.copy()
        z[-1] = z[0]

    # Remove repeated vertices so np.interp has strictly increasing distances.
    z = z[np.concatenate(([True], np.abs(np.diff(z)) > extent * 1e-12))]
    distances = np.concatenate(([0.0], np.cumsum(np.abs(np.diff(z)))))
    if len(z) < 2 or distances[-1] <= 0:
        raise ValueError("The outline has zero length.")
    positions = np.linspace(0, distances[-1], count, endpoint=False)
    return np.interp(positions, distances, z.real) + 1j * np.interp(positions, distances, z.imag)


def fourier_model(samples, term_counts):
    """Compute the FFT once and fit the complete epicycle envelope to the scene."""
    if not term_counts or any(n < 3 or n % 2 == 0 for n in term_counts):
        raise ValueError("TERM_COUNTS must contain odd integers of at least 3.")
    if tuple(sorted(set(term_counts))) != tuple(term_counts):
        raise ValueError("TERM_COUNTS must be strictly increasing.")
    if max(term_counts) >= len(samples):
        raise ValueError("SAMPLE_COUNT must exceed every term count.")

    spectrum = np.fft.fft(samples) / len(samples)
    highest = (max(term_counts) - 1) // 2
    frequencies = np.array([k for n in range(1, highest + 1) for k in (n, -n)])
    coefficients = spectrum[frequencies % len(samples)]
    offset = complex(spectrum[0])

    # Bound every intermediate circle, not just the final silhouette.
    phases = np.linspace(0, 1, 513)
    rotating = np.exp(2j * np.pi * np.outer(phases, frequencies)) * coefficients
    centers = offset + np.concatenate((np.zeros((len(phases), 1)), np.cumsum(rotating, axis=1)[:, :-1]), axis=1)
    radii = np.abs(coefficients)
    x_min = min(float(np.min(centers.real - radii)), float(np.min(samples.real)))
    x_max = max(float(np.max(centers.real + radii)), float(np.max(samples.real)))
    y_min = min(float(np.min(centers.imag - radii)), float(np.min(samples.imag)))
    y_max = max(float(np.max(centers.imag + radii)), float(np.max(samples.imag)))
    scale = min(10.6 / max(x_max - x_min, 1e-9), 4.45 / max(y_max - y_min, 1e-9))
    translation = -scale * complex((x_min + x_max) / 2, (y_min + y_max) / 2) - 0.15j
    coefficients = coefficients * scale
    offset = offset * scale + translation

    return frequencies, coefficients, offset, samples * scale + translation


def xy(z):
    return (float(z.real), float(z.imag))


def prepare_positions(frequencies, coefficients, offset, count):
    """Bound linear-interpolation error for every cumulative Fourier position."""
    if not np.isfinite(MAX_POINT_ERROR) or MAX_POINT_ERROR <= 0:
        raise ValueError("MAX_POINT_ERROR must be finite and positive.")
    f = frequencies[:count]
    c = coefficients[:count]
    # For any partial sum, |z''(phase)| <= sum |c_k| (2*pi*k)^2.
    # Linear interpolation on an interval of width h has error <= max|z''| h^2/8.
    curvature_bound = float(np.sum(np.abs(c) * (2 * np.pi * f) ** 2))
    segments = CHECK_SEGMENTS
    if segments < MIN_MOTION_SEGMENTS or segments & (segments - 1):
        raise ValueError("CHECK_SEGMENTS must be a power of two at least MIN_MOTION_SEGMENTS.")
    residual_bound = curvature_bound / (8 * segments * segments)
    if residual_bound >= MAX_POINT_ERROR:
        raise ValueError("Increase CHECK_SEGMENTS to certify this outline at MAX_POINT_ERROR.")
    phase = np.linspace(0, 1, segments + 1)
    rotating = np.exp(2j * np.pi * np.outer(phase, f)) * c
    chain = offset + np.concatenate((np.zeros((segments + 1, 1)), np.cumsum(rotating, axis=1)), axis=1)
    chain[-1] = chain[0]  # Exact closed-loop endpoint; discard floating-point roundoff.
    # On each fine interval, the unsampled error is at most residual_bound.
    # Checking the dense samples against a candidate chord therefore certifies
    # the entire continuous interval, rather than only its midpoint.
    keys = [0]
    accepted_errors = []

    def subdivide(first, last):
        fraction = np.linspace(0, 1, last - first + 1)[:, None]
        chord = chain[first] + fraction * (chain[last] - chain[first])
        measured = float(np.max(np.abs(chain[first:last + 1] - chord)))
        if measured + residual_bound <= MAX_POINT_ERROR:
            keys.append(last)
            accepted_errors.append(measured + residual_bound)
            return
        middle = (first + last) // 2
        if middle == first:
            raise ValueError("Increase CHECK_SEGMENTS to resolve the requested motion accuracy.")
        subdivide(first, middle)
        subdivide(middle, last)

    # Dyadic boundaries also avoid cumulative rounding between native clips.
    minimum = 1 << (int(MIN_MOTION_SEGMENTS) - 1).bit_length()
    stride = segments // minimum
    for first in range(0, segments, stride):
        subdivide(first, first + stride)
    if len(keys) - 1 > MAX_MOTION_SEGMENTS:
        raise ValueError("This outline needs more motion keys. Increase MAX_POINT_ERROR or MAX_MOTION_SEGMENTS.")
    return phase[keys], chain[keys], max(accepted_errors)


def native_poses(mobject, positions, phase, turns=0):
    """Keep one immutable sampled pose sequence for this stable geometry."""
    return SampledMotion(
        mobject,
        [float(t) for t in phase[1:]],
        [xy(point) for point in positions[1:]],
        angles=None if not turns else [float(2 * np.pi * turns * t) for t in phase[1:]],
        run_time=SECONDS_PER_DRAWING,
        rate_func=linear,
    )


def prepare_stage(frequencies, coefficients, offset, terms):
    count = terms - 1
    phase, chain, error_bound = prepare_positions(frequencies, coefficients, offset, count)
    objects = []
    animations = []

    for index, coefficient in enumerate(coefficients[:count]):
        radius = float(abs(coefficient))
        if radius <= 1e-7:
            continue
        circle = Circle(radius=radius, color=BLUE, stroke_width=1, fill_opacity=0)
        circle.move_to(xy(chain[0, index])).set_object_opacity(0.24)
        objects.append(circle)
        if index:  # The first circle's center is the fixed Fourier offset.
            animations.append(native_poses(circle, chain[:, index], phase))

    for index, coefficient in enumerate(coefficients[:count]):
        if abs(coefficient) <= 1e-7:
            continue
        midpoint = (chain[:, index] + chain[:, index + 1]) / 2
        # Store the coefficient's initial phase in the fixed line geometry.
        vector = Line(xy(-coefficient / 2), xy(coefficient / 2), color=YELLOW, stroke_width=1.6)
        vector.move_to(xy(midpoint[0])).set_object_opacity(0.8)
        objects.append(vector)
        animations.append(native_poses(vector, midpoint, phase, int(frequencies[index])))

    endpoint = chain[:, -1]
    halo = Dot(xy(endpoint[0]), radius=0.13, color=TEAL).set_object_opacity(0.15)
    tip = Dot(xy(endpoint[0]), radius=0.045, color=YELLOW)
    objects.extend((halo, tip))
    animations.extend((native_poses(halo, endpoint, phase), native_poses(tip, endpoint, phase)))

    # Create uses uniform curve-count progress. Subdivide the adaptive pen chords
    # onto the finest used time grid: this adds only collinear points, so native
    # trace reveal and the pen remain exactly synchronized at every time.
    trace_segments = int(round(1 / float(np.min(np.diff(phase)))))
    trace_phase = np.linspace(0, 1, trace_segments + 1)
    trace_points = np.interp(trace_phase, phase, endpoint.real) + 1j * np.interp(trace_phase, phase, endpoint.imag)
    trace = VMobject(stroke_color=TEAL, stroke_width=3.2, fill_opacity=0)
    trace.set_points_as_corners([xy(p) for p in trace_points])
    animations.append(Create(trace, run_time=SECONDS_PER_DRAWING, rate_func=linear))
    return VGroup(*objects), trace, animations, len(phase) - 1, error_bound


class FourierEpicycles(Scene):
    def construct(self):
        if not np.isfinite(SECONDS_PER_DRAWING) or SECONDS_PER_DRAWING <= 0:
            raise ValueError("SECONDS_PER_DRAWING must be finite and positive.")
        samples = resample_outline(ordered_outline(), SAMPLE_COUNT)
        frequencies, coefficients, offset, outline = fourier_model(samples, TERM_COUNTS)

        # Prepare every target before playback; Python does not run between motion keys.
        stages = [prepare_stage(frequencies, coefficients, offset, n) for n in TERM_COUNTS]

        title = Text("FOURIER EPICYCLES", font_size=32).move_to((0, 3.25, 0))
        subtitle = Text("One outline. A sum of rotating vectors.", font_size=19, color=GRAY).move_to((0, 2.65, 0))
        caption = Text("More frequencies recover finer details.", font_size=19, color=GRAY).move_to((0, -2.8, 0))
        finished = Text("A drawing made entirely from circular motion.", font_size=19, color=TEAL).move_to((0, -2.8, 0))
        label_x = np.linspace(-3.6, 3.6, len(TERM_COUNTS)) if len(TERM_COUNTS) > 1 else [0]
        labels = [Text(str(n) + " terms", font_size=22, color=TEAL).move_to((float(x), -3.35, 0)) for n, x in zip(TERM_COUNTS, label_x)]
        for label in labels:
            label.set_opacity(0.3)

        reference = VMobject(stroke_color=GRAY, stroke_width=1.2, fill_opacity=0)
        reference.set_points_as_corners([xy(p) for p in outline] + [xy(outline[0])])
        reference.set_object_opacity(0.35)
        self.add(reference)
        self.play(FadeIn(title), FadeIn(subtitle), FadeIn(caption), *[FadeIn(label) for label in labels], run_time=0.7, rate_func=smooth)

        previous = None
        for index, (rig, trace, animations, _segments, _error) in enumerate(stages):
            if previous is not None:
                self.play(FadeOut(previous[0]), FadeOut(previous[1]), labels[index - 1].animate.set_opacity(0.3), run_time=0.35, rate_func=smooth)
            self.play(FadeIn(rig), labels[index].animate.set_opacity(1), run_time=0.7, rate_func=smooth)
            self.play(*animations, run_time=SECONDS_PER_DRAWING, rate_func=linear)
            self.wait(0.55)
            previous = (rig, trace)

        self.play(FadeOut(previous[0]), FadeOut(caption), FadeIn(finished), run_time=1.0, rate_func=smooth)
        self.wait(1.5)
