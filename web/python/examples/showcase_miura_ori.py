"""Noon adaptation of Academa's ManimGX Miura-ori demonstration.

https://manimgx.academa.ai/gallery/miura-ori/
Koryo Miura's fold uses the rigid cell geometry described by Mark Schenk and
Simon D. Guest in https://doi.org/10.1073/pnas.1217998110.
The mesh poses use that geometry at retained keyframes. Playback interpolates
between those poses without running the fold equations in a Python callback.
"""
from math import atan2, cos, pi, sin, sqrt, tan

from noon import *


COLUMNS, ROWS = 6, 4
SIDE = 0.92
GAMMA = pi / 3
FACES = ((0, 1, 2), (0, 2, 3))


def patch_geometry(angle):
    # With one angle the zigzag pitch and the height of each ridge are fixed.
    rise = SIDE * sin(angle) * sin(GAMMA)
    pitch = sqrt(SIDE * SIDE - rise * rise)
    slope = cos(angle) * tan(GAMMA)
    offset = SIDE / sqrt(1 + slope * slope)
    step = slope * offset
    return rise, pitch, offset, step


def panel_pose(i, j, geometry):
    rise, pitch, offset, step = geometry
    column_sign = (-1) ** i
    row_sign = (-1) ** j
    center_x = COLUMNS * step / 2
    center_y = (ROWS * pitch + offset) / 2
    # Align the local X axis with the first edge, then tip the local Y
    # axis so that it spans the panel together with the second edge.
    yaw = atan2(column_sign * offset, step)
    roll = atan2(row_sign * rise, pitch * cos(yaw))
    cy, sy = cos(yaw / 2), sin(yaw / 2)
    cr, sr = cos(roll / 2), sin(roll / 2)
    return (
        (i * step - center_x, j * pitch + (i % 2) * offset - center_y, (j % 2 - 0.5) * rise),
        (cy * cr, cy * sr, sy * sr, sy * cr),
    )


def label(text, y, size=25, color=GREY_B):
    return Text(text, font_size=size, color=color).move_to((0, y, 0))


class MiuraOri(ThreeDScene):
    def reveal(self, *labels):
        for mob in labels:
            mob.set_opacity(0)
        self.add_fixed_in_frame_mobjects(*labels)
        return [mob.animate.set_opacity(1) for mob in labels]

    def fold(self, panels, start, end, seconds):
        steps = 24
        for k in range(1, steps + 1):
            t = k / steps
            geometry = patch_geometry(start + (end - start) * t * t * (3 - 2 * t))
            poses = []
            for panel, i, j in panels:
                translation, rotation = panel_pose(i, j, geometry)
                poses.append(WorldTransformTo(panel, translation=translation, rotation=rotation))
            self.play(*poses, run_time=seconds / steps, rate_func=linear)

    def construct(self):
        self.next_section("A folded solar panel")
        folded, opened = 1.37, 0.08
        self.set_camera_orientation(phi=1.01, theta=-1.12, zoom=1.04, focal_distance=10)
        self.add_world_mobjects(self.point_light(position=(-3, -5, 7), intensity=1))
        panels = []
        geometry = patch_geometry(folded)
        for i in range(COLUMNS):
            slant = (-1) ** i * SIDE * cos(GAMMA)
            depth = SIDE * sin(GAMMA)
            vertices = ((0, 0, 0), (SIDE, 0, 0), (SIDE + slant, depth, 0), (slant, depth, 0))
            for j in range(ROWS):
                color = GOLD if (i, j) == (2, 1) else (BLUE_C if (i + j) % 2 else BLUE_E)
                panel = Mesh3D.polyhedron(vertices, FACES, color=color, point_lit=True)
                self.add_world_mobjects(panel)
                translation, rotation = panel_pose(i, j, geometry)
                self.set_world_transform(panel, translation=translation, rotation=rotation)
                panels.append((panel, i, j))

        heading = label("Can one pull open a solar array?", 3.12, 40, WHITE)
        note = label("A Miura fold packs a wide sheet into a narrow bundle.", -3.05)
        self.play(*self.reveal(heading, note), run_time=0.8)
        self.wait(1.25)

        self.next_section("Follow one panel")
        crease_note = label("Watch the gold panel turn at the creases.", -3.05)
        self.move_camera(phi=0.73, theta=-1.36, zoom=0.94, run_time=1.25,
                         added_anims=(FadeOut(note), *self.reveal(crease_note)))
        halfway = 0.91
        self.fold(panels, folded, halfway, 2.25)
        self.wait(1.0)

        self.next_section("Open the hinged sheet")
        shift_note = label("Opening one angle pushes the linked rows and columns apart.", -3.05)
        self.play(FadeOut(crease_note), *self.reveal(shift_note), run_time=0.55)
        self.fold(panels, halfway, opened, 3.0)
        self.move_camera(phi=0.30, theta=-1.56, zoom=0.92, run_time=1.5)
        self.wait(1.0)

        self.next_section("Reverse the motion")
        finish = label("The same linked creases fold it back for storage.", -3.05, color=TEAL_A)
        self.move_camera(phi=0.82, theta=-1.21, zoom=0.98, run_time=1.2,
                         added_anims=(FadeOut(shift_note), *self.reveal(finish)))
        self.fold(panels, opened, folded, 3.0)
        self.wait(1.8)
