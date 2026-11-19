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

from math import atan2, ceil, cos, hypot, pi, radians, sin, sqrt, tan

from noon import *


A, B, GAMMA = 1.0, 1.0, radians(60)  # facet sides and angle
COLS, ROWS = 14, 12  # facets across (b edges) and along (a edges)
SCALE = 0.44  # sheet units → scene units
PAPER, INK = "#d3c8ae", "#2f4b7c"  # the sheet, and alternate columns once it folds
MOUNTAIN, VALLEY = "#d62828", "#1d5fd8"
FACES = ((0, 1, 2), (0, 2, 3))
FOCAL_DISTANCE = 20
SPOT = (2.5, -0.3)
WIDTH, LENGTH = "#8ecae6", "#ffb703"
FONT = "NewComputerModern"


def vector_add(a, b):
    return tuple(x + y for x, y in zip(a, b))


def vector_subtract(a, b):
    return tuple(x - y for x, y in zip(a, b))


def vector_scale(a, scalar):
    return tuple(x * scalar for x in a)


def dot_product(a, b):
    return sum(x * y for x, y in zip(a, b))


def cross_product(a, b):
    ax, ay, az = a
    bx, by, bz = b
    return ay*bz - az*by, az*bx - ax*bz, ax*by - ay*bx


def unit_direction(a):
    return vector_scale(a, 1 / sqrt(dot_product(a, a)))


def center_of(points):
    return tuple(sum(axis) / len(axis) for axis in zip(*points))


def camera_axes(phi, theta):
    """Camera axes as rows: right, up, and toward the viewer."""
    c, s = cos(-theta - pi / 2), sin(-theta - pi / 2)
    cp, sp = cos(phi), sin(phi)
    return (c, -s, 0.0), (cp*s, cp*c, sp), (-sp*s, -sp*c, cp)


def cell(theta):
    """Schenk & Guest's unit cell: height, zigzag step, row pitch, offset."""
    s, c = sin(theta), cos(theta)
    k = sqrt(1 + (c * tan(GAMMA)) ** 2)
    return (
        A * s * sin(GAMMA),
        B * c * tan(GAMMA) / k,
        A * sqrt(1 - (s * sin(GAMMA)) ** 2),
        B / k,
    )


def sheet(theta):
    """The folded sheet's (COLS + 1) × (ROWS + 1) vertices, in sheet units."""
    h, s, pitch, v = cell(theta)
    return [[(i * s, j * pitch + (i % 2) * v, (j % 2) * h)
             for j in range(ROWS + 1)] for i in range(COLS + 1)]


def facets(points):
    """Four corners per facet, ordered consistently for flat lighting."""
    return [(points[i][j], points[i+1][j], points[i+1][j+1], points[i][j+1])
            for i in range(COLS) for j in range(ROWS)]


def rigidity(theta):
    """Largest error in edge lengths or facet flatness at this fold angle."""
    error = 0
    for a, b, c, d in facets(sheet(theta)):
        along_b = vector_subtract(b, a)
        along_a = vector_subtract(d, a)
        normal = unit_direction(cross_product(along_b, along_a))
        error = max(error, abs(hypot(*along_b) - B), abs(hypot(*along_a) - A),
                    abs(dot_product(vector_subtract(c, a), normal)))
    return error


def creases():
    """Interior crease endpoints, adjacent facet indices, and mountain/valley sign."""
    edges = []
    for i in range(COLS):
        for j in range(1, ROWS):
            edges.append((i, j, i+1, j, i*ROWS+j-1, i*ROWS+j))
    for i in range(1, COLS):
        for j in range(ROWS):
            edges.append((i, j, i, j+1, (i-1)*ROWS+j, i*ROWS+j))
    points = sheet(radians(10))
    centers = [center_of(quad) for quad in facets(points)]
    return [(*edge, points[edge[0]][edge[1]][2] + points[edge[2]][edge[3]][2] >
             centers[edge[4]][2] + centers[edge[5]][2]) for edge in edges]


def ribbon(a, b, facet, width, lift):
    """One half of a narrow crease ribbon, lifted just above the facet."""
    o, u, _, v = facet
    normal = unit_direction(cross_product(vector_subtract(u, o), vector_subtract(v, o)))
    along = unit_direction(vector_subtract(b, a))
    toward = vector_subtract(center_of(facet), a)
    inward = unit_direction(vector_subtract(toward, vector_scale(along, dot_product(toward, along))))
    edge = vector_scale(inward, width)
    corners = [a, b, vector_add(b, edge), vector_add(a, edge)]
    if cross_product(along, inward)[2] < 0:
        corners = [corners[1], corners[0], corners[3], corners[2]]
    offset = vector_add(vector_scale(normal, 0.012), (0.0, 0.0, lift))
    return [vector_add(corner, offset) for corner in corners]


def pose(points):
    # Local X is the first edge. The projection of the second edge fixes the
    # local Y axis and therefore the orientation of the panel's normal.
    a, b, _, d = points
    x = unit_direction(vector_subtract(b, a))
    delta = vector_subtract(d, a)
    y = unit_direction(vector_subtract(delta, vector_scale(x, dot_product(delta, x))))
    yaw = atan2(x[1], x[0])
    pitch = -atan2(x[2], hypot(x[0], x[1]))
    basis_y = (-sin(yaw), cos(yaw), 0.0)
    roll = atan2(dot_product(y, cross_product(x, basis_y)), dot_product(y, basis_y))
    cy, sy = cos(yaw / 2), sin(yaw / 2)
    cp, sp = cos(pitch / 2), sin(pitch / 2)
    cr, sr = cos(roll / 2), sin(roll / 2)
    return a, (cy*cp*cr + sy*sp*sr, cy*cp*sr - sy*sp*cr,
               cy*sp*cr + sy*cp*sr, sy*cp*cr - cy*sp*sr)


def ease(t, start, end):
    alpha = max(0, min(1, (t-start)/(end-start)))
    return alpha*alpha*(3-2*alpha)


def fold_angle(t):
    return 82*ease(t, 5.3, 13.8) - 54*ease(t, 15.7, 22.2) + 17*ease(t, 22.8, 25.8)


def bounds(points):
    axes = list(zip(*(p for row in points for p in row)))
    return tuple(min(axis) for axis in axes), tuple(max(axis) for axis in axes)


def patch(angle):
    points = sheet(radians(angle))
    low, high = bounds(points)
    center = vector_scale(vector_add(low, high), 0.5)
    return [[vector_scale(vector_subtract(p, center), SCALE) for p in row] for row in points]


def span(points):
    return points[-1][0][0] - points[0][0][0], points[0][-1][1] - points[0][0][1]


def camera(t, points, flat_diagonal):
    phi, theta = radians(54), radians(-104) + 0.03*t
    zoom = 0.95 * (flat_diagonal / hypot(*span(points))) ** 0.85
    axes = camera_axes(phi, theta)
    center = vector_scale(vector_add(vector_scale(axes[0], SPOT[0]),
                                     vector_scale(axes[1], SPOT[1])), -1 / zoom)
    return phi, theta, zoom, axes, center


def projected_arrow(points, view, axis, side):
    _, _, zoom, axes, center = view
    low, high = bounds(points)
    center_of_sheet = vector_scale(vector_add(low, high), 0.5)
    edge = (low if side < 0 else high)[axis] + side * 0.15
    start = tuple(edge if i == axis else value for i, value in enumerate(center_of_sheet))
    end = tuple(value + side * 0.9 if i == axis else value for i, value in enumerate(start))
    xy = []
    for point in (start, end):
        q = vector_subtract(point, center)
        perspective = zoom / (1 - dot_product(axes[2], q) / FOCAL_DISTANCE)
        xy.append((perspective * dot_product(axes[0], q), perspective * dot_product(axes[1], q)))
    return xy


def unwrap_turns(endpoints):
    turns = []
    previous = 0.0
    for a, b in endpoints:
        angle = atan2(b[1]-a[1], b[0]-a[0])
        if turns:
            angle = previous + (angle - previous + pi) % (2*pi) - pi
        turns.append(angle)
        previous = angle
    return turns


def leaves(mob):
    if isinstance(mob, (Group, VGroup)):
        return [leaf for part in mob for leaf in leaves(part)]
    return [mob]


def left_at(mob, x, y):
    return mob.move_to((x + mob.width/2, y, 0))


def mesh(vert, color, *, point_lit=True):
    return Mesh3D.polyhedron(vert, FACES, color=color_from_hex(color), point_lit=point_lit)


def crease_meshes(t, points, edges, across):
    fade = 1 - max(0, min(1, 3*(t-5.3)/8.5))
    sweep = ease(t, 1, 2.8)
    quads = facets(points)
    meshes = []
    for is_mountain, color in ((True, MOUNTAIN), (False, VALLEY)):
        vertices = []
        triangles = []
        lift = 0.008 if is_mountain else 0  # Order antialiased overlaps consistently.
        for edge, position in zip(edges, across):
            i, j, k, l, left, right, mountain = edge
            if mountain != is_mountain:
                continue
            width = max(0.00001, 0.03 * fade * max(0, min(1, (1.3*sweep-position)/0.3)))
            for side in (left, right):
                offset = len(vertices)
                vertices.extend(ribbon(points[i][j], points[k][l], quads[side], width, lift))
                triangles.extend((a+offset, b+offset, c+offset) for a, b, c in FACES)
        meshes.append(Mesh3D.polyhedron(vertices, triangles, color=color_from_hex(color)))
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
            count = ceil((end-start)*(8 if 5.3 <= start <= 8.1333333333 else 4))
            step = (end-start) / count
            times.extend(start + step*k for k in range(1, count))
            times.append(end)
        angle = [fold_angle(t) for t in times]
        grid = [patch(value) for value in angle]
        flat_diagonal = hypot(*span(grid[0]))
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
        base_quads = facets(grid[0])
        templates = []
        width, height = B*SCALE, A*SCALE*sin(GAMMA)
        for sign in (1, -1):
            slant = sign*A*SCALE*cos(GAMMA)
            templates.append(mesh([(0, 0, 0), (width, 0, 0), (width+slant, height, 0), (slant, height, 0)], PAPER))
        for k, corners in enumerate(base_quads):
            translation, rotation = pose(corners)
            panel = templates[k // ROWS % 2].copy()
            self.add_world_mobjects(panel)
            self.set_world_transform(panel, translation=translation, rotation=rotation)
            panels.append(panel)
        ink_panels = Group(*(panel for k, panel in enumerate(panels) if (k // ROWS) % 2))
        paper_color, ink_color = color_from_hex(PAPER), color_from_hex(INK)
        previous_tint = 0

        edges = creases()
        flat = grid[0]
        diagonal_sums = [sum(flat[i][j][:2]) + sum(flat[k][l][:2])
                         for i, j, k, l, _, _, _ in edges]
        start, end = min(diagonal_sums), max(diagonal_sums)
        across = [(x-start)/(end-start) for x in diagonal_sums]
        # All crease ribbons of one color share a mesh revision. This
        # preserves the drawn sweep and the folds without giving every strip
        # its own world-pose history.
        strips = crease_meshes(0, grid[0], edges, across)
        self.add_world_mobjects(*strips)
        strip_frames = {i: crease_meshes(t, grid[i], edges, across)
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

        flat_width, flat_length = span(grid[0])
        values = [(w/flat_width, l/flat_length) for w, l in (span(points) for points in grid)]
        meters = []
        for k, (name, color) in enumerate((("width", WIDTH), ("length", LENGTH))):
            y = 0.45 - 0.55*k
            paint = color_from_hex(color)
            word = left_at(Text(name, font_size=22, font=FONT), -6.7, y)
            bar = Rectangle(width=2.2, height=0.22, color=paint, fill_opacity=0.9, stroke_width=0)
            number = Variable(100, "", num_decimal_places=0, unit=r"\%", font_size=22)
            self.clock_fade(VGroup(word, bar, number.value), clock, panel_fade)
            self.bind_sampled(bar, clock, "scale", [(t, ratio, 1) for t, ratio in zip(times, [pair[k] for pair in values])])
            self.bind_sampled(bar, clock, "position", [(t, -5.45 + 1.1*ratio, y) for t, ratio in zip(times, [pair[k] for pair in values])])
            text_offset = -5.3 + number.value.width/2
            self.bind_sampled(number.value, clock, "position", [(t, text_offset + 2.2*ratio, y)
                                                                 for t, ratio in zip(times, [pair[k] for pair in values])])
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
        if max(rigidity(radians(deg)) for deg in range(90)) > 3.5e-15:
            raise ValueError("The Miura-ori facets must remain rigid")
        note = MathTex(r"\text{rigid: edges and flatness exact to } 3 \times 10^{-15}",
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
                    turns = unwrap_turns(endpoints)
                    for t, turn, (a, b) in zip(times, turns, endpoints):
                        scale = hypot(b[0]-a[0], b[1]-a[1])
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
                for face, corners in zip(panels, facets(grid[i])):
                    position, rotation = pose(corners)
                    motions.append(WorldTransformTo(face, translation=position, rotation=rotation))
                motions.extend((angle_number.tracker.animate.set_value(angle[i]),
                                nu_number.tracker.animate.set_value(-tan(GAMMA)**2*cos(radians(angle[i]))**2),
                                *(tracker.animate.set_value(100*ratio) for tracker, ratio in zip(meters, values[i]))))
            if i in strip_frames:
                for strip, keyframe in zip(strips, strip_frames[i]):
                    self.replace_content(strip, keyframe)
            phi, theta, zoom, _, center = views[i]
            self.move_camera(phi=phi, theta=theta, zoom=zoom, frame_center=center,
                             added_anims=motions, run_time=t-previous, rate_func=linear)
            if strips and t >= 8.1333333333:
                for strip in strips:
                    self.remove(strip)
                strips.clear()
            previous = t
