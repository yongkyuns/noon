# Copyright (c) 2026 Academa, Inc.
# SPDX-License-Identifier: MIT
# See THIRD_PARTY_NOTICES.md for license and provenance.
# Source: https://manimgx.academa.ai/gallery/miura-ori/
# https://github.com/academa-labs/manimgx/blob/1fb4dd7327bf5ed7d0c40d022c05cb816455ae1d/examples/miura_ori.py
# Cell geometry: Mark Schenk and Simon D. Guest, https://doi.org/10.1073/pnas.1217998110
# Noon keeps the 14 × 12 sheet and the original choreography. Rigid panel,
# crease, and camera poses are evaluated at authoring time.
"""Miura-ori: one pull folds the whole sheet.

A sheet creased into identical parallelograms (sides a, b, angle γ) folds rigidly — no facet bends
or stretches — with a single degree of freedom, the fold angle θ. With Schenk & Guest's unit cell
(PNAS 2013): H = a sin θ sin γ, S = b cos θ tan γ / √(1 + cos²θ tan²γ),
L = a √(1 − sin²θ sin²γ), V = b / √(1 + cos²θ tan²γ), the vertex (i, j) sits at
(i S, j L + (i mod 2) V, (j mod 2) H): every edge keeps its length and every facet stays flat (checked
at build time). Width and length shrink together, a negative Poisson ratio ν = −tan²γ cos²θ, which is
why Koryo Miura's fold opens a satellite's solar array with one pull (Space Flyer Unit, 1995).
"""

from math import atan2, cos, hypot, sin

import numpy as np

from noon import *


def camera_axes(phi: float, theta: float, gamma: float) -> np.ndarray:
    """The camera's axes for its orbit angles, as rows: right, up, and toward the viewer."""

    def spin(a: float) -> np.ndarray:  # a turn about z
        return np.array(
            [[np.cos(a), -np.sin(a), 0.0], [np.sin(a), np.cos(a), 0.0], [0.0, 0.0, 1.0]]
        )

    c, s = np.cos(phi), np.sin(phi)
    tilt = np.array(
        [[1.0, 0.0, 0.0], [0.0, c, s], [0.0, -s, c]]
    )  # a turn by −φ about x
    return spin(gamma) @ tilt @ spin(-theta - np.pi / 2)


A, B, GAMMA = 1.0, 1.0, np.radians(60)  # facet sides and angle
COLS, ROWS = 14, 12  # facets across (b edges) and along (a edges)
SCALE = 0.44  # sheet units → scene units
PAPER, INK = "#d3c8ae", "#2f4b7c"  # the sheet, and alternate columns once it folds
MOUNTAIN, VALLEY = "#d62828", "#1d5fd8"


def cell(theta: float) -> tuple[float, float, float, float]:
    """Schenk & Guest's unit cell at fold angle θ: height H, zigzag step S, row pitch L, offset V."""
    s, c = np.sin(theta), np.cos(theta)
    k = np.sqrt(1 + (c * np.tan(GAMMA)) ** 2)
    return (
        A * s * np.sin(GAMMA),
        B * c * np.tan(GAMMA) / k,
        A * np.sqrt(1 - (s * np.sin(GAMMA)) ** 2),
        B / k,
    )


def sheet(theta: float) -> np.ndarray:
    """The folded sheet's vertices (COLS + 1, ROWS + 1, 3), in sheet units."""
    h, s, pitch, v = cell(theta)
    i, j = np.meshgrid(np.arange(COLS + 1), np.arange(ROWS + 1), indexing="ij")
    return np.stack([i * s, j * pitch + (i % 2) * v, (j % 2) * h], -1).astype(float)


def rigidity(theta: float) -> float:
    """The largest error, at fold angle θ, in any edge length or in any facet's flatness."""
    p = sheet(theta)
    across = np.linalg.norm(p[1:] - p[:-1], axis=-1) - B
    along = np.linalg.norm(p[:, 1:] - p[:, :-1], axis=-1) - A
    o, x, y, far = p[:-1, :-1], p[1:, :-1], p[:-1, 1:], p[1:, 1:]
    normal = np.cross(x - o, y - o)
    normal /= np.linalg.norm(normal, axis=-1, keepdims=True)
    warp = np.einsum("ijk,ijk->ij", far - o, normal)
    return float(max(np.abs(across).max(), np.abs(along).max(), np.abs(warp).max()))


def facets(p: np.ndarray) -> np.ndarray:
    """Each facet's own four corners (COLS · ROWS · 4, 3): unshared, so every facet is flat-lit."""
    quads = np.stack([p[:-1, :-1], p[1:, :-1], p[1:, 1:], p[:-1, 1:]], 2)
    return quads.reshape(-1, 3)


def creases() -> tuple[np.ndarray, np.ndarray]:
    """Every interior crease as (vertex a, vertex b, facet 1, facet 2) index rows into the
    vertex grid and the facet list, and whether it is a mountain (seen from above)."""
    vid = np.arange((COLS + 1) * (ROWS + 1)).reshape(COLS + 1, ROWS + 1)
    fid = np.arange(COLS * ROWS).reshape(COLS, ROWS)
    rows = []
    for i in range(COLS):  # zigzag creases between facet rows j − 1 and j
        for j in range(1, ROWS):
            rows.append([vid[i, j], vid[i + 1, j], fid[i, j - 1], fid[i, j]])
    for i in range(1, COLS):  # creases between facet columns i − 1 and i
        for j in range(ROWS):
            rows.append([vid[i, j], vid[i, j + 1], fid[i - 1, j], fid[i, j]])
    index = np.array(rows)
    # a crease is a mountain where it stands above the midpoint of its two facets' centers
    p = sheet(np.radians(10)).reshape(-1, 3)
    centers = facets(sheet(np.radians(10))).reshape(-1, 4, 3).mean(axis=1)
    middle = (p[index[:, 0]] + p[index[:, 1]]) / 2
    return index, middle[:, 2] > (centers[index[:, 2], 2] + centers[index[:, 3], 2]) / 2


def ribbons(p: np.ndarray, index: np.ndarray, width: np.ndarray) -> np.ndarray:
    """Strips `width` wide (one per crease) along the creases, half on each of the two facets,
    lifted a hair off them: (creases · 8, 3) vertices, 4 per half."""
    grid = p.reshape(-1, 3)
    corners = facets(p).reshape(-1, 4, 3)
    a, b = grid[index[:, 0]], grid[index[:, 1]]
    halves = []
    for side in (2, 3):
        center = corners[index[:, side]].mean(axis=1)
        normal = np.cross(
            corners[index[:, side], 1] - corners[index[:, side], 0],
            corners[index[:, side], 3] - corners[index[:, side], 0],
        )
        normal /= np.linalg.norm(normal, axis=1, keepdims=True)
        along = (b - a) / np.linalg.norm(b - a, axis=1, keepdims=True)
        inward = center - a - ((center - a) * along).sum(axis=1, keepdims=True) * along
        inward /= np.linalg.norm(inward, axis=1, keepdims=True)
        lift = 0.012 * normal
        side_edge = width[:, None] * inward
        halves.append(np.stack([a, b, b + side_edge, a + side_edge], 1) + lift[:, None])
    return np.concatenate(halves, 1).reshape(-1, 3)


FACES = ((0, 1, 2), (0, 2, 3))
FOCAL_DISTANCE = 20
SPOT = np.array([2.5, -0.3])
WIDTH, LENGTH = "#8ecae6", "#ffb703"
FONT = "NewComputerModern"


def pose(points):
    # Local X is the first edge. The local Y component of the second edge
    # fixes the plane, including its choice of normal.
    a, b, _, d = points
    x = b - a
    width = np.linalg.norm(x)
    x = x / width
    y = d - a - np.dot(d - a, x) * x
    height = np.linalg.norm(y)
    y = y / height
    yaw = atan2(x[1], x[0])
    pitch = -atan2(x[2], hypot(x[0], x[1]))
    basis_y = np.array([-sin(yaw), cos(yaw), 0.0])
    basis_z = np.cross(x, basis_y)
    roll = atan2(np.dot(y, basis_z), np.dot(y, basis_y))
    cy, sy = cos(yaw / 2), sin(yaw / 2)
    cp, sp = cos(pitch / 2), sin(pitch / 2)
    cr, sr = cos(roll / 2), sin(roll / 2)
    return tuple(a), (cy*cp*cr + sy*sp*sr, cy*cp*sr - sy*sp*cr,
                      cy*sp*cr + sy*cp*sr, sy*cp*cr - cy*sp*sr), width, height


def ease(t, start, end):
    alpha = float(np.clip((t-start)/(end-start), 0, 1))
    return alpha*alpha*(3-2*alpha)


def fold_angle(t):
    return 82*ease(t, 5.3, 13.8) - 54*ease(t, 15.7, 22.2) + 17*ease(t, 22.8, 25.8)


def patch(angle):
    points = sheet(np.radians(angle))
    grid = points.reshape(-1, 3)
    return SCALE * (points - (grid.min(axis=0) + grid.max(axis=0)) / 2)


def span(points):
    return np.array([points[-1, 0, 0] - points[0, 0, 0],
                     points[0, -1, 1] - points[0, 0, 1]])


def camera(t, points, flat_diagonal):
    phi, theta = np.radians(54), np.radians(-104) + 0.03*t
    zoom = 0.95 * (flat_diagonal / np.linalg.norm(span(points))) ** 0.85
    axes = camera_axes(phi, theta, 0)
    center = -(SPOT @ axes[:2]) / zoom
    return phi, theta, zoom, axes, center


def projected_arrow(points, view, axis, side):
    _, _, zoom, axes, center = view
    grid = points.reshape(-1, 3)
    low, high = grid.min(axis=0), grid.max(axis=0)
    start = (low + high) / 2
    start[axis] = (low if side < 0 else high)[axis] + side * 0.15
    end = start.copy()
    end[axis] += side * 0.9
    q = (np.array([start, end]) - center) @ axes.T
    xy = q[:, :2] * (zoom / (1 - q[:, 2] / FOCAL_DISTANCE))[:, None]
    return xy[0], xy[1]


def leaves(mob):
    if isinstance(mob, (Group, VGroup)):
        return [leaf for part in mob for leaf in leaves(part)]
    return [mob]


def left_at(mob, x, y):
    return mob.move_to((x + mob.width/2, y, 0))


def mesh(vert, color, *, point_lit=True):
    return Mesh3D.polyhedron(vert, FACES, color=color_from_hex(color), point_lit=point_lit)


def crease_meshes(t, points, index, across, mountain):
    fade = 1 - np.clip(3*(t-5.3)/8.5, 0, 1)
    sweep = ease(t, 1, 2.8)
    width = 0.03 * fade * np.clip((1.3*sweep-across)/0.3, 0, 1)
    quads = ribbons(points, index, np.maximum(width, 0.00001)).reshape(-1, 4, 3)
    normal_z = np.cross(quads[:, 1]-quads[:, 0], quads[:, 3]-quads[:, 0])[:, 2]
    reversed_quads = normal_z < 0
    quads[reversed_quads] = quads[reversed_quads][:, [1, 0, 3, 2]]
    colors = np.repeat(mountain, 2)
    meshes = []
    for color, mask in ((MOUNTAIN, colors), (VALLEY, ~colors)):
        faces = quads[mask]
        # Keep red and blue at distinct depths where their ribbons cross.
        # Otherwise coplanar fragments can change order after a replay seek.
        if color == MOUNTAIN:
            faces[:, :, 2] += 0.008
        triangles = np.arange(len(faces))[:, None, None]*4 + np.array(FACES)
        meshes.append(Mesh3D.polyhedron(faces.reshape(-1, 3), triangles.reshape(-1, 3),
                                       color=color_from_hex(color)))
    return meshes


class MiuraOri(ThreeDScene):
    def clock_fade(self, mob, clock, keys):
        self.add_fixed_in_frame_mobjects(mob)
        for leaf in leaves(mob):
            self.bind_sampled(leaf, clock, "opacity", keys)

    def construct(self):
        # Use the same finite time boundaries as the upstream scene.
        boundaries = (0, 1, 2.8, 3.8, 4.6, 4.9, 5.3, 8.1333333333, 13.8, 14.8,
                      15.1, 15.7, 22.2, 22.8, 25.8, 30.5)
        times = [0]
        for start, end in zip(boundaries, boundaries[1:]):
            # Use enough endpoints for the tint change at the start of folding.
            count = int(np.ceil((end-start)*(8 if 5.3 <= start <= 8.1333333333 else 4)))
            times.extend(np.linspace(start, end, count+1)[1:])
        angle = [fold_angle(t) for t in times]
        grid = [patch(value) for value in angle]
        flat_diagonal = np.linalg.norm(span(grid[0]))
        views = [camera(t, points, flat_diagonal) for t, points in zip(times, grid)]
        clock = self.value_tracker(0)

        phi, theta, zoom, _, center = views[0]
        self.set_camera_orientation(phi=phi, theta=theta, focal_distance=FOCAL_DISTANCE,
                                    zoom=zoom, frame_center=center)
        background = mesh([(-40, -40, -4), (40, -40, -4), (40, 40, -4), (-40, 40, -4)],
                          "#0b1020", point_lit=False)
        self.add_world_mobjects(background, self.point_light(position=(-6, -9, 7), intensity=1))

        # Each panel remains rigid. Share the two mirrored corner meshes
        # across the sheet and batch the sampled ink tints as family paint.
        panels = []
        base_quads = facets(grid[0]).reshape(-1, 4, 3)
        templates = []
        width, height = B*SCALE, A*SCALE*np.sin(GAMMA)
        for sign in (1, -1):
            slant = sign*A*SCALE*np.cos(GAMMA)
            templates.append(mesh([(0, 0, 0), (width, 0, 0), (width+slant, height, 0), (slant, height, 0)], PAPER))
        for k, corners in enumerate(base_quads):
            translation, rotation, _, _ = pose(corners)
            panel = templates[k // ROWS % 2].copy()
            self.add_world_mobjects(panel)
            self.set_world_transform(panel, translation=translation, rotation=rotation)
            panels.append(panel)
        ink_panels = Group(*(panel for k, panel in enumerate(panels) if (k // ROWS) % 2))
        paper_color, ink_color = color_from_hex(PAPER), color_from_hex(INK)
        previous_tint = 0

        index, mountain = creases()
        flat = grid[0].reshape(-1, 3)
        across = (flat[index[:, 0], :2] + flat[index[:, 1], :2]).sum(axis=1)
        across = (across-across.min())/(across.max()-across.min())
        # All crease ribbons of one color share a mesh revision. This
        # preserves the drawn sweep and the folds without giving every strip
        # its own world-pose history.
        strips = crease_meshes(0, grid[0], index, across, mountain)
        self.add_world_mobjects(*strips)
        strip_frames = {i: crease_meshes(t, grid[i], index, across, mountain)
                        for i, t in enumerate(times) if 1 < t <= 2.8 or 5.3 < t < 8.1333333333}

        title = Text("Miura-ori", font_size=40, font=FONT).to_corner(UL)
        self.add_fixed_in_frame_mobjects(title)
        subtitles = (
            ("a sheet creased into identical parallelograms", [(0, 0), (1, 1), (4.6, 1), (4.9, 0)]),
            ("one angle folds every crease at once, rigidly", [(0, 0), (4.9, 0), (5.3, 1), (14.8, 1), (15.1, 0)]),
            ("pull it apart one way: it opens the other way too", [(0, 0), (15.1, 0), (15.7, 1)]),
        )
        for text, keys in subtitles:
            subtitle = Text(text, font_size=24, color=GREY_B, font=FONT).next_to(title, DOWN, aligned_edge=LEFT, buff=0.15)
            self.clock_fade(subtitle, clock, keys)

        legend = VGroup(*(VGroup(Line(ORIGIN, 0.5*RIGHT, stroke_width=6, color=color_from_hex(color)),
                                 Text(name, font_size=22, font=FONT)).arrange(RIGHT, buff=0.2)
                          for name, color in (("mountain fold", MOUNTAIN), ("valley fold", VALLEY))))
        legend.arrange(DOWN, aligned_edge=LEFT, buff=0.2).move_to((-6.7, 2.15, 0), aligned_edge=LEFT)
        self.clock_fade(legend, clock, [(0, 0), (1, 0), (2.8, 1), (5.3, 1), (13.8, 0)])

        panel_fade = [(0, 0), (2.8, 0), (3.8, 1)]
        track = Line((-6.7, 1.2), (-3.7, 1.2), stroke_width=3, color=GREY_C)
        knob = Dot(radius=0.09)
        self.clock_fade(VGroup(track, knob), clock, panel_fade)
        self.bind_sampled(knob, clock, "position", [(t, -6.7 + 3*value/90, 1.2) for t, value in zip(times, angle)])

        values = np.array([span(points) / span(grid[0]) for points in grid])
        meters = []
        for k, (name, color) in enumerate((("width", WIDTH), ("length", LENGTH))):
            y = 0.45 - 0.55*k
            paint = color_from_hex(color)
            word = left_at(Text(name, font_size=22, font=FONT), -6.7, y)
            bar = Rectangle(width=2.2, height=0.22, color=paint, fill_opacity=0.9, stroke_width=0)
            number = Variable(100, "", num_decimal_places=0, unit=r"\%", font_size=22)
            self.clock_fade(VGroup(word, bar, number.value), clock, panel_fade)
            self.bind_sampled(bar, clock, "scale", [(t, ratio, 1) for t, ratio in zip(times, values[:, k])])
            self.bind_sampled(bar, clock, "position", [(t, -5.45 + 1.1*ratio, y) for t, ratio in zip(times, values[:, k])])
            text_offset = -5.3 + number.value.width/2
            self.bind_sampled(number.value, clock, "position", [(t, text_offset + 2.2*ratio, y)
                                                                 for t, ratio in zip(times, values[:, k])])
            meters.append(number.tracker)

        angle_number = Variable(0, "", num_decimal_places=0, unit=r"{}^\circ", font_size=30)
        theta_label = MathTex(r"\theta =", font_size=30).next_to(track, RIGHT, buff=0.3)
        angle_number.value.next_to(theta_label, RIGHT, buff=0.1)
        self.clock_fade(VGroup(theta_label, angle_number.value), clock, panel_fade)
        nu_label = MathTex(r"\nu = -\frac{d L / L}{d W / W} =", font_size=32)
        left_at(nu_label, -6.7, -0.95)
        nu_number = Variable(-3, "", num_decimal_places=2, font_size=32)
        nu_number.value.next_to(nu_label, RIGHT, buff=0.12)
        self.clock_fade(VGroup(nu_label, nu_number.value), clock, panel_fade)

        law = MathTex(r"\nu = -\tan^2\gamma\,\cos^2\theta", font_size=32)
        left_at(law, -6.7, -1.8)
        self.clock_fade(law, clock, [(0, 0), (15.7, 0), (22.2, 1)])
        error = max(rigidity(np.radians(deg)) for deg in range(90))
        mantissa, power = f"{error:.0e}".split("e")
        note = MathTex(rf"\text{{rigid: edges and flatness exact to }} {mantissa} \times 10^{{{int(power)}}}",
                       font_size=24, color=GREY_B)
        left_at(note, -6.7, -2.45)
        closing = VGroup(Text("Koryo Miura's fold opens", font_size=28, font=FONT),
                         Text("a solar array with one pull", font_size=28, font=FONT))
        closing.arrange(DOWN, aligned_edge=LEFT, buff=0.12).move_to((-6.7, -3.3, 0), aligned_edge=LEFT)
        self.clock_fade(VGroup(note, closing), clock, [(0, 0), (22.8, 0), (25.8, 1)])

        pull_opacity = [(0, 0), (15.1, 0), (15.7, 1), (22.2, 1), (22.8, 0)]
        for axis, color in ((0, WIDTH), (1, LENGTH)):
            for side in (-1, 1):
                arrow = Arrow(ORIGIN, RIGHT, buff=0, stroke_width=6, color=color_from_hex(color))
                self.clock_fade(arrow, clock, pull_opacity)
                endpoints = [projected_arrow(points, view, axis, side) for points, view in zip(grid, views)]
                for part in arrow:
                    positions, rotations, scales = [], [], []
                    turns = np.unwrap([atan2(*(b-a)[::-1]) for a, b in endpoints])
                    for t, turn, (a, b) in zip(times, turns, endpoints):
                        scale = float(np.linalg.norm(b-a))
                        positions.append((t, a[0], a[1]))
                        rotations.append((t, turn))
                        scales.append((t, scale, scale))
                    self.bind_sampled(part, clock, "position", positions)
                    self.bind_sampled(part, clock, "rotation", rotations)
                    self.bind_sampled(part, clock, "scale", scales)

        sections = {0: "Draw the mountain and valley creases", 4.6: "Fold the sheet",
                    14.8: "Pull both dimensions open", 22.8: "The rigid fold"}
        self.next_section(sections[0])
        previous = 0
        for i, t in enumerate(times[1:], 1):
            if previous in sections and previous != 0:
                self.next_section(sections[previous])
            motions = [clock.animate.set_value(t)]
            if angle[i] != angle[i-1]:
                tint = ease(angle[i], 8, 38)
                if tint != previous_tint:
                    ink_panels.set_color(Color(
                        (1-tint)*paper_color.red + tint*ink_color.red,
                        (1-tint)*paper_color.green + tint*ink_color.green,
                        (1-tint)*paper_color.blue + tint*ink_color.blue,
                    ))
                    previous_tint = tint
                for face, corners in zip(panels, facets(grid[i]).reshape(-1, 4, 3)):
                    position, rotation, _, _ = pose(corners)
                    motions.append(WorldTransformTo(face, translation=position, rotation=rotation))
                motions.extend((angle_number.tracker.animate.set_value(angle[i]),
                                nu_number.tracker.animate.set_value(float(-np.tan(GAMMA)**2*np.cos(np.radians(angle[i]))**2)),
                                *(tracker.animate.set_value(100*ratio) for tracker, ratio in zip(meters, values[i]))))
            if i in strip_frames:
                for strip, keyframe in zip(strips, strip_frames[i]):
                    self.replace_content(strip, keyframe)
            phi, theta, zoom, _, center = views[i]
            self.move_camera(phi=phi, theta=theta, zoom=zoom, frame_center=center,
                             added_anims=motions, run_time=t-previous, rate_func=linear)
            if strips and t >= 8.1333333333:
                self.remove(*strips)
                strips.clear()
            previous = t
