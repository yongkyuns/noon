"""A light ray in a triangular mirror: exact collisions, retained animation.

Edit the four constants below, then Run. All physics is computed once; ordinary
Create/move_to plays own interpolation in Noon's shared Rust runtime. There are
no Python frame callbacks, numerical integration steps, or browser-only effects.
A ray aimed exactly at a corner is deliberately rejected: a polygon vertex has
no unique mirror normal. Change LAUNCH_ANGLE_DEGREES slightly in that case.

Inspired by the triangle-light request: https://www.youtube.com/watch?v=z_sXLlLVUls
"""
import colorsys
import math

from noon import *


TRIANGLE = ((-3.0, -2.35), (3.0, -2.35), (0.0, -2.35 + 3.0 * math.sqrt(3.0)))
LAUNCH_POINT = (-0.63, -0.73)
LAUNCH_ANGLE_DEGREES = 17.0
BOUNCES = 44
FLIGHT_SECONDS = 22.0


def cross(a, b):
    return a[0] * b[1] - a[1] * b[0]


def difference(a, b):
    return (a[0] - b[0], a[1] - b[1])


def trace_ray(vertices, origin, angle_degrees, bounces):
    """Return (points, wall_indices), stopping precisely on each mirror.

    Ray/edge intersections use distance along a unit direction, not integration.
    Reflection uses d' = d - 2 (d dot n) n. Edge winding does not affect the result.
    The origin must be strictly inside a finite, non-degenerate triangle.
    """
    if isinstance(bounces, bool) or not isinstance(bounces, int) or bounces < 1:
        raise ValueError("BOUNCES must be a positive integer")
    if len(vertices) != 3 or any(len(v) != 2 for v in vertices) or len(origin) != 2:
        raise ValueError("Use exactly three 2D vertices and one 2D launch point")
    if not all(math.isfinite(x) for v in (*vertices, origin) for x in v):
        raise ValueError("Triangle and launch point coordinates must be finite")
    if not math.isfinite(angle_degrees):
        raise ValueError("LAUNCH_ANGLE_DEGREES must be finite")
    edges = [difference(vertices[(i + 1) % 3], vertices[i]) for i in range(3)]
    scale = max(math.hypot(*edge) for edge in edges)
    area = cross(edges[0], difference(vertices[2], vertices[0]))
    if scale == 0 or abs(area) <= 1e-12 * scale * scale:
        raise ValueError("TRIANGLE must have non-zero area")
    winding = 1.0 if area > 0 else -1.0
    if any(winding * cross(edge, difference(origin, vertices[i])) <= 1e-10 * scale * scale
           for i, edge in enumerate(edges)):
        raise ValueError("LAUNCH_POINT must be strictly inside TRIANGLE")

    angle = math.radians(angle_degrees % 360.0)
    direction = (math.cos(angle), math.sin(angle))
    position = tuple(origin)
    points, walls = [position], []
    distance_epsilon = 1e-10 * scale
    for bounce in range(bounces):
        candidates = []
        for wall, edge in enumerate(edges):
            denominator = cross(direction, edge)
            if abs(denominator) <= 1e-12 * math.hypot(*edge):
                continue  # Parallel to this mirror; another wall must be ahead.
            offset = difference(vertices[wall], position)
            distance = cross(offset, edge) / denominator
            fraction = cross(offset, direction) / denominator
            if distance > distance_epsilon and -1e-10 <= fraction <= 1.0 + 1e-10:
                candidates.append((distance, wall, fraction))
        if not candidates:
            raise ValueError("No forward wall intersection; move the launch away from a corner")
        candidates.sort()
        _, wall, fraction = candidates[0]
        if fraction <= 1e-9 or fraction >= 1.0 - 1e-9:
            raise ValueError(f"Ray hits a triangle corner at bounce {bounce + 1}; change LAUNCH_ANGLE_DEGREES slightly")
        # Reconstruct on the edge rather than nudging the ray into the interior.
        # Epsilon nudges accumulate drift and would change the authored path.
        edge = edges[wall]
        position = (vertices[wall][0] + fraction * edge[0],
                    vertices[wall][1] + fraction * edge[1])
        points.append(position)
        walls.append(wall)
        length = math.hypot(*edge)
        normal = (-edge[1] / length, edge[0] / length)
        projection = direction[0] * normal[0] + direction[1] * normal[1]
        reflected = (direction[0] - 2.0 * projection * normal[0],
                     direction[1] - 2.0 * projection * normal[1])
        norm = math.hypot(*reflected)
        direction = (reflected[0] / norm, reflected[1] / norm)
    return points, walls


def flight_durations(points, seconds):
    """One constant visual speed, including short near-corner flight segments."""
    if not math.isfinite(seconds) or seconds <= 0:
        raise ValueError("FLIGHT_SECONDS must be finite and positive")
    lengths = [math.dist(a, b) for a, b in zip(points, points[1:])]
    total = math.fsum(lengths)
    if not lengths or any(length <= 0 or not math.isfinite(length) for length in lengths):
        raise ValueError("The ray path must contain finite, non-zero segments")
    return [seconds * length / total for length in lengths]


def point3(point):
    return (point[0], point[1], 0.0)


class TriangleLightShowcase(Scene):
    def construct(self):
        points, _ = trace_ray(TRIANGLE, LAUNCH_POINT, LAUNCH_ANGLE_DEGREES, BOUNCES)
        durations = flight_durations(points, FLIGHT_SECONDS)
        night = Color(0.025, 0.035, 0.075)
        muted = Color(0.58, 0.68, 0.82)
        cyan = Color(0.20, 0.85, 1.0)
        background = Rectangle(width=16.0, height=9.0, fill=night,
                               fill_opacity=1.0, stroke=None)
        title = Text("LIGHT IN A TRIANGLE", font_size=31, color=WHITE).shift(3.35 * UP)
        lesson = Text("Equal angles at every wall. Constant speed between bounces.",
                      font_size=18, color=muted).shift(3.05 * DOWN)
        mirror = VMobject(stroke_color=WHITE, stroke_width=2.5, fill_opacity=0)
        mirror.set_points_as_corners([point3(v) for v in (*TRIANGLE, TRIANGLE[0])])
        photon = Dot(point3(points[0]), radius=0.055, color=WHITE)
        # Only the moving source owns a glow attachment. Completed trail segments
        # remain ordinary retained paths rather than dozens of offscreen effects.
        photon.set_glow(color=cyan, radius=Pixels(9), intensity=1.8)
        self.add(background)
        self.play(FadeIn(title), FadeIn(lesson), Create(mirror), run_time=1.0)
        self.play(FadeIn(photon), run_time=0.4)

        for index, (start, end, duration) in enumerate(zip(points, points[1:], durations)):
            rgb = colorsys.hsv_to_rgb(0.51 + 0.36 * index / max(1, BOUNCES - 1), 0.68, 1.0)
            color = Color(*rgb)
            ray = VMobject(stroke_color=color, stroke_width=1.8, fill_opacity=0)
            ray.set_points_as_corners([point3(start), point3(end)])
            # A straight path's reveal and the dot's affine motion share one
            # linear play; no easing pause or rounded trajectory at a reflection.
            self.play(Create(ray), photon.animate.move_to(point3(end)),
                      run_time=duration, rate_func=linear)

        resolved = Text(f"One ray. {BOUNCES} reflections. One simple law.",
                        font_size=21, color=WHITE).shift(3.05 * DOWN)
        self.play(FadeOut(lesson), FadeIn(resolved), run_time=0.6)
        self.wait(1.0)
